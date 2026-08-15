//! Application service: enroll / verify / add-template over a video stream.
//!
//! This is the inbound port. Driving adapters call these methods; they do not
//! reach outbound adapters directly.

use std::sync::Arc;
use std::time::Instant;

use crate::domain::error::DomainError;
use crate::domain::{Embedding, Frame, TemplateBundle, UserId};
use crate::ports::{
    CaptureError, EmbeddingMatcher, FaceAligner, FaceDetector, FaceEmbedder, FacePoseEstimator,
    Health, HealthStatus, LivenessChecker, LivenessError, PoseError, TemplateStore, VideoSession,
    VideoSource,
};

use super::error::AppError;
use super::verification_decision::{VerificationDecider, template_quorum_required};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamLimits {
    pub warmup_discard: u32,
    pub max_frames: u32,
}

impl StreamLimits {
    pub const fn new(warmup_discard: u32, max_frames: u32) -> Self {
        Self {
            warmup_discard,
            max_frames,
        }
    }

    pub fn validate(self) -> Result<Self, CaptureError> {
        if self.max_frames == 0 {
            return Err(CaptureError::Failed(
                "StreamLimits.max_frames must be >= 1".into(),
            ));
        }
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamingPolicy {
    pub enroll: StreamLimits,
    pub verify: StreamLimits,
}

impl Default for StreamingPolicy {
    fn default() -> Self {
        Self {
            enroll: StreamLimits::new(2, 5),
            verify: StreamLimits::new(2, 3),
        }
    }
}

pub struct TrueIdAppDeps {
    pub health: Arc<dyn Health>,
    pub video: Arc<dyn VideoSource>,
    pub detector: Arc<dyn FaceDetector>,
    pub aligner: Arc<dyn FaceAligner>,
    pub pose_estimator: Arc<dyn FacePoseEstimator>,
    pub liveness: Arc<dyn LivenessChecker>,
    pub face_embedder: Arc<dyn FaceEmbedder>,
    pub template_store: Arc<dyn TemplateStore>,
    pub matcher: Arc<dyn EmbeddingMatcher>,
    pub streaming: StreamingPolicy,
}

pub struct TrueIdApp {
    health: Arc<dyn Health>,
    video: Arc<dyn VideoSource>,
    detector: Arc<dyn FaceDetector>,
    aligner: Arc<dyn FaceAligner>,
    pose_estimator: Arc<dyn FacePoseEstimator>,
    liveness: Arc<dyn LivenessChecker>,
    face_embedder: Arc<dyn FaceEmbedder>,
    template_store: Arc<dyn TemplateStore>,
    verification: VerificationDecider,
    streaming: StreamingPolicy,
}

impl TrueIdApp {
    pub fn new(deps: TrueIdAppDeps) -> Self {
        Self {
            health: deps.health,
            video: deps.video,
            detector: deps.detector,
            aligner: deps.aligner,
            pose_estimator: deps.pose_estimator,
            liveness: deps.liveness,
            face_embedder: deps.face_embedder,
            template_store: deps.template_store,
            verification: VerificationDecider::new(deps.matcher.clone()),
            streaming: deps.streaming,
        }
    }

    fn require_healthy(&self) -> Result<(), AppError> {
        match self.health.status() {
            HealthStatus::Healthy => Ok(()),
            HealthStatus::Degraded { reason } => Err(AppError::Unhealthy(reason)),
        }
    }

    fn load_enrolled(&self, user: &UserId) -> Result<TemplateBundle, AppError> {
        self.template_store
            .load_all(user)?
            .filter(|b| b.has_any_enrollment())
            .ok_or(DomainError::NoEnrolledTemplate.into())
    }

    fn open_capture(
        &self,
        limits: StreamLimits,
    ) -> Result<(Box<dyn VideoSession>, StreamLimits), AppError> {
        let limits = limits.validate()?;
        let mut session = self.video.open_session()?;
        for i in 0..limits.warmup_discard {
            session.next_frame()?;
            tracing::trace!(frame = i, "warmup discard");
        }
        Ok((session, limits))
    }

    fn try_align_face_from_frame(&self, frame: &Frame) -> Result<Option<Frame>, AppError> {
        let t0 = Instant::now();

        let Some(det) = self.detector.detect_primary(frame)? else {
            tracing::debug!(
                w = frame.width,
                h = frame.height,
                elapsed_ms = t0.elapsed().as_millis(),
                "pipeline: detect → no face"
            );
            return Ok(None);
        };
        tracing::debug!(
            w = frame.width,
            h = frame.height,
            bbox = ?det.bbox,
            has_landmarks = det.landmarks.is_some(),
            "pipeline: detect → face"
        );

        let t_align = Instant::now();
        let aligned: Frame = self.aligner.align(frame, &det)?;
        tracing::trace!(
            elapsed_ms = t_align.elapsed().as_millis(),
            "pipeline: align ok"
        );

        match self.pose_estimator.check_frontal(&aligned, &det) {
            Ok(()) => {}
            Err(PoseError::NotFrontal) => {
                tracing::info!(
                    w = frame.width,
                    h = frame.height,
                    elapsed_ms = t0.elapsed().as_millis(),
                    "discarding frame: pose not frontal (head not toward camera)"
                );
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }

        Ok(Some(aligned))
    }

    fn try_embed_from_frame(&self, frame: &Frame) -> Result<Option<Embedding>, AppError> {
        let t0 = Instant::now();
        let Some(aligned) = self.try_align_face_from_frame(frame)? else {
            return Ok(None);
        };
        match self.liveness.verify_live(&aligned) {
            Ok(()) => {}
            Err(LivenessError::NotLive) => {
                tracing::debug!(
                    elapsed_ms = t0.elapsed().as_millis(),
                    "pipeline: liveness → not live"
                );
                return Ok(None);
            }
            Err(e) => return Err(e.into()),
        }

        let t_emb = Instant::now();
        let emb = self.face_embedder.embed(&aligned)?;
        let summ = emb.summary();
        tracing::debug!(
            dim = emb.dim(),
            embed_ms = t_emb.elapsed().as_millis(),
            total_ms = t0.elapsed().as_millis(),
            probe_min = summ.min,
            probe_max = summ.max,
            probe_mean = summ.mean,
            probe_l2 = summ.l2_norm,
            "pipeline: embed ok"
        );
        Ok(Some(emb))
    }

    fn collect_embeddings(
        &self,
        op: &'static str,
        limits: StreamLimits,
    ) -> Result<Vec<Embedding>, AppError> {
        tracing::info!(
            op,
            warmup_discard = limits.warmup_discard,
            max_frames = limits.max_frames,
            "stream limits"
        );

        let t_stream = Instant::now();
        let (mut session, limits) = self.open_capture(limits)?;
        let mut embeddings = Vec::new();
        for i in 0..limits.max_frames {
            let frame = session.next_frame()?;
            if let Some(e) = self.try_embed_from_frame(&frame)? {
                tracing::debug!(
                    op,
                    frame_index = i,
                    dim = e.dim(),
                    "frame contributed embedding"
                );
                embeddings.push(e);
            } else {
                tracing::debug!(op, frame_index = i, "frame skipped");
            }
        }

        tracing::info!(
            op,
            collected = embeddings.len(),
            elapsed_ms = t_stream.elapsed().as_millis(),
            "stream processed"
        );
        Ok(embeddings)
    }

    fn template_from_capture(
        &self,
        op: &'static str,
        limits: StreamLimits,
    ) -> Result<Embedding, AppError> {
        let embeddings = self.collect_embeddings(op, limits)?;
        if embeddings.is_empty() {
            tracing::warn!(op, "no usable embeddings from any frame");
            return Err(DomainError::NoUsableFaceInCapture.into());
        }
        let template =
            Embedding::try_average(&embeddings).ok_or(DomainError::EmbeddingAggregationFailed)?;
        tracing::info!(
            op,
            from_frames = embeddings.len(),
            template_dim = template.dim(),
            "template averaged"
        );
        Ok(template)
    }

    pub fn ping(&self) -> Result<(), AppError> {
        self.require_healthy()
    }

    pub fn verify(&self, user: &UserId) -> Result<bool, AppError> {
        let span = tracing::info_span!("verify", uid = user.0);
        let _g = span.enter();
        self.require_healthy()?;

        let bundle = self.load_enrolled(user)?;
        let n_templates = bundle.templates.len();
        let quorum_need = template_quorum_required(n_templates);
        tracing::info!(
            templates = n_templates,
            quorum_required = quorum_need,
            template_dim = bundle.templates.first().map(|e| e.dim()).unwrap_or(0),
            "verify: templates loaded"
        );

        tracing::info!(
            warmup_discard = self.streaming.verify.warmup_discard,
            max_frames = self.streaming.verify.max_frames,
            "verify: stream limits"
        );

        let t_stream = Instant::now();
        let (mut session, limits) = self.open_capture(self.streaming.verify)?;

        let mut probes: Vec<Option<Embedding>> = Vec::with_capacity(limits.max_frames as usize);
        for _ in 0..limits.max_frames {
            let frame = session.next_frame()?;
            let emb = self.try_embed_from_frame(&frame)?;
            if emb.is_none() {
                tracing::debug!(
                    frame_index = probes.len(),
                    "verify: frame produced no embedding"
                );
            }
            probes.push(emb);

            let outcome = self.verification.verify_burst(&bundle, &probes);
            if outcome.accepted {
                tracing::info!(
                    frames_tried = probes.len(),
                    elapsed_ms = t_stream.elapsed().as_millis(),
                    "verify: accept"
                );
                return Ok(true);
            }
        }

        tracing::info!(
            frames = probes.len(),
            with_embedding = probes.iter().filter(|x| x.is_some()).count(),
            elapsed_ms = t_stream.elapsed().as_millis(),
            "verify: stream processed"
        );

        let outcome = self.verification.verify_burst(&bundle, &probes);
        tracing::info!(
            accepted = outcome.accepted,
            quorum = outcome.quorum,
            best_sim = outcome.best_sim,
            has_probe = outcome.has_probe,
            elapsed_ms = t_stream.elapsed().as_millis(),
            templates = n_templates,
            "verify: match"
        );
        Ok(outcome.accepted)
    }

    pub fn enroll(&self, user: &UserId) -> Result<(), AppError> {
        let span = tracing::info_span!("enroll", uid = user.0);
        let _g = span.enter();
        self.require_healthy()?;

        if self
            .template_store
            .load_all(user)?
            .is_some_and(|b| b.has_any_enrollment())
        {
            return Err(DomainError::AlreadyEnrolled.into());
        }

        let template = self.template_from_capture("enroll", self.streaming.enroll)?;
        let mut bundle = TemplateBundle::empty();
        bundle.templates.push(template);
        self.template_store.save_all(user, &bundle)?;
        tracing::info!("enroll: stored ok");
        Ok(())
    }

    pub fn add_template(&self, user: &UserId) -> Result<(), AppError> {
        let span = tracing::info_span!("add_template", uid = user.0);
        let _g = span.enter();
        self.require_healthy()?;

        let mut bundle = self.load_enrolled(user)?;
        tracing::debug!(
            existing_templates = bundle.templates.len(),
            "add_template: loaded existing"
        );

        let new_t = self.template_from_capture("add_template", self.streaming.enroll)?;
        bundle.templates.push(new_t);
        tracing::info!(
            templates = bundle.templates.len(),
            template_dim = bundle.templates.last().map(|e| e.dim()).unwrap_or(0),
            "add_template: appended templates"
        );
        self.template_store.save_all(user, &bundle)?;
        tracing::info!("add_template: stored ok");
        Ok(())
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;

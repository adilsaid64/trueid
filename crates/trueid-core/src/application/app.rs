use std::sync::Arc;

use crate::domain::UserId;
use crate::ports::{
    BiometricVerifier, Embedder, EmbeddingMatcher, Health, HealthStatus, TemplateStore, VideoSource,
};

use super::error::AppError;

pub struct TrueIdApp {
    health: Arc<dyn Health>,
    biometric: Arc<dyn BiometricVerifier>,
    video: Arc<dyn VideoSource>,
    embedder: Arc<dyn Embedder>,
    template_store: Arc<dyn TemplateStore>,
    matcher: Arc<dyn EmbeddingMatcher>,
}

impl TrueIdApp {
    pub fn new(
        health: Arc<dyn Health>,
        biometric: Arc<dyn BiometricVerifier>,
        video: Arc<dyn VideoSource>,
        embedder: Arc<dyn Embedder>,
        template_store: Arc<dyn TemplateStore>,
        matcher: Arc<dyn EmbeddingMatcher>,
    ) -> Self {
        Self {
            health,
            biometric,
            video,
            embedder,
            template_store,
            matcher,
        }
    }

    pub fn ping(&self) -> Result<(), AppError> {
        match self.health.status() {
            HealthStatus::Healthy => Ok(()),
            HealthStatus::Degraded { reason } => Err(AppError::Unhealthy(reason)),
        }
    }

    pub fn biometric_label(&self) -> &str {
        self.biometric.label()
    }

    pub fn verify(&self, user: &UserId) -> Result<bool, AppError> {
        match self.health.status() {
            HealthStatus::Healthy => {}
            HealthStatus::Degraded { reason } => return Err(AppError::Unhealthy(reason)),
        }

        let frame = self.video.next_frame()?;
        let probe = self.embedder.embed(&frame)?;
        let Some(enrolled) = self.template_store.load(user)? else {
            return Err(crate::domain::error::DomainError::NoEnrolledTemplate.into());
        };

        if self.matcher.matches(&probe, &enrolled) {
            Ok(true)
        } else {
            Err(crate::domain::error::DomainError::VerificationFailed.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::error::DomainError;
    use crate::domain::{Embedding, Frame, PixelFormat, StreamModality};
    use crate::ports::{
        BiometricVerifier, EmbedError, Embedder, EmbeddingMatcher, Health, HealthStatus,
        StoreError, TemplateStore, VideoSource,
    };

    struct OkHealth;
    impl Health for OkHealth {
        fn status(&self) -> HealthStatus {
            HealthStatus::Healthy
        }
    }

    struct BadHealth;
    impl Health for BadHealth {
        fn status(&self) -> HealthStatus {
            HealthStatus::Degraded {
                reason: "camera offline",
            }
        }
    }

    struct StubBio;
    impl BiometricVerifier for StubBio {
        fn label(&self) -> &str {
            "stub"
        }
    }

    struct TestFrame;
    impl VideoSource for TestFrame {
        fn modality(&self) -> StreamModality {
            StreamModality::Rgb
        }

        fn next_frame(&self) -> Result<Frame, crate::ports::CaptureError> {
            Ok(Frame {
                modality: StreamModality::Rgb,
                width: 1,
                height: 1,
                format: PixelFormat::Gray8,
                bytes: vec![0],
            })
        }
    }

    struct ConstEmbedder {
        out: Embedding,
    }

    impl Embedder for ConstEmbedder {
        fn embed(&self, _frame: &Frame) -> Result<Embedding, EmbedError> {
            Ok(self.out.clone())
        }
    }

    struct MemoryStore {
        inner: std::sync::Mutex<std::collections::HashMap<UserId, Embedding>>,
    }

    impl MemoryStore {
        fn with_template(user: UserId, emb: Embedding) -> Self {
            let mut m = std::collections::HashMap::new();
            m.insert(user, emb);
            Self {
                inner: std::sync::Mutex::new(m),
            }
        }

        fn empty() -> Self {
            Self {
                inner: std::sync::Mutex::new(std::collections::HashMap::new()),
            }
        }
    }

    impl TemplateStore for MemoryStore {
        fn load(&self, user: &UserId) -> Result<Option<Embedding>, StoreError> {
            Ok(self.inner.lock().unwrap().get(user).cloned())
        }

        fn save(&self, user: &UserId, embedding: &Embedding) -> Result<(), StoreError> {
            self.inner.lock().unwrap().insert(*user, embedding.clone());
            Ok(())
        }
    }

    struct ExactMatcher;
    impl EmbeddingMatcher for ExactMatcher {
        fn matches(&self, probe: &Embedding, enrolled: &Embedding) -> bool {
            probe == enrolled
        }
    }

    fn app_with_store(store: Arc<MemoryStore>, embed_out: Embedding) -> TrueIdApp {
        let template_store: Arc<dyn TemplateStore> = store;
        TrueIdApp::new(
            Arc::new(OkHealth),
            Arc::new(StubBio),
            Arc::new(TestFrame),
            Arc::new(ConstEmbedder { out: embed_out }),
            template_store,
            Arc::new(ExactMatcher),
        )
    }

    #[test]
    fn ping_ok_when_healthy() {
        let store = Arc::new(MemoryStore::empty());
        let app = app_with_store(store, Embedding(vec![1.0, 0.0]));
        assert!(app.ping().is_ok());
    }

    #[test]
    fn ping_err_when_degraded() {
        let store: Arc<dyn TemplateStore> = Arc::new(MemoryStore::empty());
        let app = TrueIdApp::new(
            Arc::new(BadHealth),
            Arc::new(StubBio),
            Arc::new(TestFrame),
            Arc::new(ConstEmbedder {
                out: Embedding(vec![1.0]),
            }),
            store,
            Arc::new(ExactMatcher),
        );
        let err = app.ping().unwrap_err();
        assert!(err.to_string().contains("camera offline"));
    }

    #[test]
    fn verify_no_template() {
        let store = Arc::new(MemoryStore::empty());
        let app = app_with_store(store, Embedding(vec![1.0, 0.0]));
        let err = app.verify(&UserId(1000)).unwrap_err();
        assert!(matches!(
            err,
            AppError::Domain(DomainError::NoEnrolledTemplate)
        ));
    }

    #[test]
    fn verify_match() {
        let emb = Embedding(vec![0.5, 0.5, 0.0]);
        let store = Arc::new(MemoryStore::with_template(UserId(1000), emb.clone()));
        let app = app_with_store(store, emb);
        assert!(app.verify(&UserId(1000)).unwrap());
    }

    #[test]
    fn verify_mismatch() {
        let store = Arc::new(MemoryStore::with_template(
            UserId(1000),
            Embedding(vec![1.0, 0.0, 0.0]),
        ));
        let app = app_with_store(store, Embedding(vec![0.0, 1.0, 0.0]));
        let err = app.verify(&UserId(1000)).unwrap_err();
        assert!(matches!(
            err,
            AppError::Domain(DomainError::VerificationFailed)
        ));
    }
}

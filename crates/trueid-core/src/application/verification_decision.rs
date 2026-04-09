//! Match templates against per-frame probe embeddings (batch or, later, streamed).
//!
//! [`VerificationDecider`] implements quorum + RGB/IR fusion. Capture and
//! frame-to-embedding conversion live in `TrueIdApp` (see `try_embed_from_frame` for batch mode).

use std::sync::Arc;

use crate::domain::{Embedding, TemplateBundle};
use crate::ports::EmbeddingMatcher;

/// RGB/IR weights and fusion threshold when both template lists exist.
#[derive(Debug, Clone, Copy)]
pub struct ModalityFusionConfig {
    pub weight_rgb: f32,
    pub weight_ir: f32,
    pub fusion_threshold: f32,
}

impl Default for ModalityFusionConfig {
    fn default() -> Self {
        Self {
            weight_rgb: 0.45,
            weight_ir: 0.55,
            fusion_threshold: 0.5,
        }
    }
}

/// Templates that must match one probe: ceil(n/2).
pub fn template_quorum_required(template_count: usize) -> usize {
    template_count.div_ceil(2)
}

/// Outcome of evaluating a full burst (or accumulated probes) against enrolled templates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BurstVerificationOutcome {
    pub accepted: bool,
    pub best_sim_rgb: f32,
    pub best_sim_ir: f32,
    pub rgb_quorum: bool,
    pub ir_quorum: bool,
    pub has_rgb_probe: bool,
    pub has_ir_probe: bool,
}

/// Quorum and fusion over modality probe sequences vs [`TemplateBundle`].
pub struct VerificationDecider {
    matcher: Arc<dyn EmbeddingMatcher>,
    fusion: ModalityFusionConfig,
}

impl VerificationDecider {
    pub fn new(matcher: Arc<dyn EmbeddingMatcher>, fusion: ModalityFusionConfig) -> Self {
        Self { matcher, fusion }
    }

    pub const fn modality_fusion(&self) -> ModalityFusionConfig {
        self.fusion
    }

    /// Evaluate RGB/IR probe sequences (e.g. one embedding per captured frame) and apply fusion rules.
    pub fn verify_burst(
        &self,
        bundle: &TemplateBundle,
        probes_rgb: Option<&[Option<Embedding>]>,
        probes_ir: Option<&[Option<Embedding>]>,
    ) -> BurstVerificationOutcome {
        let agg_rgb = match probes_rgb {
            Some(rgb) if !rgb.is_empty() => self.aggregate_modality_for_verify(rgb, &bundle.rgb),
            _ => (0.0f32, false),
        };

        let agg_ir = match probes_ir {
            Some(ir) if !ir.is_empty() => self.aggregate_modality_for_verify(ir, &bundle.ir),
            _ => (0.0f32, false),
        };

        let has_rgb_probe = probes_rgb.is_some_and(|v| v.iter().any(|p| p.is_some()));

        let has_ir_probe = probes_ir.is_some_and(|v| v.iter().any(|p| p.is_some()));

        let accepted =
            self.fused_match_from_aggregates(bundle, agg_rgb, agg_ir, has_rgb_probe, has_ir_probe);

        BurstVerificationOutcome {
            accepted,
            best_sim_rgb: agg_rgb.0,
            best_sim_ir: agg_ir.0,
            rgb_quorum: agg_rgb.1,
            ir_quorum: agg_ir.1,
            has_rgb_probe,
            has_ir_probe,
        }
    }

    /// Max similarity vs templates and whether quorum holds for one probe.
    fn evaluate_probe_vs_templates(
        &self,
        probe: Option<&Embedding>,
        templates: &[Embedding],
    ) -> (f32, bool) {
        let Some(p) = probe else {
            return (0.0, false);
        };
        if templates.is_empty() {
            return (0.0, false);
        }
        let required = template_quorum_required(templates.len());
        let mut pass_count = 0usize;
        let mut best_sim = 0.0f32;
        for t in templates {
            if let Some(s) = self.matcher.similarity(p, t) {
                best_sim = best_sim.max(s);
            }
            if self.matcher.matches(p, t) {
                pass_count += 1;
            }
        }
        let quorum = pass_count >= required;
        (best_sim, quorum)
    }

    /// Max similarity and any-frame quorum for one modality over a burst.
    fn aggregate_modality_for_verify(
        &self,
        probes: &[Option<Embedding>],
        templates: &[Embedding],
    ) -> (f32, bool) {
        let mut any_quorum = false;
        let mut max_best_sim = 0.0f32;
        for p in probes {
            let (sim, q) = self.evaluate_probe_vs_templates(p.as_ref(), templates);
            max_best_sim = max_best_sim.max(sim);
            any_quorum |= q;
        }
        (max_best_sim, any_quorum)
    }

    /// RGB-only, IR-only, or weighted fusion from per-modality aggregates. Both quorums → accept; else weighted sims.
    fn fused_match_from_aggregates(
        &self,
        bundle: &TemplateBundle,
        (sim_r, q_r): (f32, bool),
        (sim_i, q_i): (f32, bool),
        has_r: bool,
        has_i: bool,
    ) -> bool {
        let fusion = self.fusion;

        if bundle.ir.is_empty() {
            return q_r;
        }
        if bundle.rgb.is_empty() {
            return q_i;
        }

        if !has_r && !has_i {
            return false;
        }
        if !has_r && has_i {
            return q_i;
        }
        if has_r && !has_i {
            return q_r;
        }

        if q_r && q_i {
            return true;
        }

        if !q_r && !q_i {
            return false;
        }

        let sr = sim_r.clamp(0.0, 1.0);
        let si = sim_i.clamp(0.0, 1.0);
        fusion.weight_rgb * sr + fusion.weight_ir * si >= fusion.fusion_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::Embedding;

    struct ExactMatcher;
    impl EmbeddingMatcher for ExactMatcher {
        fn matches(&self, probe: &Embedding, enrolled: &Embedding) -> bool {
            probe == enrolled
        }

        fn similarity(&self, probe: &Embedding, enrolled: &Embedding) -> Option<f32> {
            Some(if probe == enrolled { 1.0 } else { 0.0 })
        }
    }

    #[test]
    fn template_quorum_required_is_ceil_half() {
        assert_eq!(template_quorum_required(1), 1);
        assert_eq!(template_quorum_required(2), 1);
        assert_eq!(template_quorum_required(3), 2);
        assert_eq!(template_quorum_required(4), 2);
        assert_eq!(template_quorum_required(5), 3);
    }

    #[test]
    fn verify_burst_rgb_only_accept_on_quorum() {
        let bundle = TemplateBundle {
            rgb: vec![Embedding(vec![1.0, 0.0]), Embedding(vec![0.0, 1.0])],
            ir: vec![],
        };
        let matcher: Arc<dyn EmbeddingMatcher> = Arc::new(ExactMatcher);
        let decider = VerificationDecider::new(matcher, ModalityFusionConfig::default());
        let probe_ok = Embedding(vec![0.0, 1.0]);
        let out = decider.verify_burst(&bundle, Some(&[Some(probe_ok), None]), None);
        assert!(out.accepted);
        assert!(out.rgb_quorum);
    }

    #[test]
    fn verify_burst_rgb_only_reject_when_quorum_not_met() {
        let bundle = TemplateBundle {
            rgb: vec![
                Embedding(vec![1.0, 0.0]),
                Embedding(vec![0.0, 1.0]),
                Embedding(vec![0.0, 0.0, 1.0]),
            ],
            ir: vec![],
        };
        let matcher: Arc<dyn EmbeddingMatcher> = Arc::new(ExactMatcher);
        let decider = VerificationDecider::new(matcher, ModalityFusionConfig::default());
        let out =
            decider.verify_burst(&bundle, Some(&[Some(Embedding(vec![0.0, 0.0, 1.0]))]), None);
        assert!(!out.accepted);
    }
}

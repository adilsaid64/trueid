use std::sync::Arc;

use crate::domain::{Embedding, TemplateBundle};
use crate::ports::EmbeddingMatcher;

pub fn template_quorum_required(template_count: usize) -> usize {
    template_count.div_ceil(2)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BurstVerificationOutcome {
    pub accepted: bool,
    pub best_sim: f32,
    pub quorum: bool,
    pub has_probe: bool,
}

pub struct VerificationDecider {
    matcher: Arc<dyn EmbeddingMatcher>,
}

impl VerificationDecider {
    pub fn new(matcher: Arc<dyn EmbeddingMatcher>) -> Self {
        Self { matcher }
    }

    pub fn verify_burst(
        &self,
        bundle: &TemplateBundle,
        probes: &[Option<Embedding>],
    ) -> BurstVerificationOutcome {
        if bundle.templates.is_empty() || probes.is_empty() {
            return BurstVerificationOutcome {
                accepted: false,
                best_sim: 0.0,
                quorum: false,
                has_probe: false,
            };
        }

        let has_probe = probes.iter().any(|p| p.is_some());
        let mut any_quorum = false;
        let mut max_best_sim = 0.0f32;
        for p in probes {
            let (sim, q) = self.evaluate_probe_vs_templates(p.as_ref(), &bundle.templates);
            max_best_sim = max_best_sim.max(sim);
            any_quorum |= q;
        }

        BurstVerificationOutcome {
            accepted: any_quorum,
            best_sim: max_best_sim,
            quorum: any_quorum,
            has_probe,
        }
    }

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
    fn verify_burst_accept_on_quorum() {
        let bundle = TemplateBundle {
            templates: vec![Embedding(vec![1.0, 0.0]), Embedding(vec![0.0, 1.0])],
        };
        let matcher: Arc<dyn EmbeddingMatcher> = Arc::new(ExactMatcher);
        let decider = VerificationDecider::new(matcher);
        let probe_ok = Embedding(vec![0.0, 1.0]);
        let out = decider.verify_burst(&bundle, &[Some(probe_ok), None]);
        assert!(out.accepted);
        assert!(out.quorum);
    }

    #[test]
    fn verify_burst_reject_when_quorum_not_met() {
        let bundle = TemplateBundle {
            templates: vec![
                Embedding(vec![1.0, 0.0]),
                Embedding(vec![0.0, 1.0]),
                Embedding(vec![0.0, 0.0, 1.0]),
            ],
        };
        let matcher: Arc<dyn EmbeddingMatcher> = Arc::new(ExactMatcher);
        let decider = VerificationDecider::new(matcher);
        let out = decider.verify_burst(&bundle, &[Some(Embedding(vec![0.0, 0.0, 1.0]))]);
        assert!(!out.accepted);
    }
}

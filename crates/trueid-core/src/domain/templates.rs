use super::Embedding;

/// Per-user enrollment: RGB and IR templates are stored and matched separately.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateBundle {
    pub rgb: Vec<Embedding>,
    pub ir: Vec<Embedding>,
}

impl TemplateBundle {
    pub fn empty() -> Self {
        Self {
            rgb: Vec::new(),
            ir: Vec::new(),
        }
    }

    /// Any stored templates (used for “is there a file / record?”).
    pub fn is_empty(&self) -> bool {
        self.rgb.is_empty() && self.ir.is_empty()
    }

    /// Primary enrollment gate: RGB path is required for first enroll.
    pub fn has_rgb_enrollment(&self) -> bool {
        !self.rgb.is_empty()
    }
}

impl Default for TemplateBundle {
    fn default() -> Self {
        Self::empty()
    }
}

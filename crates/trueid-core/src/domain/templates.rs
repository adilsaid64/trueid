use super::Embedding;

/// RGB and IR template lists; verify matches each side separately.
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

    pub fn is_empty(&self) -> bool {
        self.rgb.is_empty() && self.ir.is_empty()
    }

    pub fn has_rgb_enrollment(&self) -> bool {
        !self.rgb.is_empty()
    }
}

impl Default for TemplateBundle {
    fn default() -> Self {
        Self::empty()
    }
}

use super::Embedding;

/// All probe templates for one user (from the active camera stream at enrollment time).
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateBundle {
    pub templates: Vec<Embedding>,
}

impl TemplateBundle {
    pub fn empty() -> Self {
        Self {
            templates: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.templates.is_empty()
    }

    pub fn has_any_enrollment(&self) -> bool {
        !self.is_empty()
    }
}

impl Default for TemplateBundle {
    fn default() -> Self {
        Self::empty()
    }
}

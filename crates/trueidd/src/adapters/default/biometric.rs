use trueid_core::ports::BiometricVerifier;

pub struct DefaultBiometric;

impl BiometricVerifier for DefaultBiometric {
    fn label(&self) -> &str {
        "unconfigured"
    }
}

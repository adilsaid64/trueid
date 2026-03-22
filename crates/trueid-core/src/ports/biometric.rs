pub trait BiometricVerifier: Send + Sync {
    fn label(&self) -> &str;
}

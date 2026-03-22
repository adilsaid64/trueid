use std::sync::Arc;

use crate::ports::{BiometricVerifier, Health, HealthStatus};

use super::error::AppError;

pub struct TrueIdApp {
    health: Arc<dyn Health>,
    biometric: Arc<dyn BiometricVerifier>,
}

impl TrueIdApp {
    pub fn new(health: Arc<dyn Health>, biometric: Arc<dyn BiometricVerifier>) -> Self {
        Self { health, biometric }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{BiometricVerifier, Health, HealthStatus};

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

    #[test]
    fn ping_ok_when_healthy() {
        let app = TrueIdApp::new(
            Arc::new(OkHealth),
            Arc::new(StubBio),
        );
        assert!(app.ping().is_ok());
    }

    #[test]
    fn ping_err_when_degraded() {
        let app = TrueIdApp::new(
            Arc::new(BadHealth),
            Arc::new(StubBio),
        );
        let err = app.ping().unwrap_err();
        assert!(err.to_string().contains("camera offline"));
    }
}

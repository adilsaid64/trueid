use trueid_core::ports::{Health, HealthStatus};

pub struct DefaultHealth;

impl Health for DefaultHealth {
    fn status(&self) -> HealthStatus {
        HealthStatus::Healthy
    }
}

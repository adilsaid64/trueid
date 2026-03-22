#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded { reason: &'static str },
}

pub trait Health: Send + Sync {
    fn status(&self) -> HealthStatus;
}

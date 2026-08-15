//! Application services (the inbound port): `ping`, `enroll`, `verify`, `add_template`.
//!
//! Driving adapters call [`app::TrueIdApp`]. This layer depends on [`crate::ports`]
//! and [`crate::domain`] only.

pub mod app;
pub mod error;
pub mod verification_decision;

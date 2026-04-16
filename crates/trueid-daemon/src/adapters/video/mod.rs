//! V4L and mock [`VideoSource`](trueid_core::ports::VideoSource) adapters.

mod mock;
mod v4l;

pub use mock::MockVideoSource;
pub use v4l::V4lVideoSource;

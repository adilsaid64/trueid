//! [`VideoSource`](trueid_core::ports::VideoSource): V4L device or in-memory test frames.

mod mock;
mod v4l;

pub use mock::MockVideoSource;
pub use v4l::V4lVideoSource;

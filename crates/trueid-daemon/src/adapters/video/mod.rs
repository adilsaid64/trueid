//! [`VideoSource`](trueid_core::ports::VideoSource): V4L device or in-memory test frames.
//! [`CameraCapture`](trueid_core::ports::CameraCapture) adapters that compose them.

mod camera_capture;
mod mock;
mod v4l;

pub use camera_capture::{ParallelRgbIrCameraCapture, RgbOnlyCameraCapture};
pub use mock::MockVideoSource;
pub use v4l::V4lVideoSource;

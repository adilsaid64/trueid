//! V4L / mock `VideoSource` and `CameraCapture` adapters.

mod camera_capture;
mod mock;
mod v4l;

pub use camera_capture::{IROnlyCameraCapture, ParallelRgbIrCameraCapture, RgbOnlyCameraCapture};
pub use mock::MockVideoSource;
pub use v4l::V4lVideoSource;

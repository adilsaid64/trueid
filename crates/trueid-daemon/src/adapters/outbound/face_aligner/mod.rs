//! Outbound `FaceAligner`: landmark warp or bbox crop.

mod crop_bbox;
mod passthrough;

pub use crop_bbox::CropFaceAligner;
pub use passthrough::PassthroughFaceAligner;

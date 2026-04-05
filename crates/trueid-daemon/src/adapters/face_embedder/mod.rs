//! ONNX and test embedders.

pub mod mock;
pub mod onnx;

pub use mock::MockFaceEmbedder;
pub use onnx::build_face_embedder;

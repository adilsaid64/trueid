//! Outbound `FaceEmbedder`: ONNX ArcFace or constant mock.

pub mod mock;
pub mod onnx;

pub use mock::MockFaceEmbedder;
pub use onnx::build_face_embedder;

//! Face embedding: ONNX (`tract`) and test doubles.

pub mod mock;
pub mod onnx;

pub use mock::MockFaceEmbedder;
pub use onnx::build_face_embedder;

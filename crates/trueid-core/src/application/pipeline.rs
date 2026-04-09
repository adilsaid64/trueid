//! How enrollment and verification acquire and process frames (batch burst vs streaming loop).
//!
//! Streaming implementations will live alongside batch; capture ports and orchestration still TBD.

/// First-time enrollment: burst capture vs streaming session (e.g. stability-based stop).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnrollPipelineMode {
    /// Warmup + fixed frame count, then average embeddings (current behavior).
    #[default]
    Batch,
    /// Frame loop with policies such as embedding stability (not implemented yet).
    Streaming,
}

/// Verification: burst vs streaming temporal decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerifyPipelineMode {
    #[default]
    Batch,
    Streaming,
}

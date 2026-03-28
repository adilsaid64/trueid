use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("no enrolled template for this user")]
    NoEnrolledTemplate,

    #[error("user already has an enrolled template")]
    AlreadyEnrolled,

    #[error("could not average embeddings")]
    EmbeddingAggregationFailed,

    #[error("no face passed the pipeline in this capture")]
    NoUsableFaceInCapture,
}

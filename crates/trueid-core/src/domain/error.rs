use thiserror::Error;

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("no enrolled template for this user")]
    NoEnrolledTemplate,

    #[error("user already has an enrolled template")]
    AlreadyEnrolled,

    #[error("could not aggregate embeddings from capture burst")]
    EmbeddingAggregationFailed,
}

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SkinError {
    #[error("not a readable ZIP archive")]
    InvalidArchive,

    /// The archive parsed but lacked a sheet the renderer cannot do without.
    #[error("skin is missing required sheet `{0}`")]
    MissingSheet(&'static str),

    #[error("could not decode `{name}`: {reason}")]
    ImageDecode { name: String, reason: String },
}

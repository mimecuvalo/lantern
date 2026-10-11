use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("no audio output device available")]
    NoOutputDevice,

    #[error("unsupported or undecodable format: {0}")]
    UnsupportedFormat(String),

    #[error("decode failed: {0}")]
    Decode(String),

    #[error("output stream error: {0}")]
    Stream(String),
}

use thiserror::Error;

/// Errors that can occur during thumbnail generation.
#[derive(Error, Debug)]
pub enum ThumbsError {
    /// The specified file does not exist on disk.
    #[error("File not found: {0}")]
    FileNotFound(String),

    /// The file extension is not recognized as a supported format.
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(String),

    /// The OS failed to decode or open the image data.
    #[error("Failed to open image: {0}")]
    ImageError(String),

    /// Failed to write the thumbnail to the output path.
    #[error("Failed to save thumbnail: {0}")]
    SaveError(String),

    /// A platform-specific API call failed (e.g., COM error on Windows,
    /// QuickLook error on macOS).
    #[error("Platform error: {0}")]
    PlatformError(String),

    /// The OS thumbnail provider returned no result.
    /// On Windows: no registered IThumbnailProvider for this file type.
    /// On macOS: QLThumbnailGenerator could not produce a representation.
    #[error("Thumbnail generation failed: {0}")]
    ThumbnailGenerationFailed(String),

    /// The current platform is not supported (only macOS and Windows are).
    #[error("Platform not supported")]
    PlatformNotSupported,

    /// Standard I/O error (file read/write failures).
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}


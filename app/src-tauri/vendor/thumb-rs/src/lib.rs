use std::path::Path;

pub mod error;
pub use error::ThumbsError;

#[cfg(target_os = "macos")]
pub mod platform;

#[cfg(target_os = "windows")]
pub mod platform;

/// Raw thumbnail data returned by the library.
///
/// Contains uncompressed RGBA8 pixel data in row-major order.
/// Each pixel is 4 bytes: [R, G, B, A].
/// Total byte length = `width * height * 4`.
#[derive(Debug, Clone)]
pub struct Thumbnail {
    /// Raw RGBA8 pixel data, row-major, no padding.
    pub rgba: Vec<u8>,

    /// Width of the thumbnail in pixels.
    pub width: u32,

    /// Height of the thumbnail in pixels.
    pub height: u32,
}

impl Thumbnail {
    /// Create a new Thumbnail from raw pixel data.
    ///
    /// # Panics
    /// Panics if `rgba.len() != (width * height * 4) as usize`.
    pub fn new(rgba: Vec<u8>, width: u32, height: u32) -> Self {
        assert_eq!(
            rgba.len(),
            (width as usize) * (height as usize) * 4,
            "rgba length must be width * height * 4"
        );
        Self {
            rgba,
            width,
            height,
        }
    }
}

const BASE_PX: u32 = 256;

/// Thumbnail size as a scale multiplier.
///
/// `scale` multiplies a 256px base: `1` = 256×256, `2` = 512×512, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbnailScale(pub u32);

impl Default for ThumbnailScale {
    fn default() -> Self {
        Self(1)
    }
}

impl ThumbnailScale {
    /// Maximum thumbnail dimension in pixels (square).
    pub fn px(&self) -> u32 {
        self.0 * BASE_PX
    }
}

/// Generate a thumbnail for any file type the OS can preview.
///
/// On macOS uses QuickLook (`QLThumbnailGenerator`).
/// On Windows uses the Shell API (`IShellItemImageFactory`).
/// Produces the same thumbnails you see in Finder/Explorer.
///
/// # Arguments
/// * `file_path` — Path to the source file. Can be any type (image, video, PDF, etc.).
/// * `scale` — Size multiplier. `1` = 256×256, `2` = 512×512, etc.
///
/// # Errors
/// - `ThumbsError::FileNotFound` if the file doesn't exist.
/// - `ThumbsError::ThumbnailGenerationFailed` if the OS can't generate a thumbnail.
/// - `ThumbsError::PlatformError` for OS-level failures.
/// - `ThumbsError::PlatformNotSupported` on unsupported platforms (e.g., Linux).
pub fn get_thumbnail<P: AsRef<Path>>(
    file_path: P,
    scale: ThumbnailScale,
) -> Result<Thumbnail, ThumbsError> {
    let file_path = file_path.as_ref();

    if !file_path.exists() {
        return Err(ThumbsError::FileNotFound(file_path.display().to_string()));
    }

    #[cfg(target_os = "macos")]
    {
        platform::macos::generate_thumbnail(file_path, scale)
    }

    #[cfg(target_os = "windows")]
    {
        platform::windows::generate_thumbnail(file_path, scale)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Err(ThumbsError::PlatformNotSupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thumbnail_scale_default() {
        let scale = ThumbnailScale::default();
        assert_eq!(scale.0, 1);
        assert_eq!(scale.px(), 256);
    }

    #[test]
    fn test_thumbnail_scale_multiplier() {
        let scale = ThumbnailScale(2);
        assert_eq!(scale.px(), 512);
    }

    #[test]
    fn test_thumbnail_new_valid() {
        let rgba = vec![0u8; 256 * 256 * 4];
        let thumb = Thumbnail::new(rgba, 256, 256);
        assert_eq!(thumb.width, 256);
        assert_eq!(thumb.height, 256);
    }

    #[test]
    #[should_panic(expected = "rgba length must be")]
    fn test_thumbnail_new_invalid_size() {
        let rgba = vec![0u8; 100];
        Thumbnail::new(rgba, 256, 256);
    }
}


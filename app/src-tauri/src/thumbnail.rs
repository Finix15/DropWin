use base64::{engine::general_purpose, Engine as _};
use image::{ImageBuffer, Rgba};
use std::borrow::Cow;
use thumb_rs::{get_thumbnail, ThumbnailScale};

fn shell_compatible_path(file_path: &str) -> Cow<'_, str> {
    #[cfg(target_os = "windows")]
    {
        if let Some(unc_path) = file_path.strip_prefix(r"\\?\UNC\") {
            return Cow::Owned(format!(r"\\{unc_path}"));
        }
        if let Some(drive_path) = file_path.strip_prefix(r"\\?\") {
            return Cow::Borrowed(drive_path);
        }
    }

    Cow::Borrowed(file_path)
}

pub fn get_thumbnail_base64(file_path: &str) -> Result<String, String> {
    let shell_path = shell_compatible_path(file_path);
    let thumb =
        get_thumbnail(shell_path.as_ref(), ThumbnailScale::default()).map_err(|e| e.to_string())?;
    let img = ImageBuffer::<Rgba<u8>, _>::from_raw(thumb.width, thumb.height, thumb.rgba)
        .ok_or_else(|| "Failed to create image buffer".to_string())?;
    let mut png_data = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut png_data),
        image::ImageFormat::Png,
    )
    .map_err(|e| e.to_string())?;
    let encoded = general_purpose::STANDARD.encode(&png_data);

    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    #[test]
    fn removes_the_verbatim_prefix_from_drive_paths() {
        assert_eq!(
            shell_compatible_path(r"\\?\C:\Users\Test\file.txt"),
            r"C:\Users\Test\file.txt"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn converts_verbatim_unc_paths_to_shell_unc_paths() {
        assert_eq!(
            shell_compatible_path(r"\\?\UNC\server\share\file.txt"),
            r"\\server\share\file.txt"
        );
    }

    #[test]
    fn leaves_regular_paths_unchanged() {
        let path = r"C:\Users\Test\file.txt";
        assert_eq!(shell_compatible_path(path), path);
    }
}

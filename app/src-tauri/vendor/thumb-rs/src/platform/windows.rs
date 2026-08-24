/// Windows thumbnail implementation using `IShellItemImageFactory`.
///
/// Uses the same Shell API that Explorer.exe calls — returns real thumbnails
/// when a provider exists (VLC, Office, etc.), file-type icons as fallback.
/// Default flags (`SIIGBF_RESIZETOFIT`) match Explorer behavior.
use crate::error::ThumbsError;
use crate::{Thumbnail, ThumbnailScale};
use std::path::Path;

use windows::core::HSTRING;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, SelectObject, BITMAP,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED};
use windows::Win32::UI::Shell::{IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF};

/// Generate a thumbnail via `IShellItemImageFactory::GetImage`.
///
/// Uses default flags (`SIIGBF_RESIZETOFIT`) — real thumbnails when available,
/// file-type icons as fallback.
pub fn generate_thumbnail(
    file_path: &Path,
    scale: ThumbnailScale,
) -> Result<Thumbnail, ThumbsError> {
    let path_str = file_path
        .to_str()
        .ok_or_else(|| ThumbsError::PlatformError("Invalid UTF-8 in file path".into()))?;
    let wide_path = HSTRING::from(path_str);

    // Initialize COM (apartment-threaded).
    let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    if hr.is_err() {
        return Err(ThumbsError::PlatformError(format!(
            "CoInitializeEx failed: {hr:?}"
        )));
    }

    let shell_item: IShellItemImageFactory =
        match unsafe { SHCreateItemFromParsingName(&wide_path, None) } {
            Ok(item) => item,
            Err(e) => {
                unsafe { CoUninitialize() };
                // 0x80070002 = ERROR_FILE_NOT_FOUND, 0x80070003 = ERROR_PATH_NOT_FOUND
                let code = e.code().0 as u32;
                if code == 0x80070002 || code == 0x80070003 {
                    return Err(ThumbsError::FileNotFound(file_path.display().to_string()));
                }
                return Err(ThumbsError::PlatformError(format!(
                    "SHCreateItemFromParsingName failed: {e}"
                )));
            }
        };

    let hbitmap = match get_hbitmap(&shell_item, scale) {
        Ok(h) => h,
        Err(e) => {
            unsafe { CoUninitialize() };
            return Err(e);
        }
    };

    let result = hbitmap_to_rgba(hbitmap);

    unsafe {
        let _ = DeleteObject(hbitmap.into());
        CoUninitialize();
    }

    let (rgba, w, h) = result?;
    Ok(Thumbnail::new(rgba, w, h))
}

fn get_hbitmap(
    shell_item: &IShellItemImageFactory,
    scale: ThumbnailScale,
) -> Result<HBITMAP, ThumbsError> {
    let px = scale.px() as i32;
    let dimensions = SIZE { cx: px, cy: px };
    unsafe { shell_item.GetImage(dimensions, SIIGBF(0)) }
        .map_err(|e| ThumbsError::ThumbnailGenerationFailed(format!("GetImage failed: {e}")))
}

/// Extract RGBA8 from an HBITMAP via GDI. Swaps BGRA→RGBA and flips rows
/// from bottom-up to top-down pixel order.
fn hbitmap_to_rgba(hbitmap: HBITMAP) -> Result<(Vec<u8>, u32, u32), ThumbsError> {
    let mut bitmap = BITMAP::default();
    unsafe {
        GetObjectW(
            HGDIOBJ(hbitmap.0),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bitmap as *mut _ as *mut _),
        );
    }

    let width = bitmap.bmWidth as u32;
    let height = bitmap.bmHeight as u32;

    let dc = unsafe { CreateCompatibleDC(None) };
    if dc == HDC::default() {
        return Err(ThumbsError::PlatformError(
            "CreateCompatibleDC failed".into(),
        ));
    }

    let old_obj = unsafe { SelectObject(dc, hbitmap.into()) };

    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: height as i32, // positive = bottom-up
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let pixel_count = (width * height) as usize;
    let mut buffer = vec![0u8; pixel_count * 4];

    let lines = unsafe {
        GetDIBits(
            dc,
            hbitmap,
            0,
            height,
            Some(buffer.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };

    unsafe {
        SelectObject(dc, old_obj);
        let _ = DeleteDC(dc);
    }

    if lines == 0 {
        return Err(ThumbsError::ThumbnailGenerationFailed(
            "GetDIBits returned 0 lines".into(),
        ));
    }

    // BGRA → RGBA
    for pixel in buffer.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    // Flip rows: GetDIBits returns bottom-up, we want top-down.
    let row_bytes = (width as usize) * 4;
    let mut flipped = vec![0u8; buffer.len()];
    for row in 0..height as usize {
        let src = row * row_bytes;
        let dst = (height as usize - 1 - row) * row_bytes;
        flipped[dst..dst + row_bytes].copy_from_slice(&buffer[src..src + row_bytes]);
    }

    Ok((flipped, width, height))
}


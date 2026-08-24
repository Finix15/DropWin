/// macOS thumbnail implementation using `QLThumbnailGenerator`.
use crate::error::ThumbsError;
use crate::{Thumbnail, ThumbnailScale};
use std::path::Path;
use std::sync::mpsc;

use block2::Block;
use objc2::AnyThread;
use objc2_core_foundation::{
    kCFRunLoopDefaultMode, CFRunLoop, CFRunLoopRunResult, CGFloat, CGPoint, CGRect, CGSize,
};
use objc2_core_graphics::{CGBitmapInfo, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo};
use objc2_foundation::{NSError, NSString, NSURL};
use objc2_quick_look_thumbnailing::{
    QLThumbnailGenerationRequest, QLThumbnailGenerationRequestRepresentationTypes,
    QLThumbnailGenerator, QLThumbnailRepresentation,
};

extern "C-unwind" {
    fn CGBitmapContextCreate(
        data: *mut std::ffi::c_void,
        width: usize,
        height: usize,
        bits_per_component: usize,
        bytes_per_row: usize,
        colorspace: Option<&CGColorSpace>,
        bitmap_info: CGBitmapInfo,
    ) -> Option<objc2_core_foundation::CFRetained<CGContext>>;
}

pub fn generate_thumbnail(
    file_path: &Path,
    scale: ThumbnailScale,
) -> Result<Thumbnail, ThumbsError> {
    let path_str = file_path
        .to_str()
        .ok_or_else(|| ThumbsError::PlatformError("Invalid UTF-8 in file path".into()))?;

    let ns_string = NSString::from_str(path_str);
    let file_url = NSURL::fileURLWithPath_isDirectory(&ns_string, false);

    let px = scale.px() as CGFloat;
    let cg_size = CGSize {
        width: px,
        height: px,
    };
    let scale: CGFloat = 1.0;
    let repr_types = QLThumbnailGenerationRequestRepresentationTypes::All;

    let request = unsafe {
        let alloc = QLThumbnailGenerationRequest::alloc();
        QLThumbnailGenerationRequest::initWithFileAtURL_size_scale_representationTypes(
            alloc, &file_url, cg_size, scale, repr_types,
        )
    };

    let generator = unsafe { QLThumbnailGenerator::sharedGenerator() };

    let (tx, rx) = mpsc::channel();

    let block = block2::StackBlock::new(
        move |rep: *mut QLThumbnailRepresentation, error: *mut NSError| {
            if !error.is_null() {
                let err = unsafe { &*error };
                let msg = err.localizedDescription().to_string();
                let _ = tx.send(Err(ThumbsError::ThumbnailGenerationFailed(msg)));
                return;
            }

            if rep.is_null() {
                let _ = tx.send(Err(ThumbsError::ThumbnailGenerationFailed(
                    "No thumbnail representation returned".into(),
                )));
                return;
            }

            let rep_ref = unsafe { &*rep };
            let cg_image = unsafe { rep_ref.CGImage() };
            let _ = tx.send(extract_rgba(&cg_image).map(|(rgba, w, h)| Thumbnail::new(rgba, w, h)));
        },
    );
    let block = block.copy();

    unsafe {
        let raw_block: &Block<dyn Fn(*mut QLThumbnailRepresentation, *mut NSError)> =
            std::mem::transmute(&*block);
        generator.generateBestRepresentationForRequest_completionHandler(&request, raw_block);
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        if let Ok(result) = rx.try_recv() {
            return result;
        }

        if let Some(default_mode) = unsafe { kCFRunLoopDefaultMode } {
            let result = CFRunLoop::run_in_mode(Some(default_mode), 0.05, true);
            match result {
                CFRunLoopRunResult::Finished | CFRunLoopRunResult::Stopped => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
                _ => {}
            }
        } else {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        if std::time::Instant::now() > deadline {
            unsafe { generator.cancelRequest(&request) };
            return Err(ThumbsError::ThumbnailGenerationFailed(
                "Thumbnail generation timed out".into(),
            ));
        }
    }
}

/// Convert any CGImage (8-bit, 16-bit float, etc.) to RGBA8 via CGBitmapContext.
fn extract_rgba(cg_image: &CGImage) -> Result<(Vec<u8>, u32, u32), ThumbsError> {
    let width = CGImage::width(Some(cg_image));
    let height = CGImage::height(Some(cg_image));

    let color_space = CGColorSpace::new_device_rgb()
        .ok_or_else(|| ThumbsError::PlatformError("Failed to create CGColorSpace".into()))?;

    let bitmap_info = CGBitmapInfo(CGImageAlphaInfo::PremultipliedLast.0);
    let bytes_per_row = width * 4;

    let mut buffer = vec![0u8; bytes_per_row * height];

    let ctx = unsafe {
        CGBitmapContextCreate(
            buffer.as_mut_ptr() as *mut _,
            width,
            height,
            8,
            bytes_per_row,
            Some(&color_space),
            bitmap_info,
        )
    }
    .ok_or_else(|| ThumbsError::PlatformError("Failed to create CGBitmapContext".into()))?;

    let rect = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: width as CGFloat,
            height: height as CGFloat,
        },
    };
    CGContext::draw_image(Some(&ctx), rect, Some(cg_image));

    // Un-premultiply alpha.
    for pixel in buffer.chunks_exact_mut(4) {
        let a = pixel[3];
        if a > 0 && a < 255 {
            let s = 255.0 / a as f64;
            pixel[0] = (pixel[0] as f64 * s).round() as u8;
            pixel[1] = (pixel[1] as f64 * s).round() as u8;
            pixel[2] = (pixel[2] as f64 * s).round() as u8;
        }
    }

    Ok((buffer, width as u32, height as u32))
}


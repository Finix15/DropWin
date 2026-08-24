use crate::drop_registry::{drop_id_from_label, DropRegistry};
use crate::internal_drag::InternalDragState;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State, WebviewWindow};
use tracing::{error, info, warn};

fn begin_internal_drag(
    window: &WebviewWindow,
    drag_state: &InternalDragState,
    source_file_ids: Vec<u64>,
) -> Result<Option<u64>, String> {
    if source_file_ids.is_empty() {
        return Ok(None);
    }
    let Ok(source_drop_id) = drop_id_from_label(window.label()) else {
        return Ok(None);
    };
    drag_state
        .begin(source_drop_id.to_string(), source_file_ids)
        .map(Some)
}

fn finish_internal_drag(
    app: &AppHandle,
    drag_state: &InternalDragState,
    registry: &DropRegistry,
    token: Option<u64>,
) -> Result<(), String> {
    let Some(token) = token else {
        return Ok(());
    };
    let Some(session) = drag_state.finish(token)? else {
        return Ok(());
    };
    let Some(target_drop_id) = session.target_drop_id else {
        return Ok(());
    };
    if target_drop_id == session.source_drop_id {
        return Ok(());
    }

    let outcome = registry.transfer_files(
        &session.source_drop_id,
        &target_drop_id,
        &session.source_file_ids,
    )?;
    if outcome.transferred_count == 0 {
        return Ok(());
    }

    if let Err(error) = super::file_ops::notify_drop(app, &target_drop_id) {
        warn!("Failed to refresh internal drag target {target_drop_id}: {error}");
    }
    if outcome.source_empty {
        super::file_ops::schedule_close_empty_drop(app.clone(), session.source_drop_id.clone());
    } else if let Err(error) = super::file_ops::notify_drop(app, &session.source_drop_id) {
        warn!(
            "Failed to refresh internal drag source {}: {error}",
            session.source_drop_id
        );
    }
    info!(
        "Transferred {} item(s) from Drop {} to Drop {}",
        outcome.transferred_count, session.source_drop_id, target_drop_id
    );
    Ok(())
}

#[tauri::command]
pub fn start_multi_drag(
    app_handle: AppHandle,
    window: WebviewWindow,
    drag_state: State<'_, InternalDragState>,
    registry: State<'_, DropRegistry>,
    file_paths: Vec<String>,
    source_file_ids: Vec<u64>,
    drag_image: Option<String>,
) -> Result<(), String> {
    info!("Starting native drag for {} file(s)", file_paths.len());

    let mut valid_paths = Vec::new();

    for file_path in &file_paths {
        match std::fs::canonicalize(file_path.clone()) {
            Ok(path) => {
                if path.exists() {
                    valid_paths.push(path);
                } else {
                    warn!("Skipping drag path because it does not exist: {:?}", path);
                }
            }
            Err(e) => {
                warn!("Failed to canonicalize drag path '{}': {}", file_path, e);
            }
        }
    }

    if valid_paths.is_empty() {
        return Err("No valid files to drag".to_string());
    }

    // Use the drag image from the frontend if provided, otherwise generate one
    let image = if let Some(base64_data) = drag_image {
        // Remove data URL prefix if present (e.g., "data:image/png;base64,")
        let base64_str = if let Some(comma_pos) = base64_data.find(',') {
            &base64_data[comma_pos + 1..]
        } else {
            &base64_data
        };

        match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_str) {
            Ok(bytes) => {
                info!("Using frontend-provided drag image ({} bytes)", bytes.len());
                drag::Image::Raw(bytes)
            }
            Err(e) => {
                warn!(
                    "Failed to decode drag image, falling back to generated image: {}",
                    e
                );
                generate_drag_image(&valid_paths)
            }
        }
    } else {
        generate_drag_image(&valid_paths)
    };

    let item = drag::DragItem::Files(valid_paths.clone());
    info!(
        "Prepared drag item with {} valid file(s)",
        valid_paths.len()
    );

    // Ensure window is shown and activated for drag to work on macOS
    #[cfg(target_os = "macos")]
    {
        if let Err(e) = window.show() {
            warn!("Failed to show main window before drag: {}", e);
        }
        if let Err(e) = window.set_focus() {
            warn!("Failed to focus main window before drag: {}", e);
        }
        // Small delay to ensure window is properly activated
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let internal_drag_token = begin_internal_drag(&window, &drag_state, source_file_ids)?;
    let completion = Arc::new(Mutex::new(None));
    let callback_completion = completion.clone();
    let on_drop_callback = move |result: drag::DragResult, _: drag::CursorPosition| {
        if let Ok(mut completion) = callback_completion.lock() {
            *completion = Some(result);
        }
    };

    // On macOS, the drag crate only supports Copy or Move individually, not combined.
    // Using Copy as default (standard macOS behavior where Option key changes to Move).
    #[cfg(target_os = "macos")]
    let mode = drag::DragMode::CopyOrMove;
    #[cfg(not(target_os = "macos"))]
    let mode = drag::DragMode::CopyOrMove;

    let drag_result = drag::start_drag(
        &window,
        item,
        image,
        on_drop_callback,
        drag::Options {
            skip_animatation_on_cancel_or_failure: true,
            mode,
        },
    );
    let internal_result =
        finish_internal_drag(&app_handle, &drag_state, &registry, internal_drag_token);
    if let Ok(completion) = completion.lock() {
        info!("Native drag completed with result: {:?}", *completion);
    }

    match drag_result {
        Ok(_) => {
            internal_result?;
            info!("Native drag started successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to start native drag: {:?}", e);
            Err(format!(
                "Failed to start multi-file drag operation: {:?}",
                e
            ))
        }
    }
}

/// Generate a simple drag image with file count badge using the `image` crate.
/// Returns a PNG-encoded drag::Image::Raw at 128x128 (good enough for Retina).
fn generate_drag_image(file_paths: &[std::path::PathBuf]) -> drag::Image {
    let contains_folder = file_paths.iter().any(|path| path.is_dir());
    let contains_file = file_paths.iter().any(|path| path.is_file());
    generate_drag_image_for_kinds(file_paths.len(), contains_folder, contains_file)
}

fn generate_drag_image_for_kinds(
    file_count: usize,
    contains_folder: bool,
    contains_file: bool,
) -> drag::Image {
    use image::{Rgba, RgbaImage};

    let size = 128u32;
    let mut img = RgbaImage::new(size, size);

    // Draw a simple file icon (white rectangle with gray border and folded corner)
    let margin = 16u32;
    let fold = 24u32;
    let border_color = Rgba([160, 160, 160, 255]);
    let fill_color = Rgba([245, 245, 245, 255]);
    let fold_color = Rgba([200, 200, 200, 255]);

    if contains_file || !contains_folder {
        // Fill the file body
        for y in margin..size - margin {
            for x in margin..size - margin {
                // Skip the folded corner area
                if y < margin + fold && x > size - margin - fold {
                    continue;
                }
                img.put_pixel(x, y, fill_color);
            }
        }

        // Draw fold triangle
        for y in margin..margin + fold {
            for x in (size - margin - fold)..(size - margin) {
                let dx = x - (size - margin - fold);
                let dy = y - margin;
                if dx + dy <= fold {
                    img.put_pixel(x, y, fold_color);
                }
            }
        }

        // Draw border
        for x in margin..size - margin - fold {
            img.put_pixel(x, margin, border_color);
        }
        for x in margin..size - margin {
            img.put_pixel(x, size - margin - 1, border_color);
        }
        for y in margin..size - margin {
            img.put_pixel(margin, y, border_color);
            img.put_pixel(size - margin - 1, y, border_color);
        }
        for i in 0..fold {
            let x = size - margin - fold + i;
            let y = margin + fold - i;
            if x < size && y < size {
                img.put_pixel(x, y, border_color);
            }
        }
    }

    if contains_folder {
        let folder_left = if contains_file { 8 } else { 18 };
        let folder_right = if contains_file { 92 } else { 110 };
        let folder_top = if contains_file { 42 } else { 34 };
        let folder_bottom = if contains_file { 108 } else { 104 };
        let tab_right = folder_left + (folder_right - folder_left) / 2;
        let folder_color = Rgba([244, 197, 66, 255]);
        let folder_border = Rgba([255, 240, 163, 255]);

        for y in folder_top..folder_bottom {
            for x in folder_left..folder_right {
                let in_tab = y < folder_top + 12 && x < tab_right;
                let in_body = y >= folder_top + 10;
                if in_tab || in_body {
                    img.put_pixel(x, y, folder_color);
                }
            }
        }
        for x in folder_left..folder_right {
            img.put_pixel(x, folder_bottom - 1, folder_border);
        }
        for y in folder_top..folder_bottom {
            img.put_pixel(folder_left, y, folder_border);
            if y >= folder_top + 10 {
                img.put_pixel(folder_right - 1, y, folder_border);
            }
        }
    }

    // If multiple files, draw a badge circle with count
    if file_count > 1 {
        let badge_radius = 18i32;
        let badge_cx = (size - margin) as i32;
        let badge_cy = (size - margin) as i32;
        let badge_color = Rgba([59, 130, 246, 255]); // Blue
        let _badge_text_color = Rgba([255, 255, 255, 255]);

        // Draw badge circle
        for y in 0..size as i32 {
            for x in 0..size as i32 {
                let dx = x - badge_cx;
                let dy = y - badge_cy;
                if dx * dx + dy * dy <= badge_radius * badge_radius {
                    img.put_pixel(x as u32, y as u32, badge_color);
                }
            }
        }

        // Draw count number (simple pixel art for single/double digit)
        let count_str = if file_count > 99 {
            "99+".to_string()
        } else {
            file_count.to_string()
        };
        // For simplicity, just draw a small dot pattern - the badge itself is informative
        let _ = count_str; // Count shown by badge presence; actual text rendering is complex with `image` crate

        // Draw a simple "+" or number shape in the badge center
        // For now, just the badge alone indicates multiple files
    }

    // Encode to PNG
    let mut png_bytes: Vec<u8> = Vec::new();
    if let Err(e) = img.write_to(
        &mut std::io::Cursor::new(&mut png_bytes),
        image::ImageFormat::Png,
    ) {
        error!("Failed to encode generated drag image: {}", e);
        // Fallback: return a 1x1 transparent PNG
        return drag::Image::Raw(vec![]);
    }

    drag::Image::Raw(png_bytes)
}

#[tauri::command]
pub fn start_text_drag(
    app_handle: AppHandle,
    window: WebviewWindow,
    drag_state: State<'_, InternalDragState>,
    registry: State<'_, DropRegistry>,
    text: String,
    source_file_ids: Vec<u64>,
    drag_image: Option<String>,
) -> Result<(), String> {
    info!("Starting text drag");

    // Use the drag image from the frontend if provided, otherwise generate one
    let image = if let Some(base64_data) = drag_image {
        let base64_str = if let Some(comma_pos) = base64_data.find(',') {
            &base64_data[comma_pos + 1..]
        } else {
            &base64_data
        };

        match base64::Engine::decode(&base64::engine::general_purpose::STANDARD, base64_str) {
            Ok(bytes) => drag::Image::Raw(bytes),
            Err(e) => {
                warn!("Failed to decode drag image, falling back: {}", e);
                generate_drag_image_for_kinds(1, false, true)
            }
        }
    } else {
        generate_drag_image_for_kinds(1, false, true)
    };

    let text_clone = text.clone();
    let provider: drag::DataProvider = Box::new(move |format: &str| -> Option<Vec<u8>> {
        if format == "text/plain" {
            Some(text_clone.as_bytes().to_vec())
        } else {
            None
        }
    });

    let item = drag::DragItem::Data {
        provider,
        types: vec!["text/plain".to_string()],
    };

    #[cfg(target_os = "macos")]
    {
        if let Err(e) = window.show() {
            warn!("Failed to show main window before drag: {}", e);
        }
        if let Err(e) = window.set_focus() {
            warn!("Failed to focus main window before drag: {}", e);
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let internal_drag_token = begin_internal_drag(&window, &drag_state, source_file_ids)?;
    let completion = Arc::new(Mutex::new(None));
    let callback_completion = completion.clone();
    let on_drop_callback = move |result: drag::DragResult, _: drag::CursorPosition| {
        if let Ok(mut completion) = callback_completion.lock() {
            *completion = Some(result);
        }
    };

    #[cfg(target_os = "macos")]
    let mode = drag::DragMode::CopyOrMove;
    #[cfg(not(target_os = "macos"))]
    let mode = drag::DragMode::CopyOrMove;

    let drag_result = drag::start_drag(
        &window,
        item,
        image,
        on_drop_callback,
        drag::Options {
            skip_animatation_on_cancel_or_failure: true,
            mode,
        },
    );
    let internal_result =
        finish_internal_drag(&app_handle, &drag_state, &registry, internal_drag_token);
    if let Ok(completion) = completion.lock() {
        info!("Native text drag completed with result: {:?}", *completion);
    }

    match drag_result {
        Ok(_) => {
            internal_result?;
            info!("Native text drag started successfully");
            Ok(())
        }
        Err(e) => {
            error!("Failed to start native text drag: {:?}", e);
            Err(format!("Failed to start text drag operation: {:?}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::generate_drag_image_for_kinds;

    fn generated_pixel(contains_folder: bool, contains_file: bool, x: u32, y: u32) -> [u8; 4] {
        let drag::Image::Raw(bytes) =
            generate_drag_image_for_kinds(1, contains_folder, contains_file)
        else {
            panic!("generated drag preview was not raw image data");
        };
        let image = image::load_from_memory(&bytes).unwrap().into_rgba8();
        image.get_pixel(x, y).0
    }

    #[test]
    fn folder_fallback_uses_folder_color_instead_of_white_file_page() {
        assert_eq!(generated_pixel(true, false, 64, 70), [244, 197, 66, 255]);
    }

    #[test]
    fn mixed_fallback_keeps_a_visible_folder_layer() {
        assert_eq!(generated_pixel(true, true, 32, 70), [244, 197, 66, 255]);
    }
}

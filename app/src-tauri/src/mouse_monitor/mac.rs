use crate::config::{AppConfig, MouseMonitorConfig};
use crate::drop_window::{close_empty_drop_after_release, create_drop_window};
use crate::mouse_monitor::common::DRAG_PASTEBOARD_NAME;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};

use objc2::rc::Retained;
use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSArray, NSString};
use tracing::{info, warn};

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventSourceButtonState(stateID: u32, button: u32) -> bool;
}

fn get_cursor_position(app_handle: &AppHandle) -> (f64, f64) {
    crate::drop_window::cursor_position(app_handle)
}

fn is_mouse_button_down() -> bool {
    const K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE: u32 = 1;
    const K_CG_MOUSE_BUTTON_LEFT: u32 = 0;
    unsafe {
        CGEventSourceButtonState(
            K_CG_EVENT_SOURCE_STATE_HID_SYSTEM_STATE,
            K_CG_MOUSE_BUTTON_LEFT,
        )
    }
}

fn get_drag_pasteboard() -> Option<Retained<NSPasteboard>> {
    let name = NSString::from_str(DRAG_PASTEBOARD_NAME);
    Some(NSPasteboard::pasteboardWithName(&name))
}

fn get_pasteboard_change_count(pasteboard: &NSPasteboard) -> i64 {
    pasteboard.changeCount() as i64
}

fn pasteboard_has_files(pasteboard: &NSPasteboard) -> bool {
    let types = match pasteboard.types() {
        Some(t) => t,
        None => return false,
    };

    if types.is_empty() {
        return false;
    }

    let file_url_type = NSString::from_str("public.file-url");
    let file_url_array = NSArray::from_slice(&[&*file_url_type]);
    let has_file_url = pasteboard.availableTypeFromArray(&file_url_array).is_some();

    let filenames_type = NSString::from_str("NSFilenamesPboardType");
    let filenames_array = NSArray::from_slice(&[&*filenames_type]);
    let has_filenames = pasteboard
        .availableTypeFromArray(&filenames_array)
        .is_some();

    has_file_url || has_filenames
}

pub fn start_mouse_monitor(config: MouseMonitorConfig, app_handle: AppHandle) {
    info!(
        "Starting macOS mouse monitor (threshold={}, required_shakes={}, time_limit_ms={})",
        config.shake_threshold, config.required_shakes, config.shake_time_limit
    );

    thread::spawn(move || {
        let mut active_shake_drop: Option<String> = None;
        let mut last_position = get_cursor_position(&app_handle);
        let check_interval = Duration::from_millis(50);
        let mut shake_count = 0u32;
        let mut last_shake_time = Instant::now();
        let mut last_direction: Option<i32> = None;

        let pasteboard = match get_drag_pasteboard() {
            Some(pb) => pb,
            None => {
                warn!("Could not access drag pasteboard, mouse monitor exiting");
                return;
            }
        };

        let mut last_change_count = get_pasteboard_change_count(&pasteboard);
        let mut is_drag_active = false;

        loop {
            let config = {
                let state: State<Arc<Mutex<AppConfig>>> = app_handle.state();
                let lock = state.lock().unwrap();
                lock.mouse_monitor.clone()
            };
            let shake_threshold_x = config.shake_threshold as f64;
            let movement_time_limit = Duration::from_millis(config.shake_time_limit);

            let current_position = get_cursor_position(&app_handle);
            let current_change_count = get_pasteboard_change_count(&pasteboard);
            let has_files = pasteboard_has_files(&pasteboard);
            let mouse_down = is_mouse_button_down();

            let change_count_changed =
                current_change_count != last_change_count && current_change_count > 0;

            // --- Detect drag start ---
            if !is_drag_active && change_count_changed && has_files {
                is_drag_active = true;
                last_change_count = current_change_count;
                last_shake_time = Instant::now();
            }

            // --- Detect drag end (mouse released) ---
            // Only check mouse button - pasteboard can flicker during drag
            let drag_ended = is_drag_active && !mouse_down;

            if drag_ended {
                if let Some(drop_id) = active_shake_drop.take() {
                    close_empty_drop_after_release(app_handle.clone(), drop_id);
                }
                // Reset state
                is_drag_active = false;
                shake_count = 0;
                last_direction = None;
                last_change_count = current_change_count;
            }

            // --- Shake detection while dragging ---
            if is_drag_active {
                let distance_x = current_position.0 - last_position.0;

                let direction = if distance_x > shake_threshold_x {
                    1
                } else if distance_x < -shake_threshold_x {
                    -1
                } else {
                    0
                };

                if direction != 0 {
                    if let Some(last_dir) = last_direction {
                        if last_dir != direction {
                            last_shake_time = Instant::now();
                            shake_count += 1;
                        }
                    }
                    last_direction = Some(direction);
                }

                // Reset shake if too much time passes between wiggles
                if last_shake_time.elapsed() > movement_time_limit {
                    shake_count = 0;
                    last_direction = None;
                }

                // Trigger window open on shake
                if shake_count >= config.required_shakes && active_shake_drop.is_none() {
                    match create_drop_window(
                        app_handle.clone(),
                        current_position,
                        false,
                        "mouse_shake",
                    ) {
                        Ok(drop_id) => active_shake_drop = Some(drop_id),
                        Err(error) => warn!("Failed to create Drop from mouse shake: {error}"),
                    }

                    shake_count = 0;
                    last_direction = None;
                }
            }

            last_position = current_position;
            thread::sleep(check_interval);
        }
    });
}

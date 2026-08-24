use crate::config::{AppConfig, MouseMonitorConfig};
use crate::drop_window::{
    close_empty_drop_after_release, create_drop_window, prepare_mouse_drop_window,
};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager, State};
use tracing::info;
use windows::Win32::Foundation::POINT;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId,
};

const PREPARE_AFTER_RELEASE_DELAY_MS: u64 = 100;

fn get_mouse_pos() -> POINT {
    let mut pos = POINT::default();
    unsafe {
        let _ = GetCursorPos(&mut pos);
    }
    pos
}

fn get_active_process_name() -> Option<String> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        if process_id == 0 {
            return None;
        }

        let process_handle =
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;

        // Use QueryFullProcessImageNameW (requires Win32_System_Threading)
        let success = QueryFullProcessImageNameW(
            process_handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );

        let _ = windows::Win32::Foundation::CloseHandle(process_handle);

        if success.is_ok() && size > 0 {
            let path = String::from_utf16_lossy(&buffer[..size as usize]);
            let name = std::path::Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
            return name;
        }
        None
    }
}

fn is_mouse_button_down() -> bool {
    unsafe { GetAsyncKeyState(VK_LBUTTON.0 as i32) as u16 & 0x8000 != 0 }
}

fn is_process_blacklisted(active_process: &str, blacklist: &[String]) -> bool {
    let active_process = active_process.trim().to_lowercase();
    let active_without_extension = active_process
        .strip_suffix(".exe")
        .unwrap_or(&active_process);

    blacklist.iter().any(|entry| {
        let entry = entry.trim().to_lowercase();
        let entry_without_extension = entry.strip_suffix(".exe").unwrap_or(&entry);
        !entry_without_extension.is_empty() && entry_without_extension == active_without_extension
    })
}

pub fn start_mouse_monitor(config: MouseMonitorConfig, app_handle: AppHandle) {
    info!(
        "Starting Windows mouse monitor (threshold={}, required_shakes={}, time_limit_ms={})",
        config.shake_threshold, config.required_shakes, config.shake_time_limit
    );

    thread::spawn(move || {
        let mut active_shake_drop: Option<String> = None;
        let mut last_position = get_mouse_pos();
        let mut shake_count = 0;
        let mut last_shake_time = Instant::now();
        let mut last_direction: Option<i32> = None;

        loop {
            let config = {
                let state: State<Arc<Mutex<AppConfig>>> = app_handle.state();
                let lock = state.lock().unwrap();
                lock.mouse_monitor.clone()
            };

            let check_interval = Duration::from_millis(30);
            let movement_time_limit = Duration::from_millis(config.shake_time_limit);

            let current_pos = get_mouse_pos();
            let mouse_down = is_mouse_button_down();

            // --- CASE 1: USER RELEASES MOUSE ---
            if !mouse_down {
                if let Some(drop_id) = active_shake_drop.take() {
                    close_empty_drop_after_release(app_handle.clone(), drop_id);
                    let prepare_app = app_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        // Let the source thread fully unwind DoDragDrop before creating another
                        // WebView. Starting that work from the OLE release path can freeze both
                        // the source Drop and its drag image.
                        tokio::time::sleep(Duration::from_millis(PREPARE_AFTER_RELEASE_DELAY_MS))
                            .await;
                        if let Err(error) = prepare_mouse_drop_window(prepare_app) {
                            info!("Failed to prepare the next mouse Drop after release: {error}");
                        }
                    });
                }
                // Reset state
                shake_count = 0;
                last_direction = None;
                last_position = current_pos;
                thread::sleep(check_interval);
                continue;
            }

            // --- CASE 2: USER IS DRAGGING (Shake Detection) ---
            let distance_x = current_pos.x - last_position.x;
            let direction = if distance_x > config.shake_threshold {
                1
            } else if distance_x < -config.shake_threshold {
                -1
            } else {
                0
            };

            if direction != 0 {
                if let Some(last_dir) = last_direction {
                    if last_dir != direction {
                        shake_count += 1;
                        last_shake_time = Instant::now();
                    }
                }
                last_direction = Some(direction);
            }

            // Reset shake if too much time passes between wiggles
            if last_shake_time.elapsed() > movement_time_limit {
                shake_count = 0;
            }

            // Trigger Window
            if shake_count >= config.required_shakes && active_shake_drop.is_none() {
                let active_app = get_active_process_name().unwrap_or_default();
                let is_blacklisted = is_process_blacklisted(&active_app, &config.blacklist);

                if !is_blacklisted {
                    match create_drop_window(
                        app_handle.clone(),
                        (current_pos.x as f64, current_pos.y as f64),
                        false,
                        "mouse_shake",
                    ) {
                        Ok(drop_id) => active_shake_drop = Some(drop_id),
                        Err(error) => info!("Failed to create Drop from mouse shake: {error}"),
                    }
                }

                shake_count = 0;
                last_direction = None;
            }

            last_position = current_pos;
            thread::sleep(check_interval);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::is_process_blacklisted;

    #[test]
    fn blacklist_matches_case_insensitively_with_optional_exe_extension() {
        let blacklist = vec!["Photoshop".to_string(), "notepad.exe".to_string()];
        assert!(is_process_blacklisted("PHOTOSHOP.EXE", &blacklist));
        assert!(is_process_blacklisted("Notepad.exe", &blacklist));
        assert!(!is_process_blacklisted("explorer.exe", &blacklist));
    }

    #[test]
    fn empty_blacklist_allows_every_process() {
        assert!(!is_process_blacklisted("explorer.exe", &[]));
    }
}

use crate::drop_registry::{drop_id_from_label, popup_label};
use tauri::{AppHandle, LogicalPosition, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use tracing::info;

const POPUP_WIDTH: f64 = 450.0;
const POPUP_HEIGHT: f64 = 350.0;
const POPUP_GAP: f64 = 5.0;
const MONITOR_MARGIN: f64 = 12.0;

fn calculate_popup_position(
    drop_bounds: (f64, f64, f64, f64),
    monitor_bounds: (f64, f64, f64, f64),
) -> (f64, f64) {
    let (drop_x, drop_y, drop_width, drop_height) = drop_bounds;
    let (monitor_x, monitor_y, monitor_width, monitor_height) = monitor_bounds;
    let monitor_right = monitor_x + monitor_width;
    let monitor_bottom = monitor_y + monitor_height;
    let min_x = monitor_x + MONITOR_MARGIN;
    let max_x = (monitor_right - POPUP_WIDTH - MONITOR_MARGIN).max(min_x);
    let popup_x = (drop_x + (drop_width - POPUP_WIDTH) / 2.0).clamp(min_x, max_x);

    let below_y = drop_y + drop_height + POPUP_GAP;
    let above_y = drop_y - POPUP_HEIGHT - POPUP_GAP;
    let min_y = monitor_y + MONITOR_MARGIN;
    let max_y = (monitor_bottom - POPUP_HEIGHT - MONITOR_MARGIN).max(min_y);
    let popup_y = if below_y <= max_y {
        below_y
    } else if above_y >= min_y {
        above_y
    } else {
        let space_below = monitor_bottom - (drop_y + drop_height);
        let space_above = drop_y - monitor_y;
        if space_below >= space_above {
            below_y.clamp(min_y, max_y)
        } else {
            above_y.clamp(min_y, max_y)
        }
    };

    (popup_x, popup_y)
}

#[cfg(target_os = "windows")]
fn monitor_work_area(
    _app: &AppHandle,
    window: &WebviewWindow,
    _center: (f64, f64),
) -> Result<(f64, f64, f64, f64), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };

    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let monitor = unsafe { MonitorFromWindow(HWND(hwnd.0 as _), MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return Err(format!(
            "Failed to resolve monitor work area: {}",
            std::io::Error::last_os_error()
        ));
    }
    let work = info.rcWork;
    Ok((
        work.left as f64,
        work.top as f64,
        (work.right - work.left) as f64,
        (work.bottom - work.top) as f64,
    ))
}

#[cfg(not(target_os = "windows"))]
fn monitor_work_area(
    app: &AppHandle,
    _window: &WebviewWindow,
    center: (f64, f64),
) -> Result<(f64, f64, f64, f64), String> {
    let monitor = app
        .monitor_from_point(center.0, center.1)
        .map_err(|error| error.to_string())?
        .or(app.primary_monitor().map_err(|error| error.to_string())?)
        .ok_or_else(|| "No monitor is available for the Drop popup".to_string())?;
    let position = monitor.position();
    let size = monitor.size();
    Ok((
        position.x as f64,
        position.y as f64,
        size.width as f64,
        size.height as f64,
    ))
}

fn resolve_popup_position(app: &AppHandle, window: &WebviewWindow) -> Result<(f64, f64), String> {
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let position = window.inner_position().map_err(|error| error.to_string())?;
    let size = window.inner_size().map_err(|error| error.to_string())?;
    let cursor_x = position.x as f64 + size.width as f64 / 2.0;
    let cursor_y = position.y as f64 + size.height as f64 / 2.0;
    let monitor = monitor_work_area(app, window, (cursor_x, cursor_y))?;

    Ok(calculate_popup_position(
        (
            position.x as f64 / scale_factor,
            position.y as f64 / scale_factor,
            size.width as f64 / scale_factor,
            size.height as f64 / scale_factor,
        ),
        (
            monitor.0 / scale_factor,
            monitor.1 / scale_factor,
            monitor.2 / scale_factor,
            monitor.3 / scale_factor,
        ),
    ))
}

fn hide_popup_for_window(app: &AppHandle, window: &WebviewWindow) -> Result<bool, String> {
    let drop_id = drop_id_from_label(window.label())?;
    let Some(popup) = app.get_webview_window(&popup_label(drop_id)) else {
        return Ok(false);
    };
    if !popup.is_visible().map_err(|error| error.to_string())? {
        return Ok(false);
    }
    popup.hide().map_err(|error| error.to_string())?;
    Ok(true)
}

fn show_popup_for_window(app: &AppHandle, window: &WebviewWindow) -> Result<(), String> {
    let drop_id = drop_id_from_label(window.label())?;
    let Some(popup) = app.get_webview_window(&popup_label(drop_id)) else {
        return Ok(());
    };
    let (popup_x, popup_y) = resolve_popup_position(app, window)?;
    popup
        .set_position(LogicalPosition::new(popup_x, popup_y))
        .map_err(|error| error.to_string())?;
    popup.show().map_err(|error| error.to_string())?;
    popup.set_focus().map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(target_os = "windows")]
struct DropMoveHook {
    app: AppHandle,
    drop_label: String,
    popup_was_visible: bool,
}

#[cfg(target_os = "windows")]
const DROP_MOVE_SUBCLASS_ID: usize = 0x4452_4f50;

#[cfg(target_os = "windows")]
unsafe extern "system" fn drop_move_subclass_proc(
    hwnd: windows::Win32::Foundation::HWND,
    message: u32,
    wparam: windows::Win32::Foundation::WPARAM,
    lparam: windows::Win32::Foundation::LPARAM,
    subclass_id: usize,
    reference_data: usize,
) -> windows::Win32::Foundation::LRESULT {
    use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        WM_ENTERSIZEMOVE, WM_EXITSIZEMOVE, WM_NCDESTROY,
    };

    let hook_pointer = reference_data as *mut DropMoveHook;
    if !hook_pointer.is_null() {
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // SAFETY: reference_data owns a Box<DropMoveHook> for exactly this HWND and is
            // released only when this callback receives WM_NCDESTROY.
            let hook = unsafe { &mut *hook_pointer };
            match message {
                WM_ENTERSIZEMOVE => {
                    hook.popup_was_visible = hook
                        .app
                        .get_webview_window(&hook.drop_label)
                        .and_then(|window| hide_popup_for_window(&hook.app, &window).ok())
                        .unwrap_or(false);
                    if hook.popup_was_visible {
                        info!("Hid popup while moving {}", hook.drop_label);
                    }
                }
                WM_EXITSIZEMOVE if hook.popup_was_visible => {
                    hook.popup_was_visible = false;
                    let app = hook.app.clone();
                    let drop_label = hook.drop_label.clone();
                    tauri::async_runtime::spawn(async move {
                        let Some(window) = app.get_webview_window(&drop_label) else {
                            return;
                        };
                        if let Err(error) = show_popup_for_window(&app, &window) {
                            tracing::warn!(
                                "Failed to restore popup after moving {drop_label}: {error}"
                            );
                        } else {
                            info!("Restored popup after moving {drop_label}");
                        }
                    });
                }
                _ => {}
            }
        }));
    }

    if message == WM_NCDESTROY && !hook_pointer.is_null() {
        // SAFETY: the subclass and its Box are installed together by install_drop_move_hook.
        let _ = unsafe { RemoveWindowSubclass(hwnd, Some(drop_move_subclass_proc), subclass_id) };
        // SAFETY: WM_NCDESTROY is the single terminal message for this subclass instance.
        drop(unsafe { Box::from_raw(hook_pointer) });
    }

    // SAFETY: every subclass callback must continue the comctl32 subclass chain.
    unsafe { DefSubclassProc(hwnd, message, wparam, lparam) }
}

#[cfg(target_os = "windows")]
fn install_drop_move_hook_now(window: &WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Shell::SetWindowSubclass;

    let hwnd_pointer = window.hwnd().map_err(|error| error.to_string())?;
    let hwnd = HWND(hwnd_pointer.0 as _);
    let hook = Box::new(DropMoveHook {
        app: window.app_handle().clone(),
        drop_label: window.label().to_string(),
        popup_was_visible: false,
    });
    let hook_pointer = Box::into_raw(hook);
    // SAFETY: hook_pointer stays valid until drop_move_subclass_proc handles WM_NCDESTROY.
    let installed = unsafe {
        SetWindowSubclass(
            hwnd,
            Some(drop_move_subclass_proc),
            DROP_MOVE_SUBCLASS_ID,
            hook_pointer as usize,
        )
    };
    if !installed.as_bool() {
        // SAFETY: SetWindowSubclass failed, so ownership never reached the callback.
        drop(unsafe { Box::from_raw(hook_pointer) });
        return Err(format!(
            "SetWindowSubclass failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn install_drop_move_hook(window: &WebviewWindow) -> Result<(), String> {
    let window = window.clone();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    window
        .clone()
        .run_on_main_thread(move || {
            let result = install_drop_move_hook_now(&window);
            let _ = sender.send(result);
        })
        .map_err(|error| error.to_string())?;

    receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .map_err(|error| format!("Timed out installing Drop move hook on main thread: {error}"))?
}

#[tauri::command]
pub fn open_popup_window(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    let drop_id = drop_id_from_label(window.label())?.to_string();
    let popup_label = popup_label(&drop_id);

    let (popup_x, popup_y) = resolve_popup_position(&app, &window)?;

    info!("Opening popup window at ({popup_x}, {popup_y}), size=({POPUP_WIDTH}, {POPUP_HEIGHT})");

    if let Some(popup_window) = app.get_webview_window(&popup_label) {
        popup_window.close().map_err(|e| e.to_string())?;
    } else {
        // Create the popup window
        tauri::async_runtime::spawn(async move {
            let popup_window =
                WebviewWindowBuilder::new(&app, &popup_label, WebviewUrl::App("/popup".into()))
                    .initialization_script(
                        "document.documentElement.classList.add('popup-window');",
                    )
                    .title("File List")
                    .decorations(false) // Remove window decorations for a popup feel
                    .transparent(true)
                    .shadow(false)
                    .resizable(false)
                    .inner_size(POPUP_WIDTH, POPUP_HEIGHT)
                    .position(popup_x, popup_y)
                    .always_on_top(true)
                    .focused(false)
                    .accept_first_mouse(true)
                    .visible_on_all_workspaces(true)
                    .build()
                    .map_err(|e: tauri::Error| e.to_string())?;

            #[cfg(target_os = "windows")]
            if let Err(error) = crate::drop_window::apply_rounded_region(&popup_window) {
                tracing::warn!("Failed to round popup window corners: {error}");
            }

            Ok::<(), String>(())
        });
    }
    Ok(())
}

#[tauri::command]
pub fn hide_popup_for_drop_move(app: AppHandle, window: WebviewWindow) -> Result<bool, String> {
    hide_popup_for_window(&app, &window)
}

#[tauri::command]
pub fn show_popup_after_drop_move(app: AppHandle, window: WebviewWindow) -> Result<(), String> {
    show_popup_for_window(&app, &window)
}

#[tauri::command]
pub fn close_popup_window(window: WebviewWindow) -> Result<(), String> {
    if !window
        .label()
        .starts_with(crate::drop_registry::POPUP_LABEL_PREFIX)
    {
        return Err("Current window is not a Drop popup".to_string());
    }
    window.close().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn open_settings_window(app: AppHandle) -> Result<(), String> {
    // Define settings window dimensions
    let settings_width = 500.0;
    let settings_height = 600.0;

    if let Some(settings_window) = app.get_webview_window("settings") {
        settings_window.show().map_err(|e| e.to_string())?;
        settings_window.unminimize().map_err(|e| e.to_string())?;
        settings_window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    tauri::async_runtime::spawn(async move {
        let builder =
            WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("settings".into()))
                .title("DropWin Settings")
                .inner_size(settings_width, settings_height)
                .min_inner_size(460.0, 520.0)
                .focused(true)
                .center();

        #[cfg(target_os = "windows")]
        let builder = builder
            .decorations(true)
            .shadow(true)
            .resizable(true)
            .maximizable(true)
            .minimizable(true)
            .disable_drag_drop_handler();

        #[cfg(not(target_os = "windows"))]
        let builder = builder
            .decorations(false)
            .shadow(false)
            .resizable(false)
            .visible_on_all_workspaces(true);

        let settings_window = builder.build().map_err(|e: tauri::Error| e.to_string())?;

        #[cfg(target_os = "windows")]
        crate::custom_drop::register_settings_drop_target(&settings_window)?;

        Ok::<(), String>(())
    });
    Ok(())
}

#[tauri::command]
pub fn close_settings_window(app: AppHandle) -> Result<(), String> {
    if let Some(settings_window) = app.get_webview_window("settings") {
        settings_window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::calculate_popup_position;

    #[test]
    fn popup_below_top_right_drop_is_clamped_inside_monitor() {
        let position =
            calculate_popup_position((1764.0, 12.0, 156.0, 156.0), (0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(position, (1458.0, 173.0));
    }

    #[test]
    fn popup_moves_above_a_drop_near_the_bottom_edge() {
        let position =
            calculate_popup_position((1764.0, 912.0, 156.0, 156.0), (0.0, 0.0, 1920.0, 1080.0));
        assert_eq!(position, (1458.0, 557.0));
    }

    #[test]
    fn popup_clamps_to_negative_origin_monitor() {
        let position = calculate_popup_position(
            (-1920.0, 12.0, 156.0, 156.0),
            (-1920.0, 0.0, 1920.0, 1080.0),
        );
        assert_eq!(position, (-1908.0, 173.0));
    }
}

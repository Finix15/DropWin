use crate::config::{AppConfig, DropSize, RuntimeDropSize, MIN_DROP_OPACITY};
use crate::drop_registry::{drop_label, popup_label, DropBounds, DropRegistry};
use std::sync::{Arc, Mutex};
#[cfg(target_os = "windows")]
use tauri::webview::Color;
#[cfg(not(target_os = "windows"))]
use tauri::window::{Effect, EffectState, EffectsBuilder};
use tauri::{
    webview::PageLoadEvent, AppHandle, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::{error, info, warn};
use uuid::Uuid;

const DROP_WIDTH: f64 = 156.0;
const DROP_HEIGHT: f64 = 156.0;
const POSITION_STEP: f64 = 24.0;
const EMPTY_DROP_CLOSE_DELAY_MS: u64 = 250;
const DROP_CORNER_RADIUS: f64 = 16.0;

fn logical_drop_size(drop_size: DropSize) -> (f64, f64) {
    let scale = drop_size.scale();
    (DROP_WIDTH * scale, DROP_HEIGHT * scale)
}

fn drop_initialization_script(drop_scale: f64, opacity: f64, corner_radius: f64) -> String {
    let inverse_scale = 100.0 / drop_scale;
    format!(
        "document.documentElement.classList.add('drop-window');document.documentElement.style.setProperty('--drop-opacity','{opacity}');document.documentElement.style.setProperty('--drop-corner-radius','{corner_radius}px');document.documentElement.style.setProperty('--drop-content-scale','{drop_scale}');document.documentElement.style.setProperty('--drop-content-size','{inverse_scale}%');"
    )
}

#[cfg(target_os = "windows")]
#[derive(Clone, Default)]
pub struct MouseDropPool {
    prepared_drop_id: Arc<Mutex<Option<String>>>,
}

#[cfg(target_os = "windows")]
impl MouseDropPool {
    fn take(&self) -> Result<Option<String>, String> {
        self.prepared_drop_id
            .lock()
            .map_err(|_| "Failed to lock prepared Drop".to_string())
            .map(|mut prepared| prepared.take())
    }

    fn store(&self, drop_id: String) -> Result<bool, String> {
        let mut prepared = self
            .prepared_drop_id
            .lock()
            .map_err(|_| "Failed to lock prepared Drop".to_string())?;
        if prepared.is_some() {
            return Ok(false);
        }
        *prepared = Some(drop_id);
        Ok(true)
    }

    fn is_ready(&self) -> Result<bool, String> {
        self.prepared_drop_id
            .lock()
            .map_err(|_| "Failed to lock prepared Drop".to_string())
            .map(|prepared| prepared.is_some())
    }
}

#[cfg(target_os = "windows")]
pub fn apply_rounded_region(window: &WebviewWindow) -> Result<(), String> {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::Graphics::Dwm::{
        DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_COLOR_NONE,
    };
    use windows::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ};

    let size = window.inner_size().map_err(|error| error.to_string())?;
    let scale_factor = window.scale_factor().map_err(|error| error.to_string())?;
    let logical_width = size.width as f64 / scale_factor;
    let drop_scale = (logical_width / DROP_WIDTH).max(1.0);
    let diameter = (DROP_CORNER_RADIUS * 2.0 * scale_factor * drop_scale).round() as i32;
    let hwnd_ptr = window.hwnd().map_err(|error| error.to_string())?;
    let hwnd = HWND(hwnd_ptr.0 as _);

    unsafe {
        let border_color = DWMWA_COLOR_NONE;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_BORDER_COLOR,
            std::ptr::from_ref(&border_color).cast(),
            std::mem::size_of_val(&border_color) as u32,
        );

        let region = CreateRoundRectRgn(
            0,
            0,
            size.width.saturating_add(1) as i32,
            size.height.saturating_add(1) as i32,
            diameter,
            diameter,
        );
        if region.0.is_null() {
            return Err("CreateRoundRectRgn returned a null region".to_string());
        }
        if SetWindowRgn(hwnd, Some(region), true) == 0 {
            let _ = DeleteObject(HGDIOBJ(region.0));
            return Err("SetWindowRgn failed".to_string());
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn apply_window_opacity(window: &WebviewWindow, opacity: u8) -> Result<(), String> {
    let alpha = ((opacity.clamp(MIN_DROP_OPACITY, 100) as u16 * 255) / 100) as u8;
    apply_window_alpha(window, alpha)
}

#[cfg(target_os = "windows")]
fn apply_window_alpha(window: &WebviewWindow, alpha: u8) -> Result<(), String> {
    use windows::Win32::Foundation::{COLORREF, HWND};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindowLongPtrW, SetLayeredWindowAttributes, SetWindowLongPtrW, GWL_EXSTYLE, LWA_ALPHA,
        WS_EX_LAYERED,
    };

    let hwnd_ptr = window.hwnd().map_err(|error| error.to_string())?;
    let hwnd = HWND(hwnd_ptr.0 as _);

    unsafe {
        let extended_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let _ = SetWindowLongPtrW(hwnd, GWL_EXSTYLE, extended_style | WS_EX_LAYERED.0 as isize);
        SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
async fn animate_window_opacity(window: WebviewWindow, opacity: u8) {
    const FRAMES: u32 = 18;
    let target_alpha = ((opacity.clamp(MIN_DROP_OPACITY, 100) as u16 * 255) / 100) as f64;

    for frame in 0..=FRAMES {
        let progress = frame as f64 / FRAMES as f64;
        let eased = 1.0 - (1.0 - progress).powi(3);
        let alpha = (target_alpha * eased).round() as u8;
        if apply_window_alpha(&window, alpha).is_err() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(16)).await;
    }
}

fn position_is_free(candidate: (f64, f64), occupied: &[DropBounds], drop_size: (f64, f64)) -> bool {
    occupied.iter().all(|bounds| {
        candidate.0 + drop_size.0 + POSITION_STEP <= bounds.position.0
            || bounds.position.0 + bounds.size.0 + POSITION_STEP <= candidate.0
            || candidate.1 + drop_size.1 + POSITION_STEP <= bounds.position.1
            || bounds.position.1 + bounds.size.1 + POSITION_STEP <= candidate.1
    })
}

pub fn choose_drop_position(
    desired: (f64, f64),
    bounds: (f64, f64, f64, f64),
    occupied: &[DropBounds],
    drop_size: (f64, f64),
) -> (f64, f64) {
    let (left, top, width, height) = bounds;
    let max_x = (left + width - drop_size.0).max(left);
    let max_y = (top + height - drop_size.1).max(top);
    let clamp =
        |position: (f64, f64)| (position.0.clamp(left, max_x), position.1.clamp(top, max_y));

    let origin = clamp(desired);
    if position_is_free(origin, occupied, drop_size) {
        return origin;
    }

    for radius in 1..=64 {
        let offset = radius as f64 * POSITION_STEP;
        let candidates = [
            (origin.0 + offset, origin.1 + offset),
            (origin.0 - offset, origin.1 + offset),
            (origin.0 + offset, origin.1 - offset),
            (origin.0 - offset, origin.1 - offset),
            (origin.0 + offset, origin.1),
            (origin.0 - offset, origin.1),
            (origin.0, origin.1 + offset),
            (origin.0, origin.1 - offset),
        ];
        for candidate in candidates {
            let candidate = clamp(candidate);
            if position_is_free(candidate, occupied, drop_size) {
                return candidate;
            }
        }
    }

    origin
}

fn monitor_geometry(app: &AppHandle, cursor: (f64, f64)) -> ((f64, f64, f64, f64), f64) {
    let monitor = app
        .monitor_from_point(cursor.0, cursor.1)
        .ok()
        .flatten()
        .or_else(|| app.primary_monitor().ok().flatten());
    monitor.map_or(((0.0, 0.0, 1920.0, 1080.0), 1.0), |monitor| {
        let position = monitor.position();
        let size = monitor.size();
        (
            (
                position.x as f64,
                position.y as f64,
                size.width as f64,
                size.height as f64,
            ),
            monitor.scale_factor(),
        )
    })
}

pub fn cursor_position(app: &AppHandle) -> (f64, f64) {
    app.cursor_position()
        .map(|position| (position.x, position.y))
        .unwrap_or((0.0, 0.0))
}

#[cfg(target_os = "windows")]
pub fn prepare_mouse_drop_window(app: AppHandle) -> Result<(), String> {
    let pool = app.state::<MouseDropPool>().inner().clone();
    if pool.is_ready()? {
        return Ok(());
    }

    let drop_id = Uuid::new_v4().simple().to_string();
    let label = drop_label(&drop_id);
    let registry = app.state::<DropRegistry>().inner().clone();
    registry.create_prepared(drop_id.clone())?;

    let runtime_drop_size = app.state::<RuntimeDropSize>().0;
    let drop_scale = runtime_drop_size.scale();
    let logical_size = logical_drop_size(runtime_drop_size);
    let opacity = app
        .state::<Arc<Mutex<AppConfig>>>()
        .lock()
        .map(|config| config.drop_opacity.clamp(MIN_DROP_OPACITY, 100))
        .unwrap_or(88);
    let initialization_script = drop_initialization_script(drop_scale, 1.0, 0.0);
    let build_result = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("/".into()))
        .title("DropWin")
        .inner_size(logical_size.0, logical_size.1)
        .decorations(false)
        .shadow(false)
        .initialization_script(initialization_script)
        .focused(false)
        .visible(false)
        .always_on_top(true)
        .accept_first_mouse(true)
        .skip_taskbar(true)
        .resizable(false)
        .visible_on_all_workspaces(true)
        .disable_drag_drop_handler()
        .transparent(false)
        .background_color(Color(23, 23, 26, 255))
        .build();

    let window = match build_result {
        Ok(window) => window,
        Err(error) => {
            let _ = registry.remove(&drop_id);
            return Err(format!("Failed to prepare {label}: {error}"));
        }
    };

    if let Err(error) = window.set_position(PhysicalPosition::new(-32_000, -32_000)) {
        let _ = registry.remove(&drop_id);
        let _ = window.close();
        return Err(format!("Failed to park prepared {label}: {error}"));
    }
    if let Err(error) = window.show() {
        let _ = registry.remove(&drop_id);
        let _ = window.close();
        return Err(format!("Failed to prime prepared {label}: {error}"));
    }
    if let Err(error) = apply_window_opacity(&window, opacity) {
        warn!("Failed to apply opacity to prepared {label}: {error}");
    }
    if let Err(error) = crate::custom_drop::register_drop_target_now(&window) {
        warn!("Failed to register native Drop target for prepared {label}: {error}");
    }
    if let Err(error) = crate::commands::window_ops::install_drop_move_hook(&window) {
        warn!("Failed to install move hook for prepared {label}: {error}");
    }
    if let Err(error) = window.hide() {
        let _ = registry.remove(&drop_id);
        let _ = window.close();
        return Err(format!("Failed to hide prepared {label}: {error}"));
    }
    if let Err(error) = apply_rounded_region(&window) {
        warn!("Failed to shape prepared {label}: {error}");
    }
    if !pool.store(drop_id.clone())? {
        let _ = registry.remove(&drop_id);
        let _ = window.close();
        return Ok(());
    }

    info!("Prepared {label} for the next mouse drag");
    Ok(())
}

#[cfg(target_os = "windows")]
fn activate_prepared_mouse_drop(
    app: &AppHandle,
    desired_position: (f64, f64),
    focused: bool,
) -> Result<Option<String>, String> {
    let pool = app.state::<MouseDropPool>().inner().clone();
    let Some(drop_id) = pool.take()? else {
        return Ok(None);
    };
    let label = drop_label(&drop_id);
    let registry = app.state::<DropRegistry>().inner().clone();
    let Some(window) = app.get_webview_window(&label) else {
        let _ = registry.remove(&drop_id);
        return Ok(None);
    };

    let occupied = registry.bounds()?;
    let (bounds, _scale_factor) = monitor_geometry(app, desired_position);
    let window_size = window.inner_size().map_err(|error| error.to_string())?;
    let drop_size = (window_size.width as f64, window_size.height as f64);
    let centered_position = (
        desired_position.0 - drop_size.0 / 2.0,
        desired_position.1 - drop_size.1 / 2.0,
    );
    let position = choose_drop_position(centered_position, bounds, &occupied, drop_size);
    registry.activate_prepared(&drop_id, position, drop_size)?;

    let opacity = app
        .state::<Arc<Mutex<AppConfig>>>()
        .lock()
        .map(|config| config.drop_opacity.clamp(MIN_DROP_OPACITY, 100))
        .unwrap_or(88);
    if let Err(error) = apply_window_opacity(&window, opacity) {
        warn!("Failed to apply opacity to {label}: {error}");
    }
    if let Err(error) = window.set_position(PhysicalPosition::new(
        position.0.round() as i32,
        position.1.round() as i32,
    )) {
        let _ = registry.remove(&drop_id);
        let _ = window.close();
        return Err(format!("Failed to position {label}: {error}"));
    }
    if let Err(error) = apply_rounded_region(&window) {
        warn!("Failed to shape {label}: {error}");
    }
    if let Err(error) = window.show() {
        let _ = registry.remove(&drop_id);
        let _ = window.close();
        return Err(format!("Failed to show {label}: {error}"));
    }
    // A prepared WebView is primed, hidden, and reused later. Showing that HWND can reset
    // its layered alpha to fully opaque, so reapply the current saved value after show.
    if let Err(error) = apply_window_opacity(&window, opacity) {
        warn!("Failed to restore opacity after showing {label}: {error}");
    }
    let _ = window.eval(
        "document.documentElement.classList.remove('drop-visible');requestAnimationFrame(() => requestAnimationFrame(() => document.documentElement.classList.add('drop-visible')))"
    );
    if focused {
        let _ = window.set_focus();
    }
    info!(
        "Activated {label} at ({}, {}) via mouse_shake",
        position.0, position.1
    );

    Ok(Some(drop_id))
}

pub fn create_drop_window(
    app: AppHandle,
    desired_position: (f64, f64),
    focused: bool,
    opened_by: &'static str,
) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    if opened_by == "mouse_shake" {
        if let Some(drop_id) = activate_prepared_mouse_drop(&app, desired_position, focused)? {
            return Ok(drop_id);
        }
        warn!("Prepared mouse Drop was unavailable; using dynamic creation");
    }

    let registry = app.state::<DropRegistry>().inner().clone();
    let occupied = registry.bounds()?;
    let runtime_drop_size = app.state::<RuntimeDropSize>().0;
    let drop_scale = runtime_drop_size.scale();
    let logical_size = logical_drop_size(runtime_drop_size);
    let drop_opacity = app
        .state::<Arc<Mutex<AppConfig>>>()
        .lock()
        .map(|config| config.drop_opacity.clamp(MIN_DROP_OPACITY, 100))
        .unwrap_or(88);
    let (bounds, scale_factor) = monitor_geometry(&app, desired_position);
    let drop_size = (logical_size.0 * scale_factor, logical_size.1 * scale_factor);
    let centered_position = (
        desired_position.0 - drop_size.0 / 2.0,
        desired_position.1 - drop_size.1 / 2.0,
    );
    let position = choose_drop_position(centered_position, bounds, &occupied, drop_size);
    let drop_id = Uuid::new_v4().simple().to_string();
    let label = drop_label(&drop_id);
    #[cfg(target_os = "windows")]
    let css_corner_radius = 0;
    #[cfg(not(target_os = "windows"))]
    let css_corner_radius = DROP_CORNER_RADIUS as i32;
    #[cfg(target_os = "windows")]
    let css_opacity = 1.0;
    #[cfg(not(target_os = "windows"))]
    let css_opacity = drop_opacity as f64 / 100.0;
    let initialization_script =
        drop_initialization_script(drop_scale, css_opacity, css_corner_radius as f64);
    registry.create_with_size(drop_id.clone(), position, drop_size)?;

    let created_drop_id = drop_id.clone();
    tauri::async_runtime::spawn(async move {
        if !registry.contains(&drop_id).unwrap_or(false) {
            return;
        }

        let ready_registry = registry.clone();
        let ready_drop_id = drop_id.clone();
        let builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("/".into()))
            .title("DropWin")
            .inner_size(logical_size.0, logical_size.1)
            .decorations(false)
            .shadow(false)
            .initialization_script(initialization_script)
            .focused(false)
            .visible(false)
            .always_on_top(true)
            .accept_first_mouse(true)
            .skip_taskbar(true)
            .resizable(false)
            .visible_on_all_workspaces(true)
            .on_page_load(move |window, payload| {
                if payload.event() != PageLoadEvent::Finished {
                    return;
                }

                let ready_registry = ready_registry.clone();
                let ready_drop_id = ready_drop_id.clone();
                tauri::async_runtime::spawn(async move {
                    // Give WebView one frame to composite React and the resolved theme while hidden.
                    tokio::time::sleep(std::time::Duration::from_millis(16)).await;
                    if !ready_registry.contains(&ready_drop_id).unwrap_or(false) {
                        let _ = window.close();
                        return;
                    }
                    #[cfg(target_os = "windows")]
                    if let Err(error) = apply_rounded_region(&window) {
                        warn!("Failed to shape {}: {error}", window.label());
                    }
                    #[cfg(target_os = "windows")]
                    if let Err(error) = apply_window_alpha(&window, 0) {
                        warn!("Failed to prepare opacity for {}: {error}", window.label());
                    }
                    if let Err(error) = window.set_position(PhysicalPosition::new(
                        position.0.round() as i32,
                        position.1.round() as i32,
                    )) {
                        warn!("Failed to position {}: {error}", window.label());
                        return;
                    }
                    if let Err(error) = window.show() {
                        warn!("Failed to show {}: {error}", window.label());
                        return;
                    }
                    let _ = window.eval(
                        "requestAnimationFrame(() => requestAnimationFrame(() => document.documentElement.classList.add('drop-visible')))",
                    );
                    #[cfg(target_os = "windows")]
                    tauri::async_runtime::spawn(animate_window_opacity(window.clone(), drop_opacity));
                    if focused {
                        let _ = window.set_focus();
                    }
                });
            });
        #[cfg(target_os = "windows")]
        let builder = builder
            .disable_drag_drop_handler()
            .transparent(false)
            .background_color(Color(23, 23, 26, 255));
        #[cfg(not(target_os = "windows"))]
        let builder = builder.transparent(true).effects(
            EffectsBuilder::new()
                .effect(Effect::HudWindow)
                .state(EffectState::Active)
                .radius(DROP_CORNER_RADIUS * drop_scale)
                .build(),
        );
        let build_result = builder.build();

        match build_result {
            Ok(window) => {
                #[cfg(target_os = "windows")]
                if let Err(error) = apply_rounded_region(&window) {
                    warn!("Failed to shape {label}: {error}");
                }
                #[cfg(target_os = "windows")]
                if let Err(error) = crate::custom_drop::register_drop_target(&window) {
                    warn!("Failed to register native Drop target for {label}: {error}");
                }
                #[cfg(target_os = "windows")]
                if let Err(error) = crate::commands::window_ops::install_drop_move_hook(&window) {
                    warn!("Failed to install move hook for {label}: {error}");
                }
                #[cfg(target_os = "windows")]
                if let Err(error) = apply_window_alpha(&window, 0) {
                    warn!("Failed to prepare entrance opacity for {label}: {error}");
                }
                info!(
                    "Created {label} at ({}, {}) via {opened_by}",
                    position.0, position.1
                );
            }
            Err(error) => {
                let _ = registry.remove(&drop_id);
                error!("Failed to build {label}: {error}");
            }
        }
    });

    Ok(created_drop_id)
}

pub fn close_empty_drop_after_release(app: AppHandle, drop_id: String) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(EMPTY_DROP_CLOSE_DELAY_MS)).await;

        let registry = app.state::<DropRegistry>();
        let removed = match registry.remove_if_empty_and_unreceived(&drop_id) {
            Ok(removed) => removed.is_some(),
            Err(error) => {
                warn!("Failed to inspect empty Drop {drop_id}: {error}");
                false
            }
        };
        if !removed {
            return;
        }

        let popup = popup_label(&drop_id);
        if let Some(window) = app.get_webview_window(&popup) {
            let _ = window.close();
        }
        if let Some(window) = app.get_webview_window(&drop_label(&drop_id)) {
            let _ = window.close();
        }
        info!("Closed empty Drop {drop_id} after drag release");
    });
}

pub fn close_drop(app: &AppHandle, drop_id: &str) {
    let popup = popup_label(drop_id);
    if let Some(window) = app.get_webview_window(&popup) {
        let _ = window.close();
    }
    let registry: State<'_, DropRegistry> = app.state();
    if let Err(error) = registry.remove(drop_id) {
        warn!("Failed to remove Drop state {drop_id}: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_overlapping_drop_and_stays_in_bounds() {
        let occupied = vec![DropBounds {
            position: (100.0, 100.0),
            size: (DROP_WIDTH, DROP_HEIGHT),
        }];
        let drop_size = (DROP_WIDTH, DROP_HEIGHT);
        let position = choose_drop_position(
            (100.0, 100.0),
            (0.0, 0.0, 500.0, 500.0),
            &occupied,
            drop_size,
        );
        assert_ne!(position, (100.0, 100.0));
        assert!(position.0 >= 0.0 && position.0 <= 335.0);
        assert!(position.1 >= 0.0 && position.1 <= 325.0);
    }

    #[test]
    fn keeps_the_first_drop_centered_on_the_cursor() {
        let cursor = (500.0, 400.0);
        let drop_size = (DROP_WIDTH, DROP_HEIGHT);
        let centered = (cursor.0 - drop_size.0 / 2.0, cursor.1 - drop_size.1 / 2.0);
        let position = choose_drop_position(centered, (0.0, 0.0, 1920.0, 1080.0), &[], drop_size);
        assert_eq!(position, centered);
    }

    #[test]
    fn maps_drop_sizes_to_the_requested_dimensions() {
        assert_eq!(logical_drop_size(DropSize::Small), (156.0, 156.0));
        assert_eq!(logical_drop_size(DropSize::Medium), (187.2, 187.2));
        assert_eq!(logical_drop_size(DropSize::Large), (234.0, 234.0));
    }

    #[test]
    fn avoids_occupied_drops_with_different_sizes() {
        let occupied = vec![DropBounds {
            position: (100.0, 100.0),
            size: (234.0, 234.0),
        }];
        let position = choose_drop_position(
            (120.0, 120.0),
            (0.0, 0.0, 800.0, 800.0),
            &occupied,
            (156.0, 156.0),
        );

        assert!(
            position.0 + 156.0 + POSITION_STEP <= 100.0
                || 100.0 + 234.0 + POSITION_STEP <= position.0
                || position.1 + 156.0 + POSITION_STEP <= 100.0
                || 100.0 + 234.0 + POSITION_STEP <= position.1
        );
    }
}

#![cfg(target_os = "windows")]

use crate::drop_registry::{drop_id_from_label, drop_label, DropRegistry};
use crate::internal_drag::InternalDragState;
use serde::Serialize;
use std::cell::Cell;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use tauri::{Emitter, Manager, WebviewWindow};
use tracing::{info, warn};
use windows::core::{implement, Result as WindowsResult};
use windows::Win32::Foundation::{HWND, POINT, POINTL};
use windows::Win32::System::Com::{
    CoCreateInstance, IDataObject, CLSCTX_INPROC_SERVER, DVASPECT_CONTENT, FORMATETC, STGMEDIUM,
    TYMED_HGLOBAL,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, ReleaseStgMedium, DROPEFFECT, DROPEFFECT_COPY,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::Shell::{CLSID_DragDropHelper, DragQueryFileW, IDropTargetHelper, HDROP};

#[derive(Clone, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum DropPayload {
    Text(String),
    Html(String),
}

struct StgMediumGuard(STGMEDIUM);

impl Drop for StgMediumGuard {
    fn drop(&mut self) {
        unsafe {
            ReleaseStgMedium(&mut self.0);
        }
    }
}

thread_local! {
    static OLE_INITIALIZED: Cell<bool> = const { Cell::new(false) };
}

fn shell_drag_point(point: &POINTL) -> POINT {
    POINT {
        x: point.x,
        y: point.y,
    }
}

fn native_drop_effect() -> DROPEFFECT {
    DROPEFFECT_COPY
}

struct DropTargetVisualHelper {
    hwnd: HWND,
    helper: Option<IDropTargetHelper>,
    failure_logged: Cell<bool>,
}

impl DropTargetVisualHelper {
    fn new(hwnd: HWND, target_label: &str) -> Self {
        let helper = unsafe {
            CoCreateInstance::<_, IDropTargetHelper>(
                &CLSID_DragDropHelper,
                None,
                CLSCTX_INPROC_SERVER,
            )
        }
        .map_err(|error| {
            warn!("Failed to create drag image helper for {target_label}: {error}");
            error
        })
        .ok();

        Self {
            hwnd,
            helper,
            failure_logged: Cell::new(false),
        }
    }

    fn report_failure(&self, action: &str, result: WindowsResult<()>) {
        if let Err(error) = result {
            if !self.failure_logged.replace(true) {
                warn!("Failed to update native drag image during {action}: {error}");
            }
        }
    }

    unsafe fn drag_enter(&self, data_object: &IDataObject, point: &POINTL) {
        if let Some(helper) = &self.helper {
            let point = shell_drag_point(point);
            self.report_failure(
                "DragEnter",
                helper.DragEnter(self.hwnd, data_object, &point, native_drop_effect()),
            );
        }
    }

    unsafe fn drag_over(&self, point: &POINTL) {
        if let Some(helper) = &self.helper {
            let point = shell_drag_point(point);
            self.report_failure("DragOver", helper.DragOver(&point, native_drop_effect()));
        }
    }

    unsafe fn drag_leave(&self) {
        if let Some(helper) = &self.helper {
            self.report_failure("DragLeave", helper.DragLeave());
        }
    }

    unsafe fn drop(&self, data_object: &IDataObject, point: &POINTL) {
        if let Some(helper) = &self.helper {
            let point = shell_drag_point(point);
            self.report_failure(
                "Drop",
                helper.Drop(data_object, &point, native_drop_effect()),
            );
        }
    }
}

#[implement(IDropTarget)]
pub struct CustomDropTarget {
    window: WebviewWindow,
    visual_helper: DropTargetVisualHelper,
}

impl CustomDropTarget {
    pub fn new(window: WebviewWindow, hwnd: HWND) -> Self {
        let visual_helper = DropTargetVisualHelper::new(hwnd, window.label());
        Self {
            window,
            visual_helper,
        }
    }

    fn target_drop_id(&self) -> Result<String, String> {
        drop_id_from_label(self.window.label()).map(str::to_owned)
    }

    fn emit_payload(&self, payload: DropPayload) {
        let Ok(drop_id) = self.target_drop_id() else {
            return;
        };
        let registry = self.window.app_handle().state::<DropRegistry>();
        if let Err(error) = registry.mark_content_received(&drop_id) {
            warn!("Failed to mark Drop {drop_id} as having received content: {error}");
            return;
        }

        let app = self.window.app_handle().clone();
        tauri::async_runtime::spawn(async move {
            if let Err(error) = app.emit_to(drop_label(&drop_id), "native_drop", payload) {
                warn!("Failed to emit native_drop event: {error}");
            }
        });
    }

    fn handle_files(&self, files: Vec<String>) {
        if files.is_empty() {
            return;
        }
        let Ok(drop_id) = self.target_drop_id() else {
            return;
        };
        let app_handle = self.window.app_handle().clone();
        let registry = app_handle.state::<DropRegistry>().inner().clone();
        if let Err(error) = registry.mark_content_received(&drop_id) {
            warn!("Failed to reserve Drop {drop_id} for native paths: {error}");
            return;
        }
        let paths: Vec<PathBuf> = files.into_iter().map(PathBuf::from).collect();
        info!("Received {} native path(s) for Drop {drop_id}", paths.len());
        crate::file_drop::handle_file_drop_from_paths(paths, drop_id, registry, app_handle);
    }

    fn reserve_drop_for_payload(&self) {
        let Ok(drop_id) = self.target_drop_id() else {
            return;
        };
        let registry = self.window.app_handle().state::<DropRegistry>();
        if let Err(error) = registry.mark_content_received(&drop_id) {
            warn!("Failed to reserve Drop {drop_id} for native payload: {error}");
        }
    }

    fn record_internal_drop_target(&self) -> bool {
        let Ok(drop_id) = self.target_drop_id() else {
            return false;
        };
        let drag_state = self.window.app_handle().state::<InternalDragState>();
        match drag_state.record_target(&drop_id) {
            Ok(recorded) => recorded,
            Err(error) => {
                warn!("Failed to record internal drag target {drop_id}: {error}");
                false
            }
        }
    }

    fn emit_drag_state(&self, active: bool) {
        let window = self.window.clone();
        tauri::async_runtime::spawn(async move {
            let label = window.label().to_owned();
            if let Err(error) = window.emit_to(label, "native_drag_state", active) {
                warn!("Failed to emit native_drag_state event: {error}");
            }
        });
    }

    unsafe fn extract_files(pdataobj: &IDataObject) -> Option<Vec<String>> {
        let fmt = FORMATETC {
            cfFormat: 15, // CF_HDROP
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };

        if let Ok(stg) = pdataobj.GetData(&fmt) {
            let stg = StgMediumGuard(stg);
            let hglobal = stg.0.u.hGlobal;
            if !hglobal.is_invalid() {
                let hdrop = HDROP(hglobal.0);
                let count = DragQueryFileW(hdrop, 0xFFFFFFFF, None);
                let mut files = Vec::new();

                for i in 0..count {
                    let len = DragQueryFileW(hdrop, i, None);
                    if len > 0 {
                        let mut buffer: Vec<u16> = vec![0; (len + 1) as usize];
                        DragQueryFileW(hdrop, i, Some(&mut buffer));

                        if let Some(pos) = buffer.iter().position(|&c| c == 0) {
                            buffer.truncate(pos);
                        }

                        let path = OsString::from_wide(&buffer).to_string_lossy().into_owned();
                        files.push(path);
                    }
                }

                return Some(files);
            }
        }
        None
    }

    unsafe fn extract_text(&self, pdataobj: &IDataObject) -> Option<String> {
        // Try CF_UNICODETEXT first
        let mut fmt = FORMATETC {
            cfFormat: 13, // CF_UNICODETEXT
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };

        match pdataobj.GetData(&fmt) {
            Ok(stg) => {
                let stg = StgMediumGuard(stg);
                let hglobal = stg.0.u.hGlobal;
                if !hglobal.is_invalid() {
                    let ptr = GlobalLock(hglobal) as *const u16;
                    if !ptr.is_null() {
                        let size = GlobalSize(hglobal);
                        let count = size / 2;
                        let slice = std::slice::from_raw_parts(ptr, count as usize);
                        let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
                        let text = String::from_utf16_lossy(&slice[..end]);
                        let _ = GlobalUnlock(hglobal);
                        return Some(text);
                    } else {
                        println!("GlobalLock failed for CF_UNICODETEXT");
                    }
                }
            }
            Err(e) => println!("GetData failed for CF_UNICODETEXT: {}", e),
        }

        // Try CF_TEXT (1)
        fmt.cfFormat = 1; // CF_TEXT
        match pdataobj.GetData(&fmt) {
            Ok(stg) => {
                let stg = StgMediumGuard(stg);
                let hglobal = stg.0.u.hGlobal;
                if !hglobal.is_invalid() {
                    let ptr = GlobalLock(hglobal) as *const u8;
                    if !ptr.is_null() {
                        let size = GlobalSize(hglobal);
                        let slice = std::slice::from_raw_parts(ptr, size as usize);
                        let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
                        let text = String::from_utf8_lossy(&slice[..end]).into_owned();
                        let _ = GlobalUnlock(hglobal);
                        return Some(text);
                    } else {
                        println!("GlobalLock failed for CF_TEXT");
                    }
                }
            }
            Err(e) => println!("GetData failed for CF_TEXT: {}", e),
        }

        None
    }

    unsafe fn extract_html(&self, pdataobj: &IDataObject) -> Option<String> {
        let html_fmt_id = windows::Win32::System::DataExchange::RegisterClipboardFormatW(
            windows::core::w!("HTML Format"),
        );
        if html_fmt_id == 0 {
            return None;
        }

        let fmt = FORMATETC {
            cfFormat: html_fmt_id as u16,
            ptd: std::ptr::null_mut(),
            dwAspect: DVASPECT_CONTENT.0,
            lindex: -1,
            tymed: TYMED_HGLOBAL.0 as u32,
        };

        if let Ok(stg) = pdataobj.GetData(&fmt) {
            let stg = StgMediumGuard(stg);
            let hglobal = stg.0.u.hGlobal;
            if !hglobal.is_invalid() {
                let ptr = GlobalLock(hglobal) as *const u8;
                if !ptr.is_null() {
                    let size = GlobalSize(hglobal);
                    let slice = std::slice::from_raw_parts(ptr, size as usize);
                    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
                    let text = String::from_utf8_lossy(&slice[..end]).into_owned();
                    let _ = GlobalUnlock(hglobal);
                    return Some(text);
                }
            }
        }
        None
    }
}

impl IDropTarget_Impl for CustomDropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: windows::core::Ref<'_, IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WindowsResult<()> {
        self.emit_drag_state(true);
        info!("Native drag entered {}; effect=COPY", self.window.label());
        unsafe {
            if !pdweffect.is_null() {
                *pdweffect = native_drop_effect();
            }
            if let Some(data_object) = pdataobj.as_ref() {
                self.visual_helper.drag_enter(data_object, pt);
            }
        }
        Ok(())
    }

    fn DragOver(
        &self,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WindowsResult<()> {
        unsafe {
            if !pdweffect.is_null() {
                *pdweffect = native_drop_effect();
            }
            self.visual_helper.drag_over(pt);
        }
        Ok(())
    }

    fn DragLeave(&self) -> WindowsResult<()> {
        unsafe {
            self.visual_helper.drag_leave();
        }
        self.emit_drag_state(false);
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: windows::core::Ref<'_, IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WindowsResult<()> {
        self.emit_drag_state(false);
        info!("Native payload dropped on {}", self.window.label());
        unsafe {
            if !pdweffect.is_null() {
                *pdweffect = native_drop_effect();
            }

            if let Some(dataobj) = pdataobj.as_ref() {
                self.visual_helper.drop(dataobj, pt);
                // Keep the OLE callback re-entrant and short for Drop-to-Drop moves. The source
                // command transfers registry entries only after DoDragDrop has returned.
                if self.record_internal_drop_target() {
                    return Ok(());
                }
                // Reserve the Drop before asking Explorer to render its IDataObject. Large,
                // network, and cloud folders may need longer than the 250 ms empty-Drop delay.
                self.reserve_drop_for_payload();

                if let Some(files) =
                    CustomDropTarget::extract_files(dataobj).filter(|paths| !paths.is_empty())
                {
                    self.handle_files(files);
                    return Ok(());
                }

                if let Some(html) = self.extract_html(dataobj) {
                    if !html.is_empty() {
                        self.emit_payload(DropPayload::Html(html));
                        return Ok(());
                    }
                }

                if let Some(text) = self.extract_text(dataobj) {
                    if !text.is_empty() {
                        self.emit_payload(DropPayload::Text(text));
                        return Ok(());
                    }
                }
            }
        }
        Ok(())
    }
}

#[implement(IDropTarget)]
struct SettingsDropTarget {
    window: WebviewWindow,
    visual_helper: DropTargetVisualHelper,
}

impl IDropTarget_Impl for SettingsDropTarget_Impl {
    fn DragEnter(
        &self,
        pdataobj: windows::core::Ref<'_, IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WindowsResult<()> {
        info!("Native drag entered Settings; effect=COPY");
        let _ = self.window.emit("settings_native_drag_state", true);
        unsafe {
            if !pdweffect.is_null() {
                *pdweffect = native_drop_effect();
            }
            if let Some(data_object) = pdataobj.as_ref() {
                self.visual_helper.drag_enter(data_object, pt);
            }
        }
        Ok(())
    }

    fn DragOver(
        &self,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WindowsResult<()> {
        unsafe {
            if !pdweffect.is_null() {
                *pdweffect = native_drop_effect();
            }
            self.visual_helper.drag_over(pt);
        }
        Ok(())
    }

    fn DragLeave(&self) -> WindowsResult<()> {
        unsafe {
            self.visual_helper.drag_leave();
        }
        let _ = self.window.emit("settings_native_drag_state", false);
        Ok(())
    }

    fn Drop(
        &self,
        pdataobj: windows::core::Ref<'_, IDataObject>,
        _grfkeystate: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        pdweffect: *mut DROPEFFECT,
    ) -> WindowsResult<()> {
        let _ = self.window.emit("settings_native_drag_state", false);
        unsafe {
            if !pdweffect.is_null() {
                *pdweffect = native_drop_effect();
            }
            if let Some(dataobj) = pdataobj.as_ref() {
                self.visual_helper.drop(dataobj, pt);
                if let Some(paths) =
                    CustomDropTarget::extract_files(dataobj).filter(|paths| !paths.is_empty())
                {
                    if let Err(error) = self.window.emit("settings_native_drop", paths) {
                        warn!("Failed to emit Settings native drop: {error}");
                    }
                }
            }
        }
        Ok(())
    }
}

fn initialize_ole_for_current_thread() -> Result<(), String> {
    use windows::Win32::System::Ole::OleInitialize;

    OLE_INITIALIZED.with(|initialized| -> Result<(), String> {
        if !initialized.get() {
            unsafe { OleInitialize(None) }.map_err(|error| error.to_string())?;
            initialized.set(true);
        }
        Ok(())
    })
}

fn register_webview_child_targets(
    parent_hwnd: windows::Win32::Foundation::HWND,
    mut make_target: impl FnMut(HWND) -> IDropTarget,
) -> Result<usize, String> {
    use std::ffi::c_void;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::System::Ole::{RegisterDragDrop, RevokeDragDrop};
    use windows::Win32::UI::WindowsAndMessaging::EnumChildWindows;

    let mut registered = 0_usize;
    let mut registration_errors = Vec::new();
    {
        let mut register_child = |child_hwnd: HWND| -> bool {
            unsafe {
                // WebView2 owns the OLE target on its child HWND. Revoke it before installing
                // DropWin's target so physical files are handled as CF_HDROP paths instead of
                // being exposed to JavaScript as pathless File objects.
                let _ = RevokeDragDrop(child_hwnd);
                let target = make_target(child_hwnd);
                match RegisterDragDrop(child_hwnd, &target) {
                    Ok(()) => registered += 1,
                    Err(error) => registration_errors.push(error.to_string()),
                }
            }
            true
        };

        let mut callback: &mut dyn FnMut(HWND) -> bool = &mut register_child;
        let callback_pointer: *mut c_void = unsafe { std::mem::transmute(&mut callback) };
        let callback_parameter = LPARAM(callback_pointer as isize);

        unsafe extern "system" fn enumerate_child(hwnd: HWND, parameter: LPARAM) -> BOOL {
            let callback = &mut *(parameter.0 as *mut c_void as *mut &mut dyn FnMut(HWND) -> bool);
            callback(hwnd).into()
        }

        let enumerated = unsafe {
            EnumChildWindows(Some(parent_hwnd), Some(enumerate_child), callback_parameter)
        };
        if !enumerated.as_bool() {
            return Err(format!(
                "Failed to enumerate WebView child windows: {}",
                std::io::Error::last_os_error()
            ));
        }
    }

    if registered == 0 {
        let details = if registration_errors.is_empty() {
            "no WebView child HWND was found".to_string()
        } else {
            registration_errors.join("; ")
        };
        return Err(format!(
            "Failed to register a native WebView drop target: {details}"
        ));
    }

    Ok(registered)
}

fn configure_and_register_webview_targets(
    window: &WebviewWindow,
    register_targets: impl FnOnce(windows::Win32::Foundation::HWND) -> Result<usize, String>
        + Send
        + 'static,
) -> Result<usize, String> {
    use std::sync::{Arc, Mutex};
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller4;
    use windows::core::Interface;
    use windows::Win32::Foundation::HWND;

    let result = Arc::new(Mutex::new(None));
    let callback_result = result.clone();
    window
        .with_webview(move |webview| {
            let registration_result = (|| -> Result<usize, String> {
                let controller = webview.controller();
                let controller4: ICoreWebView2Controller4 = controller
                    .cast()
                    .map_err(|error| format!("Failed to access WebView2 controller v4: {error}"))?;
                unsafe { controller4.SetAllowExternalDrop(false) }.map_err(|error| {
                    format!("Failed to disable WebView2 external drop handling: {error}")
                })?;

                let mut container_hwnd = HWND::default();
                unsafe { controller.ParentWindow(&mut container_hwnd) }.map_err(|error| {
                    format!("Failed to resolve the WebView2 container HWND: {error}")
                })?;
                if container_hwnd == HWND::default() {
                    return Err("WebView2 returned an empty container HWND".to_string());
                }

                initialize_ole_for_current_thread()?;
                register_targets(container_hwnd)
            })();

            if let Ok(mut result) = callback_result.lock() {
                *result = Some(registration_result);
            }
        })
        .map_err(|error| error.to_string())?;

    let registration_result = {
        let mut result = result
            .lock()
            .map_err(|_| "Failed to lock WebView drop registration result".to_string())?;
        result
            .take()
            .ok_or_else(|| "WebView drop registration did not run on the main thread".to_string())?
    };
    registration_result
}

fn register_drop_target_on_current_thread(window: &WebviewWindow) -> Result<(), String> {
    let target_window = window.clone();
    let registered = configure_and_register_webview_targets(window, move |container_hwnd| {
        register_webview_child_targets(container_hwnd, move |child_hwnd| {
            CustomDropTarget::new(target_window.clone(), child_hwnd).into()
        })
    })?;
    info!(
        "Registered {registered} native WebView Drop target(s) for {}",
        window.label()
    );
    Ok(())
}

pub fn register_drop_target_now(window: &WebviewWindow) -> Result<(), String> {
    register_drop_target_on_current_thread(window)
}

pub fn register_drop_target(window: &WebviewWindow) -> Result<(), String> {
    let registration_window = window.clone();
    window
        .run_on_main_thread(move || {
            if let Err(error) = register_drop_target_on_current_thread(&registration_window) {
                warn!(
                    "Failed to register native Drop target for {}: {error}",
                    registration_window.label()
                );
            }
        })
        .map_err(|error| error.to_string())
}

fn register_settings_drop_target_on_current_thread(window: &WebviewWindow) -> Result<(), String> {
    let target_window = window.clone();
    let registered = configure_and_register_webview_targets(window, move |container_hwnd| {
        register_webview_child_targets(container_hwnd, move |child_hwnd| {
            SettingsDropTarget {
                window: target_window.clone(),
                visual_helper: DropTargetVisualHelper::new(child_hwnd, "Settings"),
            }
            .into()
        })
    })?;
    info!("Registered {registered} native WebView Settings drop target(s)");
    Ok(())
}

pub fn register_settings_drop_target(window: &WebviewWindow) -> Result<(), String> {
    let registration_window = window.clone();
    window
        .run_on_main_thread(move || {
            if let Err(error) =
                register_settings_drop_target_on_current_thread(&registration_window)
            {
                warn!("Failed to register native Settings drop target: {error}");
            }
        })
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{native_drop_effect, shell_drag_point};
    use windows::Win32::Foundation::POINTL;
    use windows::Win32::System::Ole::DROPEFFECT_COPY;

    #[test]
    fn shell_helper_keeps_screen_coordinates_unchanged() {
        let point = shell_drag_point(&POINTL { x: -320, y: 1440 });
        assert_eq!((point.x, point.y), (-320, 1440));
    }

    #[test]
    fn shell_helper_uses_the_same_copy_effect_as_the_drop_target() {
        assert_eq!(native_drop_effect().0, DROPEFFECT_COPY.0);
    }
}

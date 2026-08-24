use crate::drop_registry::{drop_id_from_label, drop_label, popup_label, DropRegistry};
use crate::file::{canonical_path, paths_refer_to_same_file, FileMetadata};
use crate::thumbnail::get_thumbnail_base64;
use base64::{engine::general_purpose, Engine as _};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use uuid::Uuid;

fn caller_drop_id(window: &WebviewWindow) -> Result<String, String> {
    drop_id_from_label(window.label()).map(str::to_owned)
}

pub fn notify_drop(app: &AppHandle, drop_id: &str) -> Result<(), String> {
    app.emit_to(drop_label(drop_id), "files_updated", ())
        .map_err(|error| error.to_string())?;
    if app.get_webview_window(&popup_label(drop_id)).is_some() {
        app.emit_to(popup_label(drop_id), "files_updated", ())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(crate) fn schedule_close_empty_drop(app: AppHandle, drop_id: String) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(16)).await;
        if let Some(popup) = app.get_webview_window(&popup_label(&drop_id)) {
            let _ = popup.close();
        }
        if let Some(drop_window) = app.get_webview_window(&drop_label(&drop_id)) {
            let _ = drop_window.close();
        }
    });
}

fn metadata_for_path(path: &Path, id: u64) -> Result<FileMetadata, String> {
    let path = canonical_path(path);
    let metadata = path.metadata().map_err(|error| error.to_string())?;
    let is_directory = metadata.is_dir();
    let size = if is_directory { 0 } else { metadata.len() };
    let file_type = if is_directory {
        "folder".to_string()
    } else {
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("unknown")
            .to_string()
    };
    Ok(FileMetadata {
        id,
        name: path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Unknown")
            .to_string(),
        path,
        size,
        file_type,
    })
}

#[tauri::command]
pub fn add_files(
    app_handle: AppHandle,
    window: WebviewWindow,
    registry: State<'_, DropRegistry>,
    files: Vec<String>,
) -> Result<(), String> {
    let drop_id = caller_drop_id(&window)?;
    registry.with_drop_mut(&drop_id, |drop_state| {
        if files.iter().any(|path| Path::new(path).exists()) {
            drop_state.received_content = true;
        }
        for path_str in files {
            let path = canonical_path(&PathBuf::from(path_str));
            if !path.exists()
                || drop_state
                    .files
                    .iter()
                    .any(|file| paths_refer_to_same_file(&file.path, &path))
            {
                continue;
            }
            let file = metadata_for_path(&path, drop_state.next_file_id)?;
            drop_state.next_file_id += 1;
            drop_state.files.push(file);
        }
        Ok(())
    })?;
    notify_drop(&app_handle, &drop_id)
}

#[tauri::command]
pub fn mark_drop_received(
    window: WebviewWindow,
    registry: State<'_, DropRegistry>,
) -> Result<(), String> {
    let drop_id = caller_drop_id(&window)?;
    registry.mark_content_received(&drop_id)
}

fn unique_temp_path(prefix: &str, extension: &str) -> Result<PathBuf, String> {
    let timestamp = chrono::Local::now();
    let drop_folder = std::env::temp_dir()
        .join("dropwin_drops")
        .join(timestamp.format("%Y%m%d").to_string());
    std::fs::create_dir_all(&drop_folder)
        .map_err(|error| format!("Failed to create drop folder: {error}"))?;
    let safe_extension = extension.trim_start_matches('.');
    Ok(drop_folder.join(format!(
        "{prefix}_{}_{}.{}",
        timestamp.format("%H%M%S%3f"),
        Uuid::new_v4().simple(),
        safe_extension
    )))
}

#[tauri::command]
pub fn save_pasted_text(text: String, extension: String) -> Result<String, String> {
    use std::io::Write;
    let path = unique_temp_path("pasted", &extension)?;
    let mut file = std::fs::File::create(&path).map_err(|error| error.to_string())?;
    file.write_all(text.as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn save_pasted_data_base64(data_base64: String, extension: String) -> Result<String, String> {
    use std::io::Write;
    let bytes = general_purpose::STANDARD
        .decode(data_base64)
        .map_err(|error| error.to_string())?;
    let path = unique_temp_path("pasted", &extension)?;
    let mut file = std::fs::File::create(&path).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub async fn download_image_to_shelf(url: String) -> Result<String, String> {
    use std::io::Write;
    let extension = Path::new(&url)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("png");
    let path = unique_temp_path("downloaded", extension)?;
    let response = reqwest::get(&url)
        .await
        .map_err(|error| error.to_string())?;
    let bytes = response.bytes().await.map_err(|error| error.to_string())?;
    let mut file = std::fs::File::create(&path).map_err(|error| error.to_string())?;
    file.write_all(&bytes).map_err(|error| error.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn remove_files(
    app_handle: AppHandle,
    window: WebviewWindow,
    registry: State<'_, DropRegistry>,
    file_ids: Vec<u64>,
) -> Result<(), String> {
    let drop_id = caller_drop_id(&window)?;
    let is_empty = registry.with_drop_mut(&drop_id, |drop_state| {
        for file_id in file_ids {
            if let Some(position) = drop_state.files.iter().position(|file| file.id == file_id) {
                drop_state.files.remove(position);
            }
        }
        Ok(drop_state.files.is_empty())
    })?;
    if is_empty {
        schedule_close_empty_drop(app_handle, drop_id);
        Ok(())
    } else {
        notify_drop(&app_handle, &drop_id)
    }
}

#[tauri::command]
pub fn get_files(
    window: WebviewWindow,
    registry: State<'_, DropRegistry>,
) -> Result<Vec<FileMetadata>, String> {
    let drop_id = caller_drop_id(&window)?;
    registry.with_drop(&drop_id, |drop_state| Ok(drop_state.files.clone()))
}

#[tauri::command]
pub fn rename_file(
    app_handle: AppHandle,
    window: WebviewWindow,
    registry: State<'_, DropRegistry>,
    file_id: u64,
    new_name: String,
) -> Result<(), String> {
    let drop_id = caller_drop_id(&window)?;
    registry.with_drop_mut(&drop_id, |drop_state| {
        let file = drop_state
            .files
            .iter_mut()
            .find(|file| file.id == file_id)
            .ok_or_else(|| format!("File with ID {file_id} not found"))?;
        file.name = new_name;
        Ok(())
    })?;
    notify_drop(&app_handle, &drop_id)
}

#[tauri::command]
pub fn clear_files(
    app_handle: AppHandle,
    window: WebviewWindow,
    registry: State<'_, DropRegistry>,
) -> Result<(), String> {
    let drop_id = caller_drop_id(&window)?;
    registry.with_drop_mut(&drop_id, |drop_state| {
        drop_state.files.clear();
        Ok(())
    })?;
    schedule_close_empty_drop(app_handle, drop_id);
    Ok(())
}

#[tauri::command]
pub fn refresh_file_list(
    app_handle: AppHandle,
    window: WebviewWindow,
    registry: State<'_, DropRegistry>,
) -> Result<(), String> {
    let drop_id = caller_drop_id(&window)?;
    let (changed, is_empty) = registry.with_drop_mut(&drop_id, |drop_state| {
        let old_len = drop_state.files.len();
        drop_state.files.retain(|file| file.path.exists());
        Ok((
            old_len != drop_state.files.len(),
            drop_state.files.is_empty(),
        ))
    })?;
    if changed {
        if is_empty {
            schedule_close_empty_drop(app_handle, drop_id);
        } else {
            notify_drop(&app_handle, &drop_id)?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_file_icon_base64(file_path: &str) -> Result<String, String> {
    get_thumbnail_base64(file_path).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_registry_entry_does_not_delete_a_file() {
        let registry = DropRegistry::default();
        let file_path = std::env::temp_dir().join(format!("dropwin-test-{}", Uuid::new_v4()));
        std::fs::write(&file_path, b"test").unwrap();
        registry.create("test".into(), (0.0, 0.0)).unwrap();
        registry
            .with_drop_mut("test", |drop_state| {
                drop_state.files.push(metadata_for_path(&file_path, 0)?);
                Ok(())
            })
            .unwrap();
        registry.remove("test").unwrap();
        assert!(file_path.exists());
        std::fs::remove_file(file_path).unwrap();
    }

    #[test]
    fn folder_metadata_is_created_without_scanning_contents() {
        let folder_path = std::env::temp_dir().join(format!("dropwin-folder-{}", Uuid::new_v4()));
        std::fs::create_dir(&folder_path).unwrap();
        std::fs::write(folder_path.join("large.bin"), vec![0_u8; 1024]).unwrap();

        let metadata = metadata_for_path(&folder_path, 0).unwrap();
        assert_eq!(metadata.file_type, "folder");
        assert_eq!(metadata.size, 0);

        std::fs::remove_dir_all(folder_path).unwrap();
    }

    #[test]
    fn empty_text_file_metadata_preserves_the_original_file() {
        let file_name = format!("dropwin-empty-{}.txt", Uuid::new_v4());
        let file_path = std::env::temp_dir().join(&file_name);
        std::fs::File::create(&file_path).unwrap();

        let metadata = metadata_for_path(&file_path, 7).unwrap();
        assert_eq!(metadata.id, 7);
        assert_eq!(metadata.name, file_name);
        assert_eq!(metadata.path, canonical_path(&file_path));
        assert_eq!(metadata.file_type, "txt");
        assert_eq!(metadata.size, 0);

        std::fs::remove_file(file_path).unwrap();
    }
}

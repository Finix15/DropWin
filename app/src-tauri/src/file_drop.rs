use crate::commands::file_ops::notify_drop;
use crate::drop_registry::DropRegistry;
use crate::file::{canonical_path, paths_refer_to_same_file, FileMetadata};
use std::path::PathBuf;
use tauri::AppHandle;
use tracing::info;

pub fn handle_file_drop_from_paths(
    paths: Vec<PathBuf>,
    drop_id: String,
    registry: DropRegistry,
    app_handle: AppHandle,
) {
    if paths.iter().any(|path| path.exists()) {
        let _ = registry.mark_content_received(&drop_id);
    }

    tauri::async_runtime::spawn(async move {
        let mut new_files = Vec::new();
        for path in paths {
            let path = canonical_path(&path);
            if !path.exists() {
                continue;
            }
            let Ok(metadata) = path.metadata() else {
                continue;
            };
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
            new_files.push(FileMetadata {
                id: 0,
                name: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Unknown")
                    .to_string(),
                path,
                size,
                file_type,
            });
        }

        let result = registry.with_drop_mut(&drop_id, |drop_state| {
            for mut file in new_files {
                if drop_state
                    .files
                    .iter()
                    .any(|existing| paths_refer_to_same_file(&existing.path, &file.path))
                {
                    continue;
                }
                file.id = drop_state.next_file_id;
                drop_state.next_file_id += 1;
                info!("Added dropped file to Drop {drop_id}: {:?}", file.path);
                drop_state.files.push(file);
            }
            Ok(())
        });

        if result.is_ok() {
            let _ = notify_drop(&app_handle, &drop_id);
        }
    });
}

use crate::file::paths_refer_to_same_file;
use crate::file::FileMetadata;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

pub const DROP_LABEL_PREFIX: &str = "drop-";
pub const POPUP_LABEL_PREFIX: &str = "popup-";
const DEFAULT_DROP_SIZE: (f64, f64) = (156.0, 156.0);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DropBounds {
    pub position: (f64, f64),
    pub size: (f64, f64),
}

#[derive(Debug, Default)]
pub struct DropState {
    pub files: Vec<FileMetadata>,
    pub next_file_id: u64,
    pub position: (f64, f64),
    pub size: (f64, f64),
    pub received_content: bool,
    pub prepared: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TransferOutcome {
    pub transferred_count: usize,
    pub source_empty: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DropRegistry {
    drops: Arc<Mutex<HashMap<String, DropState>>>,
}

impl DropRegistry {
    #[cfg(test)]
    pub fn create(&self, drop_id: String, position: (f64, f64)) -> Result<(), String> {
        self.create_with_size(drop_id, position, DEFAULT_DROP_SIZE)
    }

    pub fn create_with_size(
        &self,
        drop_id: String,
        position: (f64, f64),
        size: (f64, f64),
    ) -> Result<(), String> {
        self.create_with_state(drop_id, position, size, false)
    }

    pub fn create_prepared(&self, drop_id: String) -> Result<(), String> {
        self.create_with_state(drop_id, (0.0, 0.0), DEFAULT_DROP_SIZE, true)
    }

    fn create_with_state(
        &self,
        drop_id: String,
        position: (f64, f64),
        size: (f64, f64),
        prepared: bool,
    ) -> Result<(), String> {
        let mut drops = self
            .drops
            .lock()
            .map_err(|_| "Failed to lock drop registry".to_string())?;
        if drops.contains_key(&drop_id) {
            return Err(format!("Drop {drop_id} already exists"));
        }
        drops.insert(
            drop_id,
            DropState {
                position,
                size,
                prepared,
                ..DropState::default()
            },
        );
        Ok(())
    }

    pub fn activate_prepared(
        &self,
        drop_id: &str,
        position: (f64, f64),
        size: (f64, f64),
    ) -> Result<(), String> {
        self.with_drop_mut(drop_id, |drop_state| {
            if !drop_state.prepared {
                return Err(format!("Drop {drop_id} is not prepared"));
            }
            drop_state.position = position;
            drop_state.size = size;
            drop_state.prepared = false;
            Ok(())
        })
    }

    pub fn remove(&self, drop_id: &str) -> Result<Option<DropState>, String> {
        self.drops
            .lock()
            .map_err(|_| "Failed to lock drop registry".to_string())
            .map(|mut drops| drops.remove(drop_id))
    }

    pub fn with_drop<T>(
        &self,
        drop_id: &str,
        operation: impl FnOnce(&DropState) -> Result<T, String>,
    ) -> Result<T, String> {
        let drops = self
            .drops
            .lock()
            .map_err(|_| "Failed to lock drop registry".to_string())?;
        let drop_state = drops
            .get(drop_id)
            .ok_or_else(|| format!("Drop {drop_id} is no longer open"))?;
        operation(drop_state)
    }

    pub fn with_drop_mut<T>(
        &self,
        drop_id: &str,
        operation: impl FnOnce(&mut DropState) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut drops = self
            .drops
            .lock()
            .map_err(|_| "Failed to lock drop registry".to_string())?;
        let drop_state = drops
            .get_mut(drop_id)
            .ok_or_else(|| format!("Drop {drop_id} is no longer open"))?;
        operation(drop_state)
    }

    pub fn bounds(&self) -> Result<Vec<DropBounds>, String> {
        self.drops
            .lock()
            .map_err(|_| "Failed to lock drop registry".to_string())
            .map(|drops| {
                drops
                    .values()
                    .filter(|drop_state| !drop_state.prepared)
                    .map(|drop_state| DropBounds {
                        position: drop_state.position,
                        size: drop_state.size,
                    })
                    .collect()
            })
    }

    pub fn ids(&self) -> Result<Vec<String>, String> {
        self.drops
            .lock()
            .map_err(|_| "Failed to lock drop registry".to_string())
            .map(|drops| drops.keys().cloned().collect())
    }

    pub fn contains(&self, drop_id: &str) -> Result<bool, String> {
        self.drops
            .lock()
            .map_err(|_| "Failed to lock drop registry".to_string())
            .map(|drops| drops.contains_key(drop_id))
    }

    pub fn mark_content_received(&self, drop_id: &str) -> Result<(), String> {
        self.with_drop_mut(drop_id, |drop_state| {
            drop_state.received_content = true;
            Ok(())
        })
    }

    pub fn transfer_files(
        &self,
        source_drop_id: &str,
        target_drop_id: &str,
        source_file_ids: &[u64],
    ) -> Result<TransferOutcome, String> {
        if source_drop_id == target_drop_id {
            return Ok(TransferOutcome::default());
        }

        let selected_ids: HashSet<u64> = source_file_ids.iter().copied().collect();
        if selected_ids.is_empty() {
            return Ok(TransferOutcome::default());
        }

        let mut drops = self
            .drops
            .lock()
            .map_err(|_| "Failed to lock drop registry".to_string())?;
        if !drops.contains_key(source_drop_id) {
            return Err(format!("Drop {source_drop_id} is no longer open"));
        }
        if !drops.contains_key(target_drop_id) {
            return Err(format!("Drop {target_drop_id} is no longer open"));
        }

        let (moved_files, source_empty) = {
            let source = drops
                .get_mut(source_drop_id)
                .expect("source Drop existence was checked above");
            let mut moved_files = Vec::new();
            source.files.retain(|file| {
                if selected_ids.contains(&file.id) {
                    moved_files.push(file.clone());
                    false
                } else {
                    true
                }
            });
            (moved_files, source.files.is_empty())
        };

        if moved_files.is_empty() {
            return Ok(TransferOutcome {
                transferred_count: 0,
                source_empty,
            });
        }

        let transferred_count = moved_files.len();
        let target = drops
            .get_mut(target_drop_id)
            .expect("target Drop existence was checked above");
        target.received_content = true;
        for mut file in moved_files {
            if target
                .files
                .iter()
                .any(|existing| paths_refer_to_same_file(&existing.path, &file.path))
            {
                continue;
            }
            file.id = target.next_file_id;
            target.next_file_id += 1;
            target.files.push(file);
        }

        Ok(TransferOutcome {
            transferred_count,
            source_empty,
        })
    }

    pub fn remove_if_empty_and_unreceived(
        &self,
        drop_id: &str,
    ) -> Result<Option<DropState>, String> {
        let mut drops = self
            .drops
            .lock()
            .map_err(|_| "Failed to lock drop registry".to_string())?;
        let should_remove = drops
            .get(drop_id)
            .map(|drop_state| !drop_state.received_content && drop_state.files.is_empty())
            .unwrap_or(false);
        Ok(if should_remove {
            drops.remove(drop_id)
        } else {
            None
        })
    }
}

pub fn drop_id_from_label(label: &str) -> Result<&str, String> {
    let id = label
        .strip_prefix(DROP_LABEL_PREFIX)
        .or_else(|| label.strip_prefix(POPUP_LABEL_PREFIX))
        .ok_or_else(|| format!("Window {label} is not associated with a Drop"))?;
    if id.is_empty() {
        return Err(format!("Window {label} has an invalid Drop identifier"));
    }
    Ok(id)
}

pub fn drop_label(drop_id: &str) -> String {
    format!("{DROP_LABEL_PREFIX}{drop_id}")
}

pub fn popup_label(drop_id: &str) -> String {
    format!("{POPUP_LABEL_PREFIX}{drop_id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn resolves_drop_and_popup_labels() {
        assert_eq!(drop_id_from_label("drop-abc"), Ok("abc"));
        assert_eq!(drop_id_from_label("popup-abc"), Ok("abc"));
        assert!(drop_id_from_label("settings").is_err());
    }

    #[test]
    fn registry_keeps_drops_independent() {
        let registry = DropRegistry::default();
        registry.create("a".into(), (0.0, 0.0)).unwrap();
        registry.create("b".into(), (24.0, 24.0)).unwrap();
        registry
            .with_drop_mut("a", |drop_state| {
                drop_state.next_file_id = 5;
                Ok(())
            })
            .unwrap();
        assert_eq!(
            registry
                .with_drop("b", |drop_state| Ok(drop_state.next_file_id))
                .unwrap(),
            0
        );
        assert!(registry.remove("a").unwrap().is_some());
        assert!(registry.with_drop("a", |_| Ok(())).is_err());
        assert!(registry.with_drop("b", |_| Ok(())).is_ok());
    }

    #[test]
    fn the_same_path_can_belong_to_two_drops() {
        let registry = DropRegistry::default();
        registry.create("a".into(), (0.0, 0.0)).unwrap();
        registry.create("b".into(), (24.0, 24.0)).unwrap();
        let file = FileMetadata {
            id: 0,
            name: "example.txt".into(),
            path: PathBuf::from("example.txt"),
            size: 1,
            file_type: "txt".into(),
        };
        registry
            .with_drop_mut("a", |drop_state| {
                drop_state.files.push(file.clone());
                Ok(())
            })
            .unwrap();
        registry
            .with_drop_mut("b", |drop_state| {
                drop_state.files.push(file);
                Ok(())
            })
            .unwrap();
        assert_eq!(
            registry
                .with_drop("a", |state| Ok(state.files.len()))
                .unwrap(),
            1
        );
        assert_eq!(
            registry
                .with_drop("b", |state| Ok(state.files.len()))
                .unwrap(),
            1
        );
    }

    #[test]
    fn received_content_is_independent_per_drop() {
        let registry = DropRegistry::default();
        registry.create("a".into(), (0.0, 0.0)).unwrap();
        registry.create("b".into(), (24.0, 24.0)).unwrap();

        registry.mark_content_received("a").unwrap();

        assert!(registry
            .with_drop("a", |state| Ok(state.received_content))
            .unwrap());
        assert!(!registry
            .with_drop("b", |state| Ok(state.received_content))
            .unwrap());
    }

    #[test]
    fn only_empty_unreceived_drop_is_removed_after_release() {
        let registry = DropRegistry::default();
        registry.create("received".into(), (0.0, 0.0)).unwrap();
        registry.create("empty".into(), (24.0, 24.0)).unwrap();
        registry.create("populated".into(), (48.0, 48.0)).unwrap();
        registry.mark_content_received("received").unwrap();
        registry
            .with_drop_mut("populated", |state| {
                state.files.push(FileMetadata {
                    id: 0,
                    name: "example.txt".into(),
                    path: PathBuf::from("example.txt"),
                    size: 1,
                    file_type: "txt".into(),
                });
                Ok(())
            })
            .unwrap();

        assert!(registry
            .remove_if_empty_and_unreceived("received")
            .unwrap()
            .is_none());
        assert!(registry
            .remove_if_empty_and_unreceived("empty")
            .unwrap()
            .is_some());
        assert!(registry
            .remove_if_empty_and_unreceived("populated")
            .unwrap()
            .is_none());
        assert!(registry.contains("received").unwrap());
        assert!(registry.contains("populated").unwrap());
        assert!(!registry.contains("empty").unwrap());
    }

    #[test]
    fn marking_a_closed_drop_returns_an_error() {
        let registry = DropRegistry::default();
        registry.create("closed".into(), (0.0, 0.0)).unwrap();
        registry.remove("closed").unwrap();

        assert!(registry.mark_content_received("closed").is_err());
    }

    #[test]
    fn prepared_drop_is_hidden_from_positions_until_activated() {
        let registry = DropRegistry::default();
        registry.create("visible".into(), (24.0, 48.0)).unwrap();
        registry.create_prepared("next".into()).unwrap();

        assert_eq!(
            registry.bounds().unwrap(),
            vec![DropBounds {
                position: (24.0, 48.0),
                size: DEFAULT_DROP_SIZE,
            }]
        );
        registry
            .activate_prepared("next", (72.0, 96.0), (234.0, 234.0))
            .unwrap();
        let bounds = registry.bounds().unwrap();
        assert_eq!(bounds.len(), 2);
        assert!(bounds.contains(&DropBounds {
            position: (24.0, 48.0),
            size: DEFAULT_DROP_SIZE,
        }));
        assert!(bounds.contains(&DropBounds {
            position: (72.0, 96.0),
            size: (234.0, 234.0),
        }));
    }

    fn test_file(id: u64, name: &str) -> FileMetadata {
        FileMetadata {
            id,
            name: name.into(),
            path: PathBuf::from(name),
            size: 1,
            file_type: "txt".into(),
        }
    }

    #[test]
    fn transfers_selected_files_and_reassigns_target_ids() {
        let registry = DropRegistry::default();
        registry.create("source".into(), (0.0, 0.0)).unwrap();
        registry.create("target".into(), (24.0, 24.0)).unwrap();
        registry
            .with_drop_mut("source", |state| {
                state.files = vec![test_file(4, "one.txt"), test_file(9, "two.txt")];
                Ok(())
            })
            .unwrap();

        let outcome = registry.transfer_files("source", "target", &[9]).unwrap();

        assert_eq!(outcome.transferred_count, 1);
        assert!(!outcome.source_empty);
        assert_eq!(
            registry
                .with_drop("source", |state| Ok(state.files[0].name.clone()))
                .unwrap(),
            "one.txt"
        );
        registry
            .with_drop("target", |state| {
                assert_eq!(state.files.len(), 1);
                assert_eq!(state.files[0].id, 0);
                assert_eq!(state.files[0].name, "two.txt");
                assert!(state.received_content);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    fn moving_all_files_reports_an_empty_source() {
        let registry = DropRegistry::default();
        registry.create("source".into(), (0.0, 0.0)).unwrap();
        registry.create("target".into(), (24.0, 24.0)).unwrap();
        registry
            .with_drop_mut("source", |state| {
                state.files.push(test_file(3, "only.txt"));
                Ok(())
            })
            .unwrap();

        let outcome = registry.transfer_files("source", "target", &[3]).unwrap();

        assert_eq!(outcome.transferred_count, 1);
        assert!(outcome.source_empty);
    }

    #[test]
    fn duplicate_target_path_is_not_added_twice_but_leaves_the_source() {
        let registry = DropRegistry::default();
        registry.create("source".into(), (0.0, 0.0)).unwrap();
        registry.create("target".into(), (24.0, 24.0)).unwrap();
        registry
            .with_drop_mut("source", |state| {
                state.files.push(test_file(1, "same.txt"));
                Ok(())
            })
            .unwrap();
        registry
            .with_drop_mut("target", |state| {
                state.files.push(test_file(8, "same.txt"));
                state.next_file_id = 9;
                Ok(())
            })
            .unwrap();

        let outcome = registry.transfer_files("source", "target", &[1]).unwrap();

        assert_eq!(outcome.transferred_count, 1);
        assert!(outcome.source_empty);
        assert_eq!(
            registry
                .with_drop("target", |state| Ok(state.files.len()))
                .unwrap(),
            1
        );
    }

    #[test]
    fn missing_ids_and_same_drop_are_no_ops() {
        let registry = DropRegistry::default();
        registry.create("source".into(), (0.0, 0.0)).unwrap();
        registry.create("target".into(), (24.0, 24.0)).unwrap();
        registry
            .with_drop_mut("source", |state| {
                state.files.push(test_file(1, "kept.txt"));
                Ok(())
            })
            .unwrap();

        assert_eq!(
            registry.transfer_files("source", "target", &[99]).unwrap(),
            TransferOutcome::default()
        );
        assert_eq!(
            registry.transfer_files("source", "source", &[1]).unwrap(),
            TransferOutcome::default()
        );
        assert_eq!(
            registry
                .with_drop("source", |state| Ok(state.files.len()))
                .unwrap(),
            1
        );
    }
}

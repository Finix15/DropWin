use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub fn canonical_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    let left = canonical_path(left);
    let right = canonical_path(right);

    #[cfg(target_os = "windows")]
    {
        left.to_string_lossy()
            .eq_ignore_ascii_case(&right.to_string_lossy())
    }
    #[cfg(not(target_os = "windows"))]
    {
        left == right
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub id: u64,
    pub name: String,
    pub path: PathBuf,
    pub size: u64,
    #[serde(rename = "type")]
    pub file_type: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_file_type_for_the_frontend() {
        let metadata = FileMetadata {
            id: 1,
            name: "folder".into(),
            path: PathBuf::from("folder"),
            size: 0,
            file_type: "folder".into(),
        };
        let value = serde_json::to_value(metadata).unwrap();
        assert_eq!(value["type"], "folder");
        assert!(value.get("file_type").is_none());
    }
}

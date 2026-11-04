use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryEntry {
    name: String,
    path: String,
    is_directory: bool,
}

#[tauri::command]
pub async fn list_local_directory(
    root_path: String,
    path: String,
) -> Result<Vec<DirectoryEntry>, String> {
    tauri::async_runtime::spawn_blocking(move || read_directory(&root_path, &path))
        .await
        .map_err(|error| format!("Could not list folder contents: {error}"))?
}

fn read_directory(root_path: &str, path: &str) -> Result<Vec<DirectoryEntry>, String> {
    let root = canonical_directory(root_path, "Could not access the selected folder")?;
    let directory = canonical_directory(path, "Could not access this folder")?;
    if !directory.starts_with(&root) {
        return Err("You can only browse within the folder you selected.".to_string());
    }

    let mut entries = fs::read_dir(&directory)
        .map_err(|error| format!("Could not read folder contents: {error}"))?
        .map(|entry| {
            let entry = entry.map_err(|error| format!("Could not read a folder entry: {error}"))?;
            let file_type = entry
                .file_type()
                .map_err(|error| format!("Could not inspect a folder entry: {error}"))?;
            if !file_type.is_dir() && !file_type.is_file() && !file_type.is_symlink() {
                return Ok(None);
            }

            let entry_path = entry.path();
            let path = entry_path
                .to_str()
                .ok_or_else(|| "A selected path cannot be represented as text.".to_string())?
                .to_string();
            Ok(Some(DirectoryEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                path,
                is_directory: file_type.is_dir(),
            }))
        })
        .collect::<Result<Vec<_>, String>>()?
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();

    entries.sort_by(|left, right| {
        right
            .is_directory
            .cmp(&left.is_directory)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
            .then_with(|| left.name.cmp(&right.name))
    });
    Ok(entries)
}

fn canonical_directory(path: &str, message: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if !path.is_absolute() {
        return Err(format!("{message}: choose an absolute path."));
    }
    let canonical = path
        .canonicalize()
        .map_err(|error| format!("{message}: {error}"))?;
    if !canonical.is_dir() {
        return Err(format!("{message}: the selected path is not a folder."));
    }
    Ok(canonical)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn lists_folders_before_files() {
        let temporary = tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir(root.join("Zebra")).unwrap();
        fs::write(root.join("alpha.txt"), "file").unwrap();
        fs::write(root.join("beta.txt"), "file").unwrap();

        let entries = read_directory(root.to_str().unwrap(), root.to_str().unwrap()).unwrap();

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            ["Zebra", "alpha.txt", "beta.txt"]
        );
        assert!(entries[0].is_directory);
        assert!(!entries[1].is_directory);
    }

    #[test]
    fn refuses_to_browse_outside_the_selected_folder() {
        let temporary = tempdir().unwrap();
        let root = temporary.path().join("root");
        let outside = temporary.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();

        let error = read_directory(root.to_str().unwrap(), outside.to_str().unwrap()).unwrap_err();

        assert_eq!(error, "You can only browse within the folder you selected.");
    }
}

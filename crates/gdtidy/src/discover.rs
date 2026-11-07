//! Finding the GDScript files to process.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use walkdir::{DirEntry, WalkDir};

/// The `.gd` files under `targets`. A file named directly is always included.
/// When walking a folder, hidden folders, `addons`, folders containing a
/// `.gdignore`, and nested Godot projects are skipped, as Godot's editor does.
pub fn gdscript_files(targets: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for target in targets {
        if target.is_file() {
            files.push(target.clone());
        } else if target.is_dir() {
            let walker = WalkDir::new(target)
                .into_iter()
                .filter_entry(|entry| entry.depth() == 0 || !is_skipped_dir(entry));
            for entry in walker {
                let entry = entry?;
                if entry.file_type().is_file() && is_gdscript(entry.path()) {
                    files.push(entry.into_path());
                }
            }
        } else {
            bail!("{}: no such file or folder", target.display());
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn is_gdscript(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext == "gd")
}

fn is_skipped_dir(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    let name = entry.file_name().to_string_lossy();
    name.starts_with('.')
        || name == "addons"
        || entry.path().join(".gdignore").exists()
        || entry.path().join("project.godot").exists()
}

/// A path as shown in reports: forward slashes, without a leading `./`.
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    text.strip_prefix("./").map(str::to_string).unwrap_or(text)
}

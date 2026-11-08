//! Finding the GDScript files to process.

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use walkdir::{DirEntry, WalkDir};

/// A script to process, and whether it was named directly (so configured
/// exclusions do not apply to it).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Target {
    pub path: PathBuf,
    pub explicit: bool,
}

/// The `.gd` files under `targets`. A `.gd` file named directly is always
/// included, and naming any other file is an error.
/// When walking a folder, hidden folders, `addons`, folders containing a
/// `.gdignore`, and nested Godot projects are skipped, as Godot's editor does.
pub fn gdscript_files(targets: &[PathBuf]) -> Result<Vec<Target>> {
    let mut files = Vec::new();
    for target in targets {
        if target.is_file() {
            if !is_gdscript(target) {
                bail!("{}: not a GDScript (.gd) file", target.display());
            }
            files.push(Target {
                path: target.clone(),
                explicit: true,
            });
        } else if target.is_dir() {
            let walker = WalkDir::new(target)
                .into_iter()
                .filter_entry(|entry| entry.depth() == 0 || !is_skipped_dir(entry));
            for entry in walker {
                let entry = entry?;
                if entry.file_type().is_file() && is_gdscript(entry.path()) {
                    files.push(Target {
                        path: entry.into_path(),
                        explicit: false,
                    });
                }
            }
        } else {
            bail!("{}: no such file or folder", target.display());
        }
    }
    // Sorted by path, explicit first, so the explicit copy of a duplicate
    // is the one kept.
    files.sort_by(|a, b| a.path.cmp(&b.path).then(b.explicit.cmp(&a.explicit)));
    files.dedup_by(|a, b| a.path == b.path);
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

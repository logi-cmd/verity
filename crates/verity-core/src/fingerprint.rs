// SPDX-License-Identifier: MPL-2.0

use ignore::WalkBuilder;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Copy)]
pub struct SnapshotLimits {
    pub max_files: usize,
    pub max_bytes: u64,
}

impl Default for SnapshotLimits {
    fn default() -> Self {
        Self {
            max_files: 200_000,
            max_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Error)]
pub enum FingerprintError {
    #[error("repository is not a directory: {0}")]
    NotDirectory(String),
    #[error("repository contains a symbolic link and cannot be safely snapshotted: {0}")]
    SymbolicLink(String),
    #[error("repository exceeds the supported file limit of {0}")]
    TooManyFiles(usize),
    #[error("repository exceeds the supported snapshot size of {0} bytes")]
    TooLarge(u64),
    #[error("unable to read repository: {0}")]
    Io(#[from] io::Error),
}

fn ignored_name(name: &str) -> bool {
    matches!(
        name,
        ".git"
            | ".hg"
            | ".svn"
            | ".worktrees"
            | ".wrangler"
            | ".agent-guardrails"
            | ".agents"
            | ".gstack"
            | ".omx"
            | "node_modules"
            | "target"
            | "dist"
            | "build"
            | "coverage"
            | ".next"
            | ".cache"
            | "__pycache__"
            | ".venv"
            | "venv"
            | "tmp"
    )
}

fn ignored_file(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.") || name.ends_with(".pem") || name.ends_with(".key")
}

pub fn repository_files(
    root: &Path,
    limits: SnapshotLimits,
) -> Result<Vec<PathBuf>, FingerprintError> {
    if !root.is_dir() {
        return Err(FingerprintError::NotDirectory(root.display().to_string()));
    }
    let mut files = Vec::new();
    let mut bytes = 0_u64;
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(true)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|entry| {
            entry.depth() == 0
                || entry
                    .file_name()
                    .to_str()
                    .is_none_or(|name| !ignored_name(name))
        });
    let walker = builder.build();
    for entry in walker {
        let entry = entry.map_err(|error| io::Error::other(error.to_string()))?;
        if entry.depth() == 0 {
            continue;
        }
        let path = entry.into_path();
        let metadata = fs::symlink_metadata(&path)?;
        let relative = path.strip_prefix(root).unwrap_or(&path);
        if metadata.file_type().is_symlink() {
            return Err(FingerprintError::SymbolicLink(
                relative.display().to_string(),
            ));
        }
        if !metadata.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        if ignored_file(name) {
            continue;
        }
        bytes = bytes.saturating_add(metadata.len());
        if bytes > limits.max_bytes {
            return Err(FingerprintError::TooLarge(limits.max_bytes));
        }
        files.push(path);
        if files.len() > limits.max_files {
            return Err(FingerprintError::TooManyFiles(limits.max_files));
        }
    }
    files.sort_by_key(|path| path.strip_prefix(root).unwrap_or(path).to_path_buf());
    Ok(files)
}

pub fn fingerprint_repository(
    root: &Path,
    limits: SnapshotLimits,
) -> Result<String, FingerprintError> {
    let files = repository_files(root, limits)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    for path in files {
        let relative = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .to_string_lossy()
            .replace('\\', "/");
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        let mut file = fs::File::open(&path)?;
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hasher.update(&buffer[..read]);
        }
        hasher.update([0xff]);
    }
    Ok(hex::encode(hasher.finalize()))
}

pub fn copyable_files(
    root: &Path,
    limits: SnapshotLimits,
) -> Result<Vec<PathBuf>, FingerprintError> {
    repository_files(root, limits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_dependencies_and_secret_files_are_not_snapshotted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("source.txt"), "included").unwrap();
        fs::write(dir.path().join(".env"), "TOKEN=never-copy").unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/dep.js"), "ignored").unwrap();
        let files = repository_files(dir.path(), SnapshotLimits::default()).unwrap();
        let relative = files
            .iter()
            .map(|path| {
                path.strip_prefix(dir.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        assert_eq!(relative, vec!["source.txt"]);
    }

    #[test]
    fn snapshot_limits_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("large.bin"), [1u8; 32]).unwrap();
        assert!(matches!(
            repository_files(
                dir.path(),
                SnapshotLimits {
                    max_files: 10,
                    max_bytes: 8
                }
            ),
            Err(FingerprintError::TooLarge(8))
        ));
    }

    #[test]
    fn gitignore_controls_snapshot_scope() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "ignored.txt\n.next/\n").unwrap();
        fs::write(dir.path().join("tracked.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("ignored.txt"), "do not copy").unwrap();
        fs::create_dir(dir.path().join(".next")).unwrap();
        fs::write(dir.path().join(".next/cache"), "generated").unwrap();
        let relative = repository_files(dir.path(), SnapshotLimits::default())
            .unwrap()
            .into_iter()
            .map(|path| {
                path.strip_prefix(dir.path())
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        assert!(relative.contains(&"tracked.rs".into()));
        assert!(!relative.contains(&"ignored.txt".into()));
        assert!(!relative.iter().any(|path| path.starts_with(".next/")));
    }
}

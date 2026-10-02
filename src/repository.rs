use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ignore::{DirEntry, WalkBuilder};

use crate::language::detect_language;
use crate::model::SourceFile;

#[derive(Debug, Clone)]
pub struct ScanLimits {
    pub max_file_bytes: u64,
    pub max_source_files: usize,
    pub max_source_bytes: u64,
    pub max_scanned_files: usize,
    pub max_depth: usize,
    pub progress: bool,
}
impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 2 * 1024 * 1024,
            max_source_files: 100_000,
            max_source_bytes: 256 * 1024 * 1024,
            max_scanned_files: 1_000_000,
            max_depth: 128,
            progress: false,
        }
    }
}
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ScanStats {
    pub files_scanned: usize,
    pub oversized_files: usize,
    pub excluded_directories: usize,
    pub depth_limited_directories: usize,
    pub source_bytes: u64,
}
pub struct Discovery {
    pub paths: Vec<PathBuf>,
    pub stats: ScanStats,
}
static CANCELLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn cancel_scan() {
    CANCELLED.store(true, std::sync::atomic::Ordering::Relaxed);
}
pub fn check_cancelled() -> Result<()> {
    anyhow::ensure!(
        !CANCELLED.load(std::sync::atomic::Ordering::Relaxed) && !request_cancelled(),
        "repository scan cancelled"
    );
    Ok(())
}
pub fn discover_source_paths(root: &Path) -> Result<(Vec<PathBuf>, usize)> {
    let discovery = discover_with_limits(root, &ScanLimits::default())?;
    Ok((discovery.paths, discovery.stats.files_scanned))
}
pub fn discover_with_limits(root: &Path, limits: &ScanLimits) -> Result<Discovery> {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    anyhow::ensure!(
        limits.max_depth > 0
            && limits.max_file_bytes > 0
            && limits.max_source_files > 0
            && limits.max_source_bytes > 0,
        "scan limits must be positive"
    );
    anyhow::ensure!(
        limits.max_file_bytes <= 2 * 1024 * 1024,
        "max file bytes cannot exceed the hard 2 MiB parser limit"
    );
    let root = root
        .canonicalize()
        .with_context(|| format!("cannot access repository root {}", root.display()))?;
    let excluded = Arc::new(AtomicUsize::new(0));
    let deep = Arc::new(AtomicUsize::new(0));
    let exclusions = excluded.clone();
    let depths = deep.clone();
    let max_depth = limits.max_depth;
    // ignore's Walk iterator uses an explicit directory stack and respects
    // .gitignore even for exported fixtures without a .git directory.
    let walker = WalkBuilder::new(&root)
        .hidden(true)
        .git_ignore(true)
        .require_git(false)
        .git_global(true)
        .git_exclude(true)
        .parents(true)
        .filter_entry(move |entry| {
            if is_excluded_dir(entry) {
                exclusions.fetch_add(1, Ordering::Relaxed);
                return false;
            }
            if entry.file_type().is_some_and(|t| t.is_dir()) && entry.depth() >= max_depth {
                depths.fetch_add(1, Ordering::Relaxed);
                return false;
            }
            true
        })
        .build();
    let mut paths = Vec::new();
    let mut stats = ScanStats::default();
    for entry in walker {
        check_cancelled()?;
        let entry = entry.with_context(|| format!("failed while traversing {}", root.display()))?;
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        stats.files_scanned += 1;
        anyhow::ensure!(
            stats.files_scanned <= limits.max_scanned_files,
            "scan limit exceeded after {} files (max scanned files {}); narrow the root or explicitly raise limits",
            stats.files_scanned,
            limits.max_scanned_files
        );
        if limits.progress && stats.files_scanned.is_multiple_of(1000) {
            eprintln!(
                "scanned {} files; selected {} source files; skipped {} oversized files",
                stats.files_scanned,
                paths.len(),
                stats.oversized_files
            );
        }
        if detect_language(entry.path()).is_none() {
            continue;
        }
        let metadata = entry.metadata()?;
        if metadata.len() > limits.max_file_bytes {
            stats.oversized_files += 1;
            continue;
        }
        stats.source_bytes = stats.source_bytes.saturating_add(metadata.len());
        anyhow::ensure!(
            paths.len() < limits.max_source_files && stats.source_bytes <= limits.max_source_bytes,
            "repository limit exceeded: scanned {} files, {} source bytes (limits: {} source files, {} bytes); narrow the root or explicitly raise limits",
            stats.files_scanned,
            stats.source_bytes,
            limits.max_source_files,
            limits.max_source_bytes
        );
        paths.push(entry.into_path());
    }
    stats.excluded_directories = excluded.load(Ordering::Relaxed);
    stats.depth_limited_directories = deep.load(Ordering::Relaxed);
    paths.sort();
    Ok(Discovery { paths, stats })
}

pub fn load_source_file(root: &Path, path: &Path) -> Result<Option<SourceFile>> {
    let Some(language) = detect_language(path) else {
        return Ok(None);
    };
    check_cancelled()?;
    // Recheck at read time and cap the read too, so a file growing during a
    // scan cannot bypass discovery's size bound.
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 2 * 1024 * 1024 || bytes.contains(&0) {
        return Ok(None);
    }
    let source = match String::from_utf8(bytes) {
        Ok(source) => source,
        Err(_) => return Ok(None),
    };
    let relative_path = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    Ok(Some(SourceFile {
        absolute_path: path.to_owned(),
        relative_path,
        language: crate::language::language_for_source(path, &source, language),
        source,
    }))
}

fn is_excluded_dir(entry: &DirEntry) -> bool {
    if !entry.file_type().is_some_and(|kind| kind.is_dir()) {
        return false;
    }
    matches!(
        entry.file_name().to_str(),
        Some(
            ".git"
                | ".hg"
                | ".svn"
                | "node_modules"
                | "target"
                | "dist"
                | "build"
                | ".next"
                | ".venv"
                | "venv"
                | "__pycache__"
                | "vendor"
                | "coverage"
                | "generated"
                | "__generated__"
        )
    )
}

thread_local! {
    static REQUEST_CANCEL: std::cell::RefCell<Option<std::sync::Arc<std::sync::atomic::AtomicBool>>> = const {std::cell::RefCell::new(None)};
}
pub(crate) struct RequestCancellation;
impl RequestCancellation {
    pub(crate) fn enter(flag: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        REQUEST_CANCEL.with(|slot| *slot.borrow_mut() = Some(flag));
        Self
    }
}
impl Drop for RequestCancellation {
    fn drop(&mut self) {
        REQUEST_CANCEL.with(|slot| *slot.borrow_mut() = None);
    }
}
pub(crate) fn request_cancelled() -> bool {
    REQUEST_CANCEL.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::Relaxed))
    })
}

//! Opt-in allocation instrumentation: Rust System allocator only; C allocations
//! (including Tree-sitter) are represented by process RSS, not these counters.
use anyhow::Result;
use clap::Parser;
use serde::Serialize;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
struct Counting;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
fn allocated(size: usize) {
    CALLS.fetch_add(1, Ordering::Relaxed);
    BYTES.fetch_add(size, Ordering::Relaxed);
    let live = LIVE.fetch_add(size, Ordering::Relaxed) + size;
    PEAK.fetch_max(live, Ordering::Relaxed);
}
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            allocated(layout.size());
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            allocated(layout.size());
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe { System.dealloc(p, layout) };
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(p, layout, size) };
        if !new.is_null() {
            LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
            allocated(size);
        }
        new
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;
#[derive(Parser)]
struct Args {
    root: PathBuf,
    #[arg(long, default_value = "validate token")]
    query: String,
    #[arg(long, default_value_t = 5)]
    queries: usize,
    /// Measure a fresh resident process without earlier indexing phases.
    #[arg(long)]
    resident_only: bool,
}
#[derive(Serialize)]
struct Stage {
    stage: String,
    elapsed_us: u128,
    allocation_calls: usize,
    allocated_bytes: usize,
    live_rust_bytes: usize,
    peak_rust_bytes: usize,
    rss_bytes: Option<usize>,
    peak_rss_bytes: Option<usize>,
}
fn rss() -> Option<usize> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<usize>()
        .ok()
        .map(|k| k * 1024)
}
fn peak_rss() -> Option<usize> {
    #[cfg(unix)]
    {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
            return None;
        }
        let value = unsafe { usage.assume_init() }.ru_maxrss as usize;
        Some(if cfg!(target_os = "macos") {
            value
        } else {
            value * 1024
        })
    }
    #[cfg(not(unix))]
    {
        None
    }
}
fn measure<T>(name: &str, rows: &mut Vec<Stage>, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let calls = CALLS.load(Ordering::Relaxed);
    let bytes = BYTES.load(Ordering::Relaxed);
    PEAK.store(LIVE.load(Ordering::Relaxed), Ordering::Relaxed);
    let start = Instant::now();
    let result = f()?;
    let elapsed_us = start.elapsed().as_micros();
    let allocation_calls = CALLS.load(Ordering::Relaxed) - calls;
    let allocated_bytes = BYTES.load(Ordering::Relaxed) - bytes;
    let live_rust_bytes = LIVE.load(Ordering::Relaxed);
    let peak_rust_bytes = PEAK.load(Ordering::Relaxed);
    rows.push(Stage {
        stage: name.into(),
        elapsed_us,
        allocation_calls,
        allocated_bytes,
        live_rust_bytes,
        peak_rust_bytes,
        rss_bytes: rss(),
        peak_rss_bytes: peak_rss(),
    });
    Ok(result)
}
fn main() -> Result<()> {
    let args = Args::parse();
    let root = args.root.canonicalize()?;
    let mut rows = Vec::new();
    if !args.resident_only {
        let (paths, _) = measure("discovery", &mut rows, || {
            flexcontext::repository::discover_source_paths(&root)
        })?;
        let cold = measure("cold_indexing", &mut rows, || {
            flexcontext::cache::load_indexed_repository(&root, &paths, false)
        })?;
        drop(cold);
        let cached = measure("populate_cache", &mut rows, || {
            flexcontext::cache::load_indexed_repository(&root, &paths, true)
        })?;
        drop(cached);
        let warm = measure("cache_load", &mut rows, || {
            flexcontext::cache::load_indexed_repository(&root, &paths, true)
        })?;
        let prepared = measure("prepared_index", &mut rows, || {
            Ok(flexcontext::ranking::PreparedIndex::build(&warm.symbols))
        })?;
        let relations = measure("relation_index", &mut rows, || {
            Ok(flexcontext::relations::RelationIndex::build(&warm.symbols))
        })?;
        drop(relations);
        drop(prepared);
        drop(warm);
    }
    let session = measure("resident_open", &mut rows, || {
        flexcontext::SearchSession::open(&root, true)
    })?;
    let summary = session.index_summary();
    let source_bytes = &summary["source_bytes"];
    let symbols = &summary["symbols"];
    measure("resident_idle", &mut rows, || Ok(()))?;
    for _ in 0..args.queries {
        let response = measure("query", &mut rows, || session.query(&args.query, 12000, 12))?;
        drop(response);
    }
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"schema":1,"resident_only":args.resident_only,"source_bytes":source_bytes,"symbols":symbols,"allocator":"Rust System allocations; excludes native Tree-sitter allocations","rss":"ps RSS after each stage; peak RSS is process lifetime getrusage high water","stages":rows})
        )?
    );
    Ok(())
}

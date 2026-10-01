use anyhow::{Result, anyhow, ensure};
use clap::{Parser, Subcommand};
use flexcontext::{SearchOptions, repository::ScanLimits, search};
use std::path::PathBuf;
#[derive(Debug, Parser)]
#[command(
    name = "flexcontext",
    version,
    about = "Structural lexical context retrieval for coding agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Legacy alias for search ROOT QUERY.
    #[arg(long,value_names=["ROOT","QUERY"],num_args=2,conflicts_with="mcp")]
    code_search: Vec<String>,
    /// Alias for serve ROOT (MCP 2026-07-28 only).
    #[arg(long, value_name = "ROOT")]
    mcp: Option<PathBuf>,
    /// Deprecated source-token estimate. Prefer --max-tokens for emitted context.
    #[arg(long, global = true)]
    budget: Option<usize>,
    /// Estimated tokens for the complete emitted representation (bytes / 4).
    #[arg(long, global = true)]
    max_tokens: Option<usize>,
    #[arg(long, global = true)]
    json: bool,
    /// Source-selection byte limit; metadata is additional.
    #[arg(long, global = true)]
    max_bytes: Option<usize>,
    #[arg(long, global = true, default_value_t = 12)]
    max_results: usize,
    #[arg(long, global = true)]
    no_cache: bool,
    #[arg(long, global = true, default_value_t = 268_435_456)]
    max_source_bytes: u64,
    #[arg(long, global = true, default_value_t = 100_000)]
    max_source_files: usize,
    #[arg(long, global = true, default_value_t = 2_097_152)]
    max_file_bytes: u64,
    #[arg(long, global = true, default_value_t = 128)]
    max_depth: usize,
    /// Emit scan progress to stderr.
    #[arg(long, global = true)]
    progress: bool,
}
#[derive(Debug, Subcommand)]
enum Command {
    Search {
        root: PathBuf,
        query: String,
    },
    Serve {
        root: PathBuf,
    },
    Index {
        root: PathBuf,
    },
    Benchmark {
        #[arg(default_value = "benchmarks")]
        corpus: PathBuf,
        #[arg(long, default_value_t = 5)]
        k: usize,
        #[arg(long)]
        output: Option<PathBuf>,
    },
}
fn main() -> Result<()> {
    let cli = Cli::parse();
    ctrlc::set_handler(|| {
        static INTERRUPTED: std::sync::atomic::AtomicBool =
            std::sync::atomic::AtomicBool::new(false);
        if INTERRUPTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            std::process::exit(130);
        }
        flexcontext::repository::cancel_scan();
    })?;
    ensure!(
        cli.command.is_none() || (cli.code_search.is_empty() && cli.mcp.is_none()),
        "subcommands cannot be combined with legacy aliases"
    );
    let command = match cli.command {
        Some(command) => command,
        None if cli.mcp.is_some() => Command::Serve {
            root: cli.mcp.unwrap(),
        },
        None => {
            let [root, query] = cli.code_search.as_slice() else {
                return Err(anyhow!(
                    "use search <ROOT> <QUERY>, serve <ROOT>, index <ROOT>, or benchmark"
                ));
            };
            Command::Search {
                root: root.into(),
                query: query.clone(),
            }
        }
    };
    let limits = ScanLimits {
        max_file_bytes: cli.max_file_bytes,
        max_source_files: cli.max_source_files,
        max_source_bytes: cli.max_source_bytes,
        max_depth: cli.max_depth,
        progress: cli.progress,
        ..Default::default()
    };
    let max_bytes = match cli.budget {
        Some(0) => return Err(anyhow!("--budget must be greater than zero")),
        Some(tokens) => tokens
            .checked_mul(4)
            .ok_or_else(|| anyhow!("budget is too large"))?
            .min(cli.max_bytes.unwrap_or(usize::MAX)),
        None => cli.max_bytes.unwrap_or(16384),
    };
    match command {
        Command::Serve { root } => {
            let mut session =
                flexcontext::SearchSession::open_with_limits(&root, !cli.no_cache, &limits)?;
            flexcontext::mcp::serve(
                &mut session,
                std::io::BufReader::new(std::io::stdin()),
                std::io::stdout().lock(),
            )?;
        }
        Command::Index { root } => {
            let session =
                flexcontext::SearchSession::open_with_limits(&root, !cli.no_cache, &limits)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&session.index_summary())?
            );
        }
        Command::Benchmark { corpus, k, output } => {
            let report = flexcontext::evaluation::evaluate(&corpus, k, max_bytes)?;
            let json = serde_json::to_string_pretty(&report)? + "\n";
            if let Some(path) = output {
                std::fs::write(path, json)?;
            } else {
                print!("{json}");
            }
        }
        Command::Search { root, query } => {
            ensure!(
                cli.max_tokens != Some(0),
                "--max-tokens must be greater than zero"
            );
            let mut response = search(&SearchOptions {
                root,
                query,
                max_bytes,
                max_results: cli.max_results,
                use_cache: !cli.no_cache,
                scan_limits: limits,
                ..Default::default()
            })?;
            let representation = if cli.json {
                flexcontext::output::Representation::Json
            } else {
                flexcontext::output::Representation::Human
            };
            flexcontext::output::finalize(&mut response, &representation, cli.max_tokens)?;
            use std::io::Write;
            std::io::stdout()
                .lock()
                .write_all(&representation.render(&response)?)?;
        }
    }
    Ok(())
}

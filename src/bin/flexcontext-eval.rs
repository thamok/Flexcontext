use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "benchmarks")]
    corpus: PathBuf,
    #[arg(long, default_value_t = 5)]
    k: usize,
    #[arg(long, default_value_t = 12000)]
    max_bytes: usize,
    #[arg(long)]
    tune: bool,
    #[arg(long)]
    weights: Option<PathBuf>,
    #[arg(long)]
    output: Option<PathBuf>,
    /// Evaluate only named JSON suites (repeatable); defaults to the original four.
    #[arg(long, conflicts_with = "tune")]
    suite: Vec<String>,
}
fn main() -> Result<()> {
    let args = Args::parse();
    let json = if args.tune {
        serde_json::to_string_pretty(&flexcontext::evaluation::tune(
            &args.corpus,
            args.k,
            args.max_bytes,
        )?)?
    } else {
        let weights = if let Some(path) = args.weights {
            serde_json::from_slice(&std::fs::read(path)?)?
        } else {
            Default::default()
        };
        let suites: Vec<&str> = if args.suite.is_empty() {
            vec!["rust", "typescript", "python", "mixed-monorepo"]
        } else {
            args.suite.iter().map(String::as_str).collect()
        };
        serde_json::to_string_pretty(&flexcontext::evaluation::evaluate_suites(
            &args.corpus,
            args.k,
            args.max_bytes,
            &weights,
            &suites,
        )?)?
    } + "\n";
    if let Some(path) = args.output {
        std::fs::write(path, json)?;
    } else {
        print!("{json}");
    }
    Ok(())
}

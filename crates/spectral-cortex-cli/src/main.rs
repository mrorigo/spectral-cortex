//! CLI for the Spectral Cortex spectral-memory graph.
//!
//! Subcommands:
//!  - `ingest` : ingest a git repository into an in-memory SMG and build spectral structures.
//!
//! This file provides a minimal, well-documented `clap` skeleton and a working
//! `ingest` implementation that uses `git2` when the `git2-backend` feature is
//! enabled. The command will print a short summary after ingestion so you can
//! validate the pipeline end-to-end.
//!
//! Design goals:
//!  - Small, testable, and clear CLI surface.
//!  - Use the library crate (`spectral_cortex`) for ingestion and spectral ops.
//!  - Prefer `anyhow::Result` for application-level error handling.
//!
//! Usage examples:
//!  cargo run -p spectral-cortex -- ingest --repo /path/to/repo
//!
//! Notes:
//!  - The ingest command currently uses the library API (`SpectralMemoryGraph::new`,
//!    `ingest_turn` and `build_spectral_structure`) to perform work. Persistence
//!    (save/load) will be added in later phases.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use regex::{Regex, RegexBuilder};
use serde_json::json;
use spectral_cortex::embed;

mod git_commit_split;
mod mcp_server;

use crate::git_commit_split::{split_commit_message, CommitSplitConfig, CommitSplitStats};
use crate::mcp_server::run_mcp_server;

/// Local library crate export (hyphen -> underscore).
use spectral_cortex::{
    load_smg_json, save_smg_json,
    temporal::{TemporalConfig, TemporalMode},
    ConversationTurn, SpectralMemoryGraph,
};

/// CLI entrypoint.
#[derive(Parser)]
#[command(
    name = "spectral-cortex",
    about = "Spectral Cortex CLI — git history ingestion & query",
    version
)]
struct Cli {
    /// Subcommands
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Ingest a git repository into an SMG and build spectral structures.
    Ingest(IngestArgs),

    /// Incrementally update an existing SMG with only new commits (alias for ingest --append --incremental).
    Update(UpdateArgs),

    /// Query a persisted SMG for relevant notes.
    Query(QueryArgs),

    /// Retrieve a specific note by note_id and inspect related notes.
    Note(NoteArgs),

    /// Run an MCP stdio server using a preloaded SMG file.
    Mcp(McpArgs),
}

/// Arguments for the `ingest` subcommand.
#[derive(Args, Debug)]
struct IngestArgs {
    /// Path to the git repository (defaults to current directory).
    #[arg(short, long, value_name = "PATH", default_value = ".")]
    repo: PathBuf,

    /// Path to write SMG JSON output (optional).
    #[arg(long, short = 'o', value_name = "PATH")]
    out: Option<PathBuf>,

    /// If set, append ingested data to an existing SMG at `--out` (load then ingest).
    #[arg(long)]
    append: bool,

    /// Include diffs in the commit content (not implemented yet; placeholder).
    #[arg(long)]
    include_diff: bool,

    /// Maximum number of commits to ingest (useful for testing).
    #[arg(long)]
    max_commits: Option<usize>,

    /// Number of parallel embedding workers (default: 4).
    #[arg(long, default_value = "4")]
    workers: usize,

    /// Cache size per worker (default: 0, no caching for unique commits).
    #[arg(long, default_value = "100")]
    cache_size: usize,

    /// Drop commit message lines that match this regex. Repeatable.
    #[arg(long = "git-filter-drop", value_name = "REGEX")]
    git_filter_drop: Vec<String>,

    /// Built-in line filter preset. Supported: git-noise
    #[arg(long = "git-filter-preset", value_name = "NAME")]
    git_filter_preset: Option<String>,

    /// Apply case-insensitive matching for git line filters.
    #[arg(long = "git-filter-case-insensitive")]
    git_filter_case_insensitive: bool,

    /// Only ingest commits that are not already present in the target SMG (matched by commit_id).
    /// Recommended for post-commit hooks with `--append --out <smg.json>`.
    #[arg(long)]
    incremental: bool,

    /// Commit message split mode: off|auto|strict.
    #[arg(long = "git-commit-split-mode", default_value = "auto")]
    git_commit_split_mode: String,

    /// Maximum number of segments emitted per commit.
    #[arg(long = "git-commit-split-max-segments", default_value_t = 6)]
    git_commit_split_max_segments: usize,

    /// Minimum parser confidence for emitting split segments in auto mode (0.0..1.0).
    #[arg(long = "git-commit-split-min-confidence", default_value_t = 0.75)]
    git_commit_split_min_confidence: f32,
}

/// Arguments for the `update` subcommand.
#[derive(Args, Debug)]
struct UpdateArgs {
    /// Path to the git repository (defaults to current directory).
    #[arg(short, long, value_name = "PATH", default_value = ".")]
    repo: PathBuf,

    /// Path to SMG JSON file to update in place.
    #[arg(long, short = 'o', value_name = "PATH")]
    out: PathBuf,

    /// Maximum number of commits to scan from git history.
    #[arg(long)]
    max_commits: Option<usize>,

    /// Number of parallel embedding workers (default: 4).
    #[arg(long, default_value = "4")]
    workers: usize,

    /// Cache size per worker (default: 100).
    #[arg(long, default_value = "100")]
    cache_size: usize,

    /// Drop commit message lines that match this regex. Repeatable.
    #[arg(long = "git-filter-drop", value_name = "REGEX")]
    git_filter_drop: Vec<String>,

    /// Built-in line filter preset. Supported: git-noise
    #[arg(long = "git-filter-preset", value_name = "NAME")]
    git_filter_preset: Option<String>,

    /// Apply case-insensitive matching for git line filters.
    #[arg(long = "git-filter-case-insensitive")]
    git_filter_case_insensitive: bool,

    /// Commit message split mode: off|auto|strict.
    #[arg(long = "git-commit-split-mode", default_value = "auto")]
    git_commit_split_mode: String,

    /// Maximum number of segments emitted per commit.
    #[arg(long = "git-commit-split-max-segments", default_value_t = 6)]
    git_commit_split_max_segments: usize,

    /// Minimum parser confidence for emitting split segments in auto mode (0.0..1.0).
    #[arg(long = "git-commit-split-min-confidence", default_value_t = 0.75)]
    git_commit_split_min_confidence: f32,
}

/// Arguments for the `query` subcommand (skeleton).
#[derive(Args, Debug)]
struct QueryArgs {
    /// Query string to search for.
    #[arg(short, long)]
    query: Option<String>,

    /// Path to a saved SMG JSON file to load.
    #[arg(short = 's', long)]
    smg: Option<PathBuf>,

    /// Number of top results to return.
    #[arg(long, default_value_t = 5)]
    top_k: usize,

    /// Number of candidate results to retrieve before filtering and final selection.
    /// If omitted, defaults to `top_k * 5`.
    #[arg(long)]
    candidate_k: Option<usize>,

    /// Minimum score threshold (inclusive). Results with score < min_score are filtered out.
    /// Default: 0.7
    #[arg(long, default_value_t = 0.7)]
    min_score: f32,

    /// Disable temporal re-ranking for this query (temporal is enabled by default).
    #[arg(long)]
    no_temporal: bool,

    /// Temporal weight (0.0..1.0). Default: 0.20
    #[arg(long, default_value_t = 0.20)]
    temporal_weight: f32,

    /// Temporal mode: exponential|linear|step|buckets (default: exponential)
    #[arg(long, default_value = "exponential")]
    temporal_mode: String,

    /// Half-life (days) for exponential decay. Default: 14.0
    #[arg(long, default_value_t = 14.0)]
    temporal_half_life_days: f32,

    /// Optional override of "now" for reproducible queries / testing (RFC3339 string).
    #[arg(long)]
    temporal_now: Option<String>,

    /// Output results as JSON to stdout.
    #[arg(long)]
    json: bool,

    /// Optional start time for filtering notes (RFC3339 string).
    /// Only notes with timestamps >= this time will be considered.
    #[arg(long)]
    time_start: Option<String>,

    /// Optional end time for filtering notes (RFC3339 string).
    /// Only notes with timestamps <= this time will be considered.
    #[arg(long)]
    time_end: Option<String>,

    /// Alternative to time_start/time_end: look back this many days from now.
    /// Only notes with timestamps >= (now - time_window_days) will be considered.
    #[arg(long)]
    time_window_days: Option<f64>,

    /// Number of parallel embedding workers (default: 4).
    #[arg(long, default_value = "4")]
    workers: usize,

    /// Cache size per worker (default: 0, no caching for unique queries).
    #[arg(long, default_value = "0")]
    cache_size: usize,

    /// Number of long-range links to return (default: all).
    #[arg(long)]
    links_k: Option<usize>,
}

/// Arguments for the `note` subcommand.
#[derive(Args, Debug)]
struct NoteArgs {
    /// Path to a saved SMG JSON file to load.
    #[arg(short = 's', long)]
    smg: PathBuf,

    /// Note ID to inspect.
    #[arg(long)]
    note_id: u32,

    /// Number of related notes to return (default: all).
    #[arg(long)]
    links_k: Option<usize>,

    /// Output as JSON.
    #[arg(long)]
    json: bool,
}

/// Arguments for the `mcp` subcommand.
#[derive(Args, Debug)]
struct McpArgs {
    /// Path to the SMG JSON file to preload and serve.
    #[arg(short = 's', long = "smg", alias = "smd", value_name = "PATH")]
    smg: PathBuf,
}

/// Application entry point.
fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Ingest(args) => run_ingest(args),
        Commands::Update(args) => run_update(args),
        Commands::Query(args) => run_query(args),
        Commands::Note(args) => run_note(args),
        Commands::Mcp(args) => run_mcp(args),
    }
}

/// Run the `mcp` subcommand.
fn run_mcp(args: McpArgs) -> Result<()> {
    run_mcp_server(&args.smg)
}

/// Run the `update` subcommand as an alias for incremental append ingestion.
fn run_update(args: UpdateArgs) -> Result<()> {
    let ingest_args = IngestArgs {
        repo: args.repo,
        out: Some(args.out),
        append: true,
        include_diff: false,
        max_commits: args.max_commits,
        workers: args.workers,
        cache_size: args.cache_size,
        git_filter_drop: args.git_filter_drop,
        git_filter_preset: args.git_filter_preset,
        git_filter_case_insensitive: args.git_filter_case_insensitive,
        incremental: true,
        git_commit_split_mode: args.git_commit_split_mode,
        git_commit_split_max_segments: args.git_commit_split_max_segments,
        git_commit_split_min_confidence: args.git_commit_split_min_confidence,
    };
    run_ingest(ingest_args)
}

/// Run the `ingest` subcommand.
///
/// This function:
/// 1. Collects commits from the repository (using `git2` if available).
/// 2. Converts commits into `ConversationTurn` objects.
/// 3. Ingests them into `SpectralMemoryGraph`.
/// 4. Rebuilds spectral structures.
///
/// # Errors
///
/// Returns an `anyhow::Error` when IO/git operations fail or when the library API fails.
fn run_ingest(args: IngestArgs) -> Result<()> {
    println!("Starting ingest for repo: {}", args.repo.display());

    // Initialize embedding pool with CLI parameters
    println!(
        "Initializing embedding pool with {} workers...",
        args.workers
    );
    embed::init(args.workers, args.cache_size).with_context(|| "initializing embedding pool")?;

    // Ensure pool is shut down even if ingestion fails
    let _guard = scopeguard::guard((), |_| {
        let _ = embed::shutdown();
    });

    let git_filters = GitFilterConfig::from_ingest_args(&args)?;
    let split_config = CommitSplitConfig::from_ingest_args(&args)?;

    // Collect commits into conversation turns.
    let collected = collect_commits(&args.repo, args.max_commits, &git_filters, &split_config)
        .with_context(|| format!("collecting commits from {}", args.repo.display()))?;
    let mut turns = collected.turns;

    println!("Collected {} commits (turns).", turns.len());
    if git_filters.enabled() {
        let before = collected.filter_stats.total_chars_before;
        let after = collected.filter_stats.total_chars_after;
        let ratio = if before == 0 {
            0.0
        } else {
            (after as f64 / before as f64) * 100.0
        };
        println!(
            "Git filter summary: seen={} kept={} skipped={} dropped_lines={} chars_before={} chars_after={} ({:.1}% retained)",
            collected.filter_stats.total_commits_seen,
            collected.filter_stats.commits_kept,
            collected.filter_stats.commits_skipped_empty,
            collected.filter_stats.lines_dropped,
            before,
            after,
            ratio
        );
    }
    println!(
        "Commit split summary: mode={} commits_seen={} commits_split={} total_segments={} fallback_single={} parser_modes=[headers:{} bullets:{} paragraphs:{}]",
        split_config.mode.as_str(),
        collected.split_stats.commits_seen,
        collected.split_stats.commits_split,
        collected.split_stats.total_segments_emitted,
        collected.split_stats.fallback_to_single,
        collected.split_stats.segments_from_headers,
        collected.split_stats.segments_from_bullets,
        collected.split_stats.segments_from_paragraphs
    );

    // Validate append/out combination.
    if args.append && args.out.is_none() {
        return Err(anyhow::anyhow!(
            "--append requires --out <path> to be provided"
        ));
    }
    if args.incremental && args.out.is_none() {
        return Err(anyhow::anyhow!(
            "--incremental requires --out <path> so existing commits can be compared"
        ));
    }

    // Initialize or load SMG. If --append/--incremental and --out points to an existing file, load it first.
    let should_load_existing = args.append || args.incremental;
    let mut smg = if should_load_existing {
        let outp = args
            .out
            .as_ref()
            .expect("--out required when using --append/--incremental");
        if outp.exists() {
            println!("Loading existing SMG from {}", outp.display());
            load_smg_json(outp).with_context(|| format!("loading SMG from {}", outp.display()))?
        } else {
            println!(
                "Output path {} does not exist, creating new SMG.",
                outp.display()
            );
            SpectralMemoryGraph::new().context("initializing SpectralMemoryGraph")?
        }
    } else {
        SpectralMemoryGraph::new().context("initializing SpectralMemoryGraph")?
    };

    if args.incremental {
        let existing_commit_ids: HashSet<String> = smg
            .notes
            .values()
            .flat_map(|note| note.source_commit_ids.iter())
            .filter_map(|cid| cid.clone())
            .collect();

        let before = turns.len();
        turns.retain(|turn| match &turn.commit_id {
            Some(cid) => !existing_commit_ids.contains(cid),
            None => true,
        });
        let skipped = before.saturating_sub(turns.len());
        println!(
            "Incremental mode: {} existing commits skipped, {} new commits to ingest.",
            skipped,
            turns.len()
        );
    }

    // Ensure globally unique turn IDs across repeated append/update runs.
    let max_existing_turn_id = smg
        .notes
        .values()
        .flat_map(|note| note.source_turn_ids.iter().copied())
        .max()
        .unwrap_or(0);
    for (i, turn) in turns.iter_mut().enumerate() {
        turn.turn_id = max_existing_turn_id.saturating_add(i as u64 + 1);
    }

    if turns.is_empty() {
        println!("No new turns to ingest.");
        if let Some(outp) = args.out {
            save_smg_json(&smg, &outp)
                .with_context(|| format!("saving SMG to {}", outp.display()))?;
            println!("Saved SMG to {}", outp.display());
        }
        println!(
            "SMG summary: notes = {}, cluster_labels_present = {}",
            smg.notes.len(),
            smg.cluster_labels
                .as_ref()
                .map(|labels| !labels.is_empty())
                .unwrap_or(false)
        );
        return Ok(());
    }

    // Ingest turns in batch for optimized embedding performance.
    let ingest_bar = ProgressBar::new(turns.len() as u64);
    ingest_bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );
    ingest_bar.set_message("Embedding commits...");

    // Capture the total count before creating the closure
    let total_turns = turns.len();

    // Create a progress callback that updates the bar
    let progress_cb = Arc::new({
        let bar = ingest_bar.clone();
        move |msg: String, fraction: f32| {
            bar.set_message(msg);
            bar.set_position((fraction * total_turns as f32).floor() as u64);
        }
    });

    smg.ingest_turns_batch(&turns, Some(progress_cb))
        .with_context(|| "batch embedding turns")?;

    ingest_bar.finish_with_message(format!("Ingested {} turns into the SMG.", smg.notes.len()));

    // Always rebuild spectral structures with progress bar.
    let spectral_bar = ProgressBar::new(10);
    spectral_bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] [{bar:40.green/yellow}] {pos}/{len} {msg}")
            .unwrap()
            .progress_chars("##-"),
    );
    spectral_bar.set_message("Building spectral structures...");

    // Create a progress callback that updates the bar
    let progress_cb = Arc::new({
        let bar = spectral_bar.clone();
        move |msg: String, fraction: f32| {
            bar.set_message(msg);
            bar.set_position((fraction * 10.0).floor() as u64);
        }
    });

    smg.build_spectral_structure(Some(progress_cb))
        .context("building spectral structures")?;
    spectral_bar.finish_with_message("Spectral build complete.");

    // Optionally persist to JSON.
    if let Some(outp) = args.out {
        save_smg_json(&smg, &outp).with_context(|| format!("saving SMG to {}", outp.display()))?;
        println!("Saved SMG to {}", outp.display());
    }

    // Summary output: number of notes and some cluster info if present.
    let notes_count = smg.notes.len();
    let clusters = smg
        .cluster_labels
        .as_ref()
        .map(|labels| labels.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    println!(
        "SMG summary: notes = {}, cluster_labels_present = {}",
        notes_count,
        !clusters.is_empty()
    );

    Ok(())
}

#[derive(Debug, Default)]
struct GitFilterStats {
    total_commits_seen: usize,
    commits_kept: usize,
    commits_skipped_empty: usize,
    lines_dropped: usize,
    total_chars_before: usize,
    total_chars_after: usize,
}

#[derive(Debug, Default)]
struct GitFilterConfig {
    drop_patterns: Vec<Regex>,
}

impl GitFilterConfig {
    fn enabled(&self) -> bool {
        !self.drop_patterns.is_empty()
    }

    fn from_ingest_args(args: &IngestArgs) -> Result<Self> {
        let mut raw_patterns: Vec<String> = Vec::new();

        if let Some(preset) = args.git_filter_preset.as_ref() {
            match preset.to_lowercase().as_str() {
                "git-noise" => {
                    raw_patterns.extend([
                        String::from(r"^Co-authored-by:"),
                        String::from(r"^Signed-off-by:"),
                        String::from(r"^Reviewed-by:"),
                        String::from(r"^Change-Id:"),
                        String::from(r"^See merge request"),
                    ]);
                }
                other => {
                    return Err(anyhow::anyhow!(
                        "unsupported --git-filter-preset '{}'; supported: git-noise",
                        other
                    ));
                }
            }
        }

        raw_patterns.extend(args.git_filter_drop.iter().cloned());

        let mut drop_patterns = Vec::with_capacity(raw_patterns.len());
        for pattern in raw_patterns {
            let rx = RegexBuilder::new(&pattern)
                .case_insensitive(args.git_filter_case_insensitive)
                .build()
                .with_context(|| format!("invalid --git-filter-drop regex: '{}'", pattern))?;
            drop_patterns.push(rx);
        }

        Ok(Self { drop_patterns })
    }
}

struct CollectCommitsOutput {
    turns: Vec<ConversationTurn>,
    filter_stats: GitFilterStats,
    split_stats: CommitSplitStats,
}

fn apply_git_line_filters(
    message: &str,
    filters: &GitFilterConfig,
    stats: &mut GitFilterStats,
) -> Option<String> {
    stats.total_commits_seen = stats.total_commits_seen.saturating_add(1);
    stats.total_chars_before = stats.total_chars_before.saturating_add(message.len());

    if message.trim().is_empty() {
        stats.commits_skipped_empty = stats.commits_skipped_empty.saturating_add(1);
        return None;
    }

    if !filters.enabled() {
        stats.commits_kept = stats.commits_kept.saturating_add(1);
        stats.total_chars_after = stats.total_chars_after.saturating_add(message.len());
        return Some(message.to_string());
    }

    let lines: Vec<&str> = message.lines().collect();
    let mut out_lines: Vec<String> = Vec::new();

    // Keep subject line by default for semantic anchor.
    if let Some(subject) = lines.first() {
        let subject = subject.trim();
        if !subject.is_empty() {
            out_lines.push(subject.to_string());
        }
    }

    for line in lines.iter().skip(1) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let should_drop = filters.drop_patterns.iter().any(|rx| rx.is_match(trimmed));
        if should_drop {
            stats.lines_dropped = stats.lines_dropped.saturating_add(1);
            continue;
        }

        out_lines.push(trimmed.to_string());
    }

    // De-duplicate consecutive identical lines after filtering.
    let mut deduped: Vec<String> = Vec::with_capacity(out_lines.len());
    for line in out_lines.into_iter() {
        if deduped.last() != Some(&line) {
            deduped.push(line);
        }
    }

    let filtered = deduped.join("\n");
    if filtered.trim().is_empty() {
        stats.commits_skipped_empty = stats.commits_skipped_empty.saturating_add(1);
        return None;
    }

    stats.commits_kept = stats.commits_kept.saturating_add(1);
    stats.total_chars_after = stats.total_chars_after.saturating_add(filtered.len());
    Some(filtered)
}

/// Run the `query` subcommand.
fn run_query(args: QueryArgs) -> Result<()> {
    // Initialize embedding pool with CLI parameters
    let _start_total = Instant::now();
    let _ = _start_total; // suppress unused warning
    embed::init(args.workers, args.cache_size).with_context(|| "initializing embedding pool")?;

    // Require query string and SMG path.
    let q = args
        .query
        .ok_or_else(|| anyhow::anyhow!("--query is required"))?;
    let smg_path = args
        .smg
        .ok_or_else(|| anyhow::anyhow!("--smg <path> is required"))?;

    // Load the persisted SMG and run retrieval.
    let start_load = Instant::now();
    let smg = load_smg_json(&smg_path)
        .with_context(|| format!("loading SMG from {}", smg_path.display()))?;
    eprintln!("Loaded SMG in {:?}", start_load.elapsed());

    // Determine how many candidates to retrieve (default = top_k * 5).
    let candidate_k = args.candidate_k.unwrap_or(args.top_k.saturating_mul(5));

    // Parse temporal now if provided.
    let now_seconds_override = if let Some(now_str) = args.temporal_now.as_ref() {
        let dt = chrono::DateTime::parse_from_rfc3339(now_str)
            .with_context(|| format!("Failed to parse --temporal-now as RFC3339: {}", now_str))?;
        Some(dt.timestamp() as u64)
    } else {
        None
    };

    // Parse time filtering arguments.
    let time_start_seconds = if let Some(start_str) = args.time_start.as_ref() {
        let dt = chrono::DateTime::parse_from_rfc3339(start_str)
            .with_context(|| format!("Failed to parse --time-start as RFC3339: {}", start_str))?;
        Some(dt.timestamp() as u64)
    } else {
        None
    };

    let _time_end_seconds = if let Some(end_str) = args.time_end.as_ref() {
        let dt = chrono::DateTime::parse_from_rfc3339(end_str)
            .with_context(|| format!("Failed to parse --time-end as RFC3339: {}", end_str))?;
        Some(dt.timestamp() as u64)
    } else {
        None
    };

    // Compute time window if provided.
    let time_window_start_seconds = if let Some(window_days) = args.time_window_days {
        let now = now_seconds_override.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0)
        });
        let window_seconds = (window_days * 86400.0) as u64;
        Some(now.saturating_sub(window_seconds))
    } else {
        None
    };

    // Combine time filters: time_start takes precedence over time_window.
    let _effective_time_start = time_start_seconds.or(time_window_start_seconds);

    // Construct temporal config from CLI flags.
    let mode = match args.temporal_mode.to_lowercase().as_str() {
        "linear" | "linearwindow" => TemporalMode::LinearWindow,
        "step" => TemporalMode::Step,
        "buckets" => TemporalMode::Buckets,
        _ => TemporalMode::Exponential,
    };

    let tcfg = TemporalConfig {
        enabled: !args.no_temporal,
        weight: args.temporal_weight,
        mode,
        half_life_seconds: Some((args.temporal_half_life_days * 86400.0) as u64),
        window_seconds: None,
        boost_magnitude: None,
        buckets: None,
        now_seconds: now_seconds_override,
    };

    let start_retrieve = Instant::now();

    // Retrieve candidates (this includes embedding the query internally)
    let start_candidates = Instant::now();
    let candidates = smg
        .retrieve_candidates(&q, candidate_k)
        .with_context(|| "retrieving candidates")?;
    eprintln!(
        "Retrieved {} candidates in {:?}",
        candidates.len(),
        start_candidates.elapsed()
    );

    // Step 3: Temporal re-ranking
    // let start_temporal = Instant::now();
    let re_ranked = spectral_cortex::temporal::re_rank_with_temporal(candidates, &tcfg, None);
    // eprintln!("Temporal re-ranking in {:?}", start_temporal.elapsed());

    // Step 4: Convert to final scored results
    let mut scored: Vec<(u64, f32)> = re_ranked
        .into_iter()
        .map(|cws| (cws.candidate.turn_id, cws.final_score))
        .collect();

    eprintln!("Total retrieval in {:?}", start_retrieve.elapsed());

    // Apply minimum score filtering (inclusive) on the final_score produced by retrieval.
    let min_score = args.min_score;
    scored.retain(|(_tid, score)| *score >= min_score);

    // Sort by final score descending and truncate to the requested `top_k` final results.
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    if scored.len() > args.top_k {
        scored.truncate(args.top_k);
    }

    // Use `final_results` as the unified list used by both JSON and human output paths.
    let final_results = scored;

    if args.json {
        // Produce a JSON payload including note content, metadata and score for each returned turn.
        let mut results: Vec<serde_json::Value> = Vec::with_capacity(final_results.len());
        // Prepare a deterministic ordering of notes to map cluster labels (if present).
        let mut note_ids: Vec<u32> = smg.notes.keys().cloned().collect();
        note_ids.sort_unstable();
        for (tid, score) in final_results.iter() {
            // Find a note that contains this turn id.
            let mut found: Option<(u32, &spectral_cortex::model::smg_note::SMGNote)> = None;
            for nid in note_ids.iter() {
                if let Some(note) = smg.notes.get(nid) {
                    if note.source_turn_ids.contains(tid) {
                        found = Some((*nid, note));
                        break;
                    }
                }
            }
            if let Some((nid, note)) = found {
                // Find commit id corresponding to this turn (if present).
                let commit_id_for_turn: Option<String> = note
                    .source_turn_ids
                    .iter()
                    .position(|x| x == tid)
                    .and_then(|idx| note.source_commit_ids.get(idx).cloned().flatten());

                // Base object for the note (include score and commit id).
                let related_notes: Vec<serde_json::Value> = smg
                    .get_related_note_links(nid, args.links_k)
                    .into_iter()
                    .map(|(related_nid, sim)| {
                        serde_json::json!({
                            "note_id": related_nid,
                            "spectral_similarity": sim
                        })
                    })
                    .collect();
                let mut obj = serde_json::json!({
                    "turn_id": tid,
                    "note_id": nid,
                    "score": score,
                    "raw_content": note.raw_content,
                    "context": note.context,
                    "source_turn_ids": note.source_turn_ids,
                    "related_notes": related_notes,
                    "commit_id": commit_id_for_turn,
                });
                // If cluster labels are present, map the note id to its label using the sorted ordering.
                if let Some(labels) = smg.cluster_labels.as_ref() {
                    if let Some(idx) = note_ids.iter().position(|x| x == &nid) {
                        if let Some(lbl) = labels.get(idx) {
                            // Insert cluster label into the JSON object.
                            if let Some(map) = obj.as_object_mut() {
                                map.insert(
                                    "cluster_label".to_string(),
                                    serde_json::Value::from(*lbl),
                                );
                            }
                        }
                    }
                }
                results.push(obj);
            } else {
                // No associated note found; include the turn id and score only.
                results.push(serde_json::json!({ "turn_id": tid, "score": score }));
            }
        }
        // Echo the effective temporal configuration in the JSON output.
        let temporal_info = json!({
            "enabled": !args.no_temporal,
            "weight": args.temporal_weight,
            "mode": args.temporal_mode,
            "half_life_days": args.temporal_half_life_days,
            "now": args.temporal_now,
        });

        // Get long-range links if requested
        let long_range_links: Vec<serde_json::Value> = smg
            .get_long_range_links(args.links_k)
            .into_iter()
            .map(|(a, b, score)| {
                serde_json::json!({
                    "note_id_a": a,
                    "note_id_b": b,
                    "spectral_similarity": score
                })
            })
            .collect();

        let out = json!({
            "query": q,
            "smg": smg_path.to_string_lossy().to_string(),
            "top_k": args.top_k,
            "temporal": temporal_info,
            "results": results,
            "long_range_links": long_range_links,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("Top {} matching results for query {:?}:", args.top_k, q);
        // Print a short human-readable snippet per result, including score when available.
        let mut note_ids: Vec<u32> = smg.notes.keys().cloned().collect();
        note_ids.sort_unstable();
        for (i, (tid, score)) in final_results.iter().enumerate() {
            // Attempt to find the note containing this turn id to show a snippet.
            let mut snippet: Option<String> = None;
            let mut note_id_opt: Option<u32> = None;
            let mut commit_for_tid: Option<String> = None;
            for nid in note_ids.iter() {
                if let Some(note) = smg.notes.get(nid) {
                    if note.source_turn_ids.contains(tid) {
                        let raw = &note.raw_content;
                        let sn = if raw.len() > 120 {
                            format!("{}...", &raw[..120])
                        } else {
                            raw.clone()
                        };
                        snippet = Some(sn);
                        note_id_opt = Some(*nid);
                        // Compute commit id corresponding to this turn if available.
                        commit_for_tid = note
                            .source_turn_ids
                            .iter()
                            .position(|x| x == tid)
                            .and_then(|idx| note.source_commit_ids.get(idx).cloned().flatten());
                        break;
                    }
                }
            }
            if let Some(nid) = note_id_opt {
                if let Some(sn) = snippet {
                    if let Some(cid) = &commit_for_tid {
                        println!(
                            "{}. turn_id={} note_id={} commit_id={} score={} snippet: {}",
                            i + 1,
                            tid,
                            nid,
                            cid,
                            score,
                            sn
                        );
                    } else {
                        println!(
                            "{}. turn_id={} note_id={} score={} snippet: {}",
                            i + 1,
                            tid,
                            nid,
                            score,
                            sn
                        );
                    }
                } else {
                    println!("{}. turn_id={} score={}", i + 1, tid, score);
                }
            } else {
                println!("{}. turn_id={} score={}", i + 1, tid, score);
            }
        }

        // Print long-range links if requested
        if let Some(k) = args.links_k {
            let links = smg.get_long_range_links(Some(k));
            if !links.is_empty() {
                println!("\nLong-range links (spectrally similar but semantically distant):");
                for (a, b, score) in links {
                    println!("  note_id {} <-> {} (spectral sim: {:.3})", a, b, score);
                }
            }
        }
    }

    Ok(())
}

/// Run the `note` subcommand.
fn run_note(args: NoteArgs) -> Result<()> {
    let smg = load_smg_json(&args.smg)
        .with_context(|| format!("loading SMG from {}", args.smg.display()))?;

    let note = smg.notes.get(&args.note_id).ok_or_else(|| {
        anyhow::anyhow!(
            "note_id {} not found (SMG contains {} notes)",
            args.note_id,
            smg.notes.len()
        )
    })?;

    let cluster_label = smg.cluster_labels.as_ref().and_then(|labels| {
        let mut note_ids: Vec<u32> = smg.notes.keys().cloned().collect();
        note_ids.sort_unstable();
        note_ids
            .iter()
            .position(|x| x == &args.note_id)
            .and_then(|idx| labels.get(idx).copied())
    });

    let related = smg.get_related_note_links(args.note_id, args.links_k);

    if args.json {
        let related_json: Vec<serde_json::Value> = related
            .iter()
            .map(|(related_id, sim)| {
                if let Some(rnote) = smg.notes.get(related_id) {
                    serde_json::json!({
                        "note_id": related_id,
                        "spectral_similarity": sim,
                        "context": rnote.context,
                        "source_turn_ids": rnote.source_turn_ids,
                    })
                } else {
                    serde_json::json!({
                        "note_id": related_id,
                        "spectral_similarity": sim
                    })
                }
            })
            .collect();

        let out = json!({
            "smg": args.smg.to_string_lossy().to_string(),
            "note": {
                "note_id": note.note_id,
                "context": note.context,
                "raw_content": note.raw_content,
                "source_turn_ids": note.source_turn_ids,
                "source_commit_ids": note.source_commit_ids,
                "cluster_label": cluster_label,
            },
            "related_notes": related_json
        });

        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("note_id={}", note.note_id);
        if let Some(lbl) = cluster_label {
            println!("cluster_label={}", lbl);
        }
        println!("source_turn_ids={:?}", note.source_turn_ids);
        println!("context: {}", note.context);
        let snippet = if note.raw_content.len() > 200 {
            format!("{}...", &note.raw_content[..200])
        } else {
            note.raw_content.clone()
        };
        println!("raw_content: {}", snippet);

        if related.is_empty() {
            println!("\nNo related notes found.");
        } else {
            println!("\nRelated notes:");
            for (related_id, sim) in related {
                if let Some(rnote) = smg.notes.get(&related_id) {
                    let rsn = if rnote.raw_content.len() > 120 {
                        format!("{}...", &rnote.raw_content[..120])
                    } else {
                        rnote.raw_content.clone()
                    };
                    println!(
                        "  note_id={} spectral_similarity={:.6} source_turn_ids={:?} snippet: {}",
                        related_id, sim, rnote.source_turn_ids, rsn
                    );
                } else {
                    println!(
                        "  note_id={} spectral_similarity={:.6} (note payload missing)",
                        related_id, sim
                    );
                }
            }
        }
    }

    Ok(())
}

/// Collect commits from a git repository and convert them to `ConversationTurn`.
///
/// This function uses the `git2` backend when the `git2-backend` feature is enabled.
/// If the feature is not enabled, it returns an informative error.
///
/// # Arguments
///
/// * `repo_path` - Path to the repo (directory containing `.git`).
/// * `max_commits` - Optional limit on number of commits to collect.
///
/// # Returns
///
/// A vector of `ConversationTurn` objects in reverse chronological order (most recent first).
fn collect_commits(
    repo_path: &PathBuf,
    max_commits: Option<usize>,
    filters: &GitFilterConfig,
    split_config: &CommitSplitConfig,
) -> Result<CollectCommitsOutput> {
    // The implementation uses git2 when compiled with the feature; otherwise, fail-fast.
    #[cfg(feature = "git2-backend")]
    {
        use git2::{Repository, Revwalk, Sort};

        let repo = Repository::open(repo_path).with_context(|| {
            format!("failed to open git repository at '{}'", repo_path.display())
        })?;

        // Create a revwalk starting at HEAD, sorted by time (descending)
        let mut revwalk: Revwalk = repo.revwalk()?;
        revwalk.push_head()?;
        revwalk.set_sorting(Sort::TIME)?;

        let mut turns: Vec<ConversationTurn> = Vec::new();
        let mut filter_stats = GitFilterStats::default();
        let mut split_stats = CommitSplitStats::default();
        let mut idx: u64 = 1;

        for (i, oid_result) in revwalk.enumerate() {
            if let Some(limit) = max_commits {
                if i >= limit {
                    break;
                }
            }
            let oid = oid_result?;
            let commit = repo.find_commit(oid)?;

            // Extract commit data.
            let author = commit.author();
            let author_name = author.name().unwrap_or("unknown").to_string();
            let message = commit.message().unwrap_or("").to_string();
            let filtered_content =
                match apply_git_line_filters(&message, filters, &mut filter_stats) {
                    Some(content) => content,
                    None => continue,
                };

            // Timestamp seconds -> u64
            let time = commit.time();
            let timestamp = time.seconds() as u64;

            let commit_id = oid.to_string();
            let segments = split_commit_message(&filtered_content, split_config, &mut split_stats);

            for segment in segments {
                // Construct ConversationTurn. The library expects turn_id:u64 and fields per README.
                let turn = ConversationTurn {
                    turn_id: idx,
                    speaker: author_name.clone(),
                    content: segment.to_content(),
                    topic: "git".to_string(),
                    entities: Vec::new(),
                    // Record the originating commit id (hex) for easy lookup from query results.
                    commit_id: Some(commit_id.clone()),
                    timestamp,
                };

                turns.push(turn);
                idx = idx.saturating_add(1);
            }
        }

        Ok(CollectCommitsOutput {
            turns,
            filter_stats,
            split_stats,
        })
    }

    #[cfg(not(feature = "git2-backend"))]
    {
        bail!("git2 backend feature is not enabled. Rebuild the CLI with '--features git2-backend' or enable the default features.");
    }
}

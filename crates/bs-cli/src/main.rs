mod commands;
mod thresholds;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "borescope",
    version,
    about = "Static call-path engine — flamegraphs from structure, not execution"
)]
struct Cli {
    /// Repository root (default: discovered via .git)
    #[arg(long, global = true)]
    repo: Option<PathBuf>,

    /// Maximum tree depth
    #[arg(long, global = true, default_value = "3")]
    depth: u32,

    /// Zoom level: pkg | mod | fn
    #[arg(long, global = true, default_value = "fn")]
    zoom: String,

    /// Weight: none | loc | fanin | churn | hotspot | diff
    #[arg(long, global = true, default_value = "none")]
    weight: bs_render::Weight,

    /// Hide edges below confidence threshold (external:* edges are always shown)
    #[arg(long, global = true, default_value = "0.3")]
    min_confidence: f32,

    /// Output format: tree | folded | json | html | tui | mermaid | dot
    #[arg(short = 'o', long, global = true, default_value = "tree")]
    output: bs_render::OutputFormat,

    /// Plain ASCII tree (no ANSI color)
    #[arg(long, global = true)]
    no_color: bool,

    /// Emit raw diagram syntax without a fenced code block (mermaid/dot only)
    #[arg(long, global = true)]
    no_fence: bool,

    /// Quiet output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build or update .borescope/ index
    Index(commands::index::IndexArgs),
    /// Forward slice: everything reachable from target
    Paths(commands::paths::PathsArgs),
    /// Reverse slice: all callers of target
    Callers(commands::callers::CallersArgs),
    /// Call-tree diff between revisions
    Diff(commands::diff::DiffArgs),
    /// Diff from merge-base of a branch
    Branch(commands::branch::BranchArgs),
    /// Repository overview
    Map(commands::map::MapArgs),
    /// Ranked churn × complexity table
    Hotspots(commands::hotspots::HotspotsArgs),
    /// Co-change partners of a file or symbol
    Coupled(commands::coupled::CoupledArgs),
    /// Code-age view
    Age(commands::age::AgeArgs),
    /// Antipattern report
    Smells(commands::smells::SmellsArgs),
    /// Plain-English explanation of a symbol's signals
    Explain(commands::explain::ExplainArgs),
    /// PR impact analysis: risk, blast radius, co-change warnings
    ExplainPr(commands::explain_pr::ExplainPrArgs),
    /// Print the embedded skill file (for Claude Code, Cursor, agent system prompts)
    Skill(commands::skill::SkillArgs),
    /// Per-project memory: architectural decisions, team notes, and recent worklog
    Memo(commands::memo::MemoArgs),
}

fn main() {
    let cli = Cli::parse();

    // Commands that don't need a repo index — dispatch before resolve_repo
    if let Commands::Skill(ref args) = cli.command {
        if let Err(e) = commands::skill::run(args) {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
        return;
    }

    let repo_root = match commands::resolve_repo(cli.repo.as_deref()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    let ctx = commands::Context {
        repo_root,
        depth: cli.depth,
        zoom: cli.zoom,
        weight: cli.weight,
        min_confidence: cli.min_confidence,
        output: cli.output,
        no_color: cli.no_color,
        no_fence: cli.no_fence,
        quiet: cli.quiet,
        verbose: cli.verbose,
    };

    // --weight diff is only meaningful with a revision pair; reject early with a clear message
    if ctx.weight.requires_diff_context() {
        match &cli.command {
            Commands::Diff(_) | Commands::Branch(_) => {}
            _ => {
                eprintln!(
                    "error: --weight diff requires a revision pair — use it with `diff` or `branch`, not this command"
                );
                std::process::exit(2);
            }
        }
    }

    let result = match &cli.command {
        Commands::Index(args) => commands::index::run(&ctx, args),
        Commands::Paths(args) => commands::paths::run(&ctx, args),
        Commands::Callers(args) => commands::callers::run(&ctx, args),
        Commands::Diff(args) => commands::diff::run(&ctx, args),
        Commands::Branch(args) => commands::branch::run(&ctx, args),
        Commands::Map(args) => commands::map::run(&ctx, args),
        Commands::Hotspots(args) => commands::hotspots::run(&ctx, args),
        Commands::Coupled(args) => commands::coupled::run(&ctx, args),
        Commands::Age(args) => commands::age::run(&ctx, args),
        Commands::Smells(args) => commands::smells::run(&ctx, args),
        Commands::Explain(args) => commands::explain::run(&ctx, args),
        Commands::ExplainPr(args) => commands::explain_pr::run(&ctx, args),
        Commands::Skill(args) => commands::skill::run(args),
        Commands::Memo(args) => commands::memo::run(&ctx, args),
    };

    if let Err(e) = result {
        match e.downcast_ref::<bs_core::Error>() {
            Some(bs_core::Error::NoIndex) => {
                eprintln!("error: no index found — run `borescope index` first");
                std::process::exit(4);
            }
            Some(bs_core::Error::AmbiguousTarget(target, n)) => {
                eprintln!("error: ambiguous target '{target}' ({n} candidates)");
                std::process::exit(3);
            }
            Some(bs_core::Error::GrammarUnavailable(lang)) => {
                eprintln!("error: grammar unavailable for {lang}");
                std::process::exit(5);
            }
            _ => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

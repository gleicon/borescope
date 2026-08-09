use super::Context;
use anyhow::Result;
use bs_core::Store;
use bs_git::Miner;
use clap::Args;
use std::path::PathBuf;

#[derive(Args)]
pub struct IndexArgs {
    /// Force full reindex
    #[arg(long)]
    pub full: bool,

    /// Mine git history (default: true)
    #[arg(long, default_value = "true")]
    pub git: bool,

    /// Path to additional grammar directory
    #[arg(long)]
    pub grammar_path: Option<PathBuf>,
}

pub fn run(ctx: &Context, args: &IndexArgs) -> Result<()> {
    let store = Store::open(&ctx.repo_root)?;
    ensure_gitignore(&ctx.repo_root)?;

    let t0 = std::time::Instant::now();

    if args.git {
        if !ctx.quiet {
            eprintln!("Mining git history...");
        }
        let miner = Miner::new(ctx.repo_root.clone());
        miner.mine(&store, args.full)?;
        if !ctx.quiet {
            eprintln!("  {} files", store.file_count()?);
        }
    }

    if !ctx.quiet {
        eprintln!("Extracting symbols...");
    }
    let result = bs_extract::extract_repo(&store, &ctx.repo_root, args.grammar_path.as_deref())?;

    if !ctx.quiet {
        eprintln!(
            "  {} files, {} symbols",
            result.files_processed, result.symbols_found
        );
    }

    if !ctx.quiet {
        eprintln!("Linking...");
    }
    let link_stats = bs_link::link(&store)?;
    if ctx.verbose {
        eprintln!(
            "  resolved: {}, unresolved: {}",
            link_stats.resolved, link_stats.left_unresolved
        );
    }

    // Span-level git attribution: attribute churn to individual symbol spans
    if args.git {
        if !ctx.quiet {
            eprintln!("Attributing spans...");
        }
        let miner = Miner::new(ctx.repo_root.clone());
        let all_stats = store.get_all_file_stats()?;
        let mut attributed = 0usize;
        for stat in &all_stats {
            if store
                .symbols_for_file(&stat.path)
                .map(|s| s.len())
                .unwrap_or(0)
                > 0
            {
                let _ = miner.mine_symbol_spans(&store, &stat.path);
                attributed += 1;
            }
        }
        if ctx.verbose {
            eprintln!("  attributed {} files", attributed);
        }
    }

    let elapsed = t0.elapsed();
    if !ctx.quiet {
        eprintln!("Done in {:.1}s", elapsed.as_secs_f64());
    }

    Ok(())
}

fn ensure_gitignore(repo_root: &std::path::Path) -> Result<()> {
    let gi = repo_root.join(".gitignore");
    let entry = ".borescope/\n";
    if gi.exists() {
        let contents = std::fs::read_to_string(&gi)?;
        if contents.contains(".borescope") {
            return Ok(());
        }
        let mut contents = contents;
        if !contents.ends_with('\n') {
            contents.push('\n');
        }
        contents.push_str(entry);
        std::fs::write(&gi, contents)?;
    } else {
        std::fs::write(&gi, entry)?;
    }
    eprintln!("note: added .borescope/ to .gitignore");
    Ok(())
}

use super::{emit, open_store, Context};
use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct CoupledArgs {
    /// File or symbol target
    pub target: String,

    /// Minimum co-change strength (0..1)
    #[arg(long, default_value = "0.3")]
    pub min: f32,

    /// Minimum commit support
    #[arg(long, default_value = "5")]
    pub support: u32,
}

pub fn run(ctx: &Context, args: &CoupledArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let results = store.get_coupled(&args.target, args.min, args.support)?;

    let out = match ctx.output {
        bs_render::OutputFormat::Json => serde_json::to_string_pretty(&results).unwrap_or_default(),
        bs_render::OutputFormat::Mermaid => {
            bs_render::mermaid::render_dependency(&results, &args.target, ctx.no_fence)
        }
        bs_render::OutputFormat::Dot => {
            bs_render::dot::render_dependency(&results, &args.target, ctx.no_fence)
        }
        _ => {
            let mut out = String::new();
            out.push_str(&format!(
                "Co-change partners of {} (strength≥{:.1}, support≥{}):\n",
                args.target, args.min, args.support
            ));
            if results.is_empty() {
                out.push_str("  (none)\n");
            }
            for c in &results {
                let partner = if c.file_a == args.target {
                    &c.file_b
                } else {
                    &c.file_a
                };
                out.push_str(&format!(
                    "  {} ← strength {:.2}, support {}\n",
                    partner, c.strength, c.support
                ));
            }
            out
        }
    };

    emit(ctx, &out);
    Ok(())
}

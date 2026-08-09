use super::{emit, open_store, Context};
use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct HotspotsArgs {
    /// Number of top results
    #[arg(long, default_value = "20")]
    pub top: usize,
}

pub fn run(ctx: &Context, args: &HotspotsArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let stats = store.get_hotspots(args.top)?;

    let out = match ctx.output {
        bs_render::OutputFormat::Json => serde_json::to_string_pretty(&stats).unwrap_or_default(),
        _ => {
            let mut out = String::new();
            out.push_str(&format!(
                "{:<6} {:<8} {:<8}  {}\n",
                "churn", "age_days", "hotspot", "file"
            ));
            out.push_str(&"-".repeat(60));
            out.push('\n');
            for s in &stats {
                out.push_str(&format!(
                    "{:<6} {:<8} {:<8.3}  {}\n",
                    s.churn, s.age_days, s.hotspot, s.path
                ));
            }
            out
        }
    };

    emit(ctx, &out);
    Ok(())
}

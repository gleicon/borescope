use super::{emit, open_store, Context};
use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct AgeArgs {}

pub fn run(ctx: &Context, _args: &AgeArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let stats = store.get_age_view(&ctx.zoom)?;

    let out = match ctx.output {
        bs_render::OutputFormat::Json => serde_json::to_string_pretty(&stats).unwrap_or_default(),
        _ => {
            let mut out = String::new();
            out.push_str(&format!("{:<8}  {}\n", "age_days", "file"));
            out.push_str(&"-".repeat(50));
            out.push('\n');
            for s in &stats {
                out.push_str(&format!("{:<8}  {}\n", s.age_days, s.path));
            }
            out
        }
    };

    emit(ctx, &out);
    Ok(())
}

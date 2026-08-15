use super::{emit, open_store, Context};
use anyhow::Result;
use clap::Args;

#[derive(Args)]
pub struct HotspotsArgs {
    /// Number of top results
    #[arg(long, default_value = "20")]
    pub top: usize,

    /// Include test files (tests/, *_test.rs, *.spec.ts, etc.) in results
    #[arg(long)]
    pub include_tests: bool,
}

pub fn run(ctx: &Context, args: &HotspotsArgs) -> Result<()> {
    let store = open_store(ctx)?;
    let stats = store.get_hotspots(args.top, !args.include_tests)?;

    let out = match ctx.output {
        bs_render::OutputFormat::Json => serde_json::to_string_pretty(&stats).unwrap_or_default(),
        _ => {
            let mut out = String::new();
            // One-line explanation so first-time users immediately understand what they're seeing.
            out.push_str(
                "hotspot = churn × recency  (1.0 = changed constantly and just recently; 0.0 = never touched)\n\n",
            );
            out.push_str(&format!(
                "{:<8} {:<6} {:<7}  {:<14}  {}\n",
                "hotspot", "churn", "age", "heat", "file"
            ));
            out.push_str(&"-".repeat(72));
            out.push('\n');
            for s in &stats {
                let bar = heat_bar(s.hotspot);
                let age = format_age(s.age_days);
                out.push_str(&format!(
                    "{:<8.3} {:<6} {:<7}  {:<14}  {}\n",
                    s.hotspot, s.churn, age, bar, s.path
                ));
            }
            if !args.include_tests {
                out.push_str("\n(test files hidden — pass --include-tests to show them)\n");
            }
            out
        }
    };

    emit(ctx, &out);
    Ok(())
}

fn heat_bar(score: f32) -> &'static str {
    match score {
        s if s >= 0.8 => "🔥 very hot",
        s if s >= 0.6 => "🔥 hot",
        s if s >= 0.4 => "warm",
        s if s >= 0.2 => "mild",
        s if s >= 0.05 => "cool",
        _ => "cold",
    }
}

fn format_age(age_days: u32) -> String {
    if age_days == 0 {
        "today".to_string()
    } else if age_days == 1 {
        "1 day".to_string()
    } else if age_days < 14 {
        format!("{} days", age_days)
    } else if age_days < 60 {
        format!("{} wks", age_days / 7)
    } else if age_days < 730 {
        format!("{} mo", age_days / 30)
    } else {
        format!("{} yr", age_days / 365)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heat_bar_boundaries() {
        assert!(heat_bar(0.9).contains("hot"));
        assert!(heat_bar(0.65).contains("hot"));
        assert_eq!(heat_bar(0.5), "warm");
        assert_eq!(heat_bar(0.3), "mild");
        assert_eq!(heat_bar(0.1), "cool");
        assert_eq!(heat_bar(0.0), "cold");
    }

    #[test]
    fn test_format_age() {
        assert_eq!(format_age(0), "today");
        assert_eq!(format_age(1), "1 day");
        assert_eq!(format_age(10), "10 days");
        assert_eq!(format_age(21), "3 wks");
        assert_eq!(format_age(90), "3 mo");
        assert_eq!(format_age(730), "2 yr");
    }
}

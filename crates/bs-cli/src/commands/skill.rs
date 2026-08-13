use anyhow::Result;
use clap::Args;

const SKILL_CONTENT: &str = include_str!("../../../../skill/SKILL.md");

#[derive(Args)]
pub struct SkillArgs {
    /// Emit raw Markdown (no fencing or wrapping) — same as default, kept for scripting clarity
    #[arg(long)]
    pub raw: bool,
}

/// Print the embedded SKILL.md to stdout.
///
/// Redirect to install on any platform:
///   Claude Code:  borescope skill > ~/.claude/skills/borescope.md
///   Cursor:       borescope skill > .cursor/rules/borescope.md
///   Any agent:    borescope skill | your-agent --system-prompt -
pub fn run(_args: &SkillArgs) -> Result<()> {
    print!("{}", SKILL_CONTENT);
    Ok(())
}

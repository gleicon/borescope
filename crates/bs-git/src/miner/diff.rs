use bs_core::Result;
use std::collections::{HashMap, HashSet};
use super::Miner;

impl Miner {
    /// Mine per-symbol churn by streaming hunk-overlap.
    ///
    /// Streams `git log -p` line by line. Per commit, collects touched hunk ranges only,
    /// then immediately checks each symbol span — never stores all lines in memory.
    /// Memory: O(symbols + hunks_per_commit), not O(commits × lines).
    pub fn mine_symbol_spans(&self, store: &bs_core::Store, file: &str) -> Result<()> {
        let symbols = store.symbols_for_file(file)?;
        if symbols.is_empty() {
            return Ok(());
        }

        let mut span_churn: HashMap<String, u32> =
            symbols.iter().map(|s| (s.id.clone(), 0u32)).collect();
        let spans: Vec<(u32, u32, String)> = symbols
            .iter()
            .map(|s| (s.span.0, s.span.1, s.id.clone()))
            .collect();

        let log_out = self.git(&[
            "log".to_string(),
            "-p".to_string(),
            "--follow".to_string(),
            "--no-merges".to_string(),
            "--format=%H".to_string(),
            "--".to_string(),
            file.to_string(),
        ])?;

        // State machine: per-commit, accumulate touched ranges, flush on next commit header
        let mut commit_ranges: Vec<(u32, u32)> = Vec::new();
        let mut in_commit = false;

        let flush = |ranges: &[(u32, u32)], span_churn: &mut HashMap<String, u32>| {
            if ranges.is_empty() {
                return;
            }
            for (span_start, span_end, id) in &spans {
                let touched = ranges
                    .iter()
                    .any(|&(rs, re)| rs <= *span_end && re >= *span_start);
                if touched {
                    *span_churn.entry(id.clone()).or_default() += 1;
                }
            }
        };

        for line in log_out.lines() {
            if line.len() == 40 && line.chars().all(|c| c.is_ascii_hexdigit()) {
                if in_commit {
                    flush(&commit_ranges, &mut span_churn);
                    commit_ranges.clear();
                }
                in_commit = true;
                continue;
            }
            if !in_commit {
                continue;
            }
            if let Some(hunk) = line.strip_prefix("@@ ") {
                if let Some((start, len)) = parse_hunk_new(hunk) {
                    let end = start + len.saturating_sub(1);
                    commit_ranges.push((start, end));
                }
            }
        }
        if in_commit {
            flush(&commit_ranges, &mut span_churn);
        }

        let max_churn = span_churn.values().copied().max().unwrap_or(1) as f32;
        for sym in &symbols {
            let churn = *span_churn.get(&sym.id).unwrap_or(&0);
            let hotspot = churn as f32 / max_churn.max(1.0);
            store.update_symbol_signals(&sym.id, churn, hotspot)?;
        }

        Ok(())
    }

    /// Returns (file, set_of_touched_new_lines) for each file changed between rev1 and rev2.
    pub fn diff_line_ranges(
        &self,
        rev1: &str,
        rev2: Option<&str>,
    ) -> Result<HashMap<String, HashSet<u32>>> {
        let args: Vec<String> = match rev2 {
            Some(r2) => vec!["diff".into(), "--unified=0".into(), rev1.into(), r2.into()],
            None => vec!["diff".into(), "--unified=0".into(), rev1.into()],
        };
        let out = self.git(&args)?;
        Ok(parse_diff_ranges(&out))
    }

    /// Returns (file, FileDiffRanges) for each file changed between rev1 and rev2.
    /// FileDiffRanges carries both the full touched set and the pure-addition subset,
    /// enabling `+` / `~` polarity classification.
    pub fn diff_line_ranges_full(
        &self,
        rev1: &str,
        rev2: Option<&str>,
    ) -> Result<HashMap<String, FileDiffRanges>> {
        let args: Vec<String> = match rev2 {
            Some(r2) => vec!["diff".into(), "--unified=0".into(), rev1.into(), r2.into()],
            None => vec!["diff".into(), "--unified=0".into(), rev1.into()],
        };
        let out = self.git(&args)?;
        Ok(parse_diff_ranges_full(&out))
    }

    pub fn merge_base(&self, rev1: &str, rev2: &str) -> Result<String> {
        let out = self.git(&["merge-base".to_string(), rev1.to_string(), rev2.to_string()])?;
        Ok(out.trim().to_string())
    }

    pub fn changed_files(&self, rev1: &str, rev2: &str) -> Result<Vec<String>> {
        let out = self.git(&[
            "diff".to_string(),
            "--name-only".to_string(),
            rev1.to_string(),
            rev2.to_string(),
        ])?;
        Ok(out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
    }

    pub fn changed_files_worktree(&self, rev: &str) -> Result<Vec<String>> {
        let out = self.git(&[
            "diff".to_string(),
            "--name-only".to_string(),
            rev.to_string(),
        ])?;
        Ok(out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
    }
}

/// Per-file line coverage from a `git diff --unified=0` output with hunk polarity.
#[derive(Debug, Default)]
pub struct FileDiffRanges {
    /// New-file line numbers from pure-addition hunks (old_len == 0, new_len > 0).
    pub pure_added: HashSet<u32>,
    /// All new-file line numbers from any hunk that has new content (new_len > 0).
    pub all_touched: HashSet<u32>,
}

/// Parse a `git diff --unified=0` output into per-file line ranges with polarity.
/// Hunks with `new_len == 0` (pure deletions) do not contribute any new-file lines.
pub fn parse_diff_ranges_full(diff: &str) -> HashMap<String, FileDiffRanges> {
    let mut result: HashMap<String, FileDiffRanges> = HashMap::new();
    let mut current_file = String::new();

    for line in diff.lines() {
        if let Some(stripped) = line.strip_prefix("+++ b/") {
            current_file = stripped.to_string();
        } else if let Some(stripped) = line.strip_prefix("@@ ") {
            if let (Some((_, old_len)), Some((new_start, new_len))) =
                (parse_hunk_old(stripped), parse_hunk_new(stripped))
            {
                if new_len == 0 {
                    continue;
                }
                let entry = result.entry(current_file.clone()).or_default();
                for l in new_start..(new_start + new_len) {
                    entry.all_touched.insert(l);
                    if old_len == 0 {
                        entry.pure_added.insert(l);
                    }
                }
            }
        }
    }

    result
}

/// Parse a `git diff --unified=0` output into file -> set of touched new lines.
/// Kept for backward compatibility; use `parse_diff_ranges_full` for polarity.
pub fn parse_diff_ranges(diff: &str) -> HashMap<String, HashSet<u32>> {
    parse_diff_ranges_full(diff)
        .into_iter()
        .map(|(k, v)| (k, v.all_touched))
        .collect()
}

fn parse_hunk_new(hunk: &str) -> Option<(u32, u32)> {
    let plus = hunk.find('+')? + 1;
    let rest = &hunk[plus..];
    let end = rest.find([' ', '@']).unwrap_or(rest.len());
    let range = &rest[..end];
    if let Some(comma) = range.find(',') {
        let start: u32 = range[..comma].parse().ok()?;
        let len: u32 = range[comma + 1..].parse().ok()?;
        Some((start, len))
    } else {
        let start: u32 = range.parse().ok()?;
        Some((start, 1))
    }
}

fn parse_hunk_old(hunk: &str) -> Option<(u32, u32)> {
    let rest = hunk.strip_prefix('-')?;
    let end = rest.find([' ', '+']).unwrap_or(rest.len());
    let range = &rest[..end];
    if let Some(comma) = range.find(',') {
        let start: u32 = range[..comma].parse().ok()?;
        let len: u32 = range[comma + 1..].parse().ok()?;
        Some((start, len))
    } else {
        let start: u32 = range.parse().ok()?;
        Some((start, 1))
    }
}

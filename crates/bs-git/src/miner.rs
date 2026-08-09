use bs_core::{store::Store, LangId, Result};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

pub struct Miner {
    repo: PathBuf,
}

impl Miner {
    pub fn new(repo: PathBuf) -> Self {
        Self { repo }
    }

    pub fn mine(&self, store: &Store, full: bool) -> Result<()> {
        let since_sha = if full {
            None
        } else {
            store.get_meta("git_last_sha")?
        };

        let commits = self.list_commits(since_sha.as_deref())?;
        if commits.is_empty() {
            return Ok(());
        }

        // file_path -> (churn, last_commit_ts, last_commit_sha)
        let mut file_churn: HashMap<String, u32> = HashMap::new();
        let mut file_last: HashMap<String, (i64, String)> = HashMap::new();

        // commit_sha -> [file_paths]  — for co-change
        let mut commit_files: Vec<Vec<String>> = Vec::new();

        for commit in &commits {
            let files = self.commit_files(&commit.sha)?;
            // Skip mass-change commits (renames, formatting runs) — they inflate co-change noise
            if files.len() > 50 {
                continue;
            }
            for f in &files {
                *file_churn.entry(f.clone()).or_default() += 1;
                let e = file_last
                    .entry(f.clone())
                    .or_insert((commit.ts, commit.sha.clone()));
                if commit.ts > e.0 {
                    *e = (commit.ts, commit.sha.clone());
                }
            }
            if !files.is_empty() {
                commit_files.push(files);
            }
        }

        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        // max churn for normalization
        let max_churn = file_churn.values().copied().max().unwrap_or(1) as f32;

        for (path, &churn) in &file_churn {
            let (last_ts, last_sha) = file_last
                .get(path)
                .map(|(ts, sha)| (*ts, sha.as_str()))
                .unwrap_or((0, ""));
            let age_days = ((now_ts - last_ts).max(0) / 86400) as u32;
            let hotspot = churn as f32 / max_churn; // simple normalization; complexity multiplier in M1+
            let lang = LangId::from_path(Path::new(path));
            let loc = count_loc_cached(store, path);
            let file_id = store.upsert_file(path, &lang, loc)?;
            store.upsert_git_stat(
                file_id,
                churn,
                age_days,
                Some(last_sha),
                Some(last_ts),
                hotspot,
            )?;
        }

        // co-change matrix
        self.compute_cochange(store, &commit_files, &file_churn)?;

        // record last-seen sha
        if let Some(first) = commits.first() {
            store.set_meta("git_last_sha", &first.sha)?;
        }

        Ok(())
    }

    fn compute_cochange(
        &self,
        store: &Store,
        commit_files: &[Vec<String>],
        file_churn: &HashMap<String, u32>,
    ) -> Result<()> {
        // pair_support: (a,b) where a<b -> count of commits touching both
        let mut pair_support: HashMap<(String, String), u32> = HashMap::new();

        for files in commit_files {
            if files.len() < 2 {
                continue;
            }
            let mut sorted = files.clone();
            sorted.sort();
            sorted.dedup();
            for i in 0..sorted.len() {
                for j in (i + 1)..sorted.len() {
                    let a = sorted[i].clone();
                    let b = sorted[j].clone();
                    *pair_support.entry((a, b)).or_default() += 1;
                }
            }
        }

        for ((a, b), support) in &pair_support {
            let churn_a = file_churn.get(a).copied().unwrap_or(1) as f32;
            let churn_b = file_churn.get(b).copied().unwrap_or(1) as f32;
            let strength = *support as f32 / churn_a.min(churn_b);
            let strength_a_given_b = *support as f32 / churn_b;

            let a_id = store.upsert_file(a, &LangId::from_path(Path::new(a.as_str())), 0)?;
            let b_id = store.upsert_file(b, &LangId::from_path(Path::new(b.as_str())), 0)?;
            store.upsert_cochange(a_id, b_id, *support, strength, strength_a_given_b)?;
        }

        Ok(())
    }

    fn list_commits(&self, since_sha: Option<&str>) -> Result<Vec<CommitMeta>> {
        let mut args = vec![
            "log".to_string(),
            "--format=%H %at %P".to_string(),
            "--no-merges".to_string(),
        ];
        if let Some(sha) = since_sha {
            args.push(format!("{sha}..HEAD"));
        }

        let out = self.git(&args)?;
        let mut commits = Vec::new();
        for line in out.lines() {
            let parts: Vec<&str> = line.splitn(3, ' ').collect();
            if parts.len() < 2 {
                continue;
            }
            let sha = parts[0].to_string();
            let ts: i64 = parts[1].parse().unwrap_or(0);
            let parent_count = if parts.len() > 2 {
                parts[2].split_whitespace().count()
            } else {
                0
            };
            commits.push(CommitMeta {
                sha,
                ts,
                parent_count,
            });
        }
        Ok(commits)
    }

    fn commit_files(&self, sha: &str) -> Result<Vec<String>> {
        let out = self.git(&[
            "diff-tree".to_string(),
            "--no-commit-id".to_string(),
            "-r".to_string(),
            "--name-only".to_string(),
            sha.to_string(),
        ])?;
        Ok(out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

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

        // symbol_id -> commit-hit count
        let mut span_churn: HashMap<String, u32> =
            symbols.iter().map(|s| (s.id.clone(), 0u32)).collect();
        // Sorted spans for fast lookup
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
            // Commit SHA header line
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
        // Flush final commit
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
    ) -> Result<std::collections::HashMap<String, std::collections::HashSet<u32>>> {
        let args: Vec<String> = match rev2 {
            Some(r2) => vec!["diff".into(), "--unified=0".into(), rev1.into(), r2.into()],
            None => vec!["diff".into(), "--unified=0".into(), rev1.into()],
        };
        let out = self.git(&args)?;
        Ok(parse_diff_ranges(&out))
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
        Ok(out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    pub fn changed_files_worktree(&self, rev: &str) -> Result<Vec<String>> {
        let out = self.git(&[
            "diff".to_string(),
            "--name-only".to_string(),
            rev.to_string(),
        ])?;
        Ok(out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    fn git(&self, args: &[String]) -> Result<String> {
        let out = Command::new("git")
            .args(args)
            .current_dir(&self.repo)
            .output()
            .map_err(|e| bs_core::Error::Git(e.to_string()))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(bs_core::Error::Git(stderr.into_owned()));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }
}

struct CommitMeta {
    sha: String,
    ts: i64,
    #[allow(dead_code)]
    parent_count: usize,
}

fn count_loc_cached(store: &Store, path: &str) -> u32 {
    // Return existing LOC if already set; else 0 (updated by extract phase)
    store.file_loc(path).unwrap_or(0)
}

/// Parse "@@ -old_start,old_len +new_start,new_len @@" and return (new_start, new_len).
fn parse_hunk_new(hunk: &str) -> Option<(u32, u32)> {
    // Format after leading "@@ " is "-old +new @@..."
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

/// Parse a `git diff --unified=0` output into file -> set of touched new lines.
pub fn parse_diff_ranges(
    diff: &str,
) -> std::collections::HashMap<String, std::collections::HashSet<u32>> {
    let mut result: std::collections::HashMap<String, std::collections::HashSet<u32>> =
        std::collections::HashMap::new();
    let mut current_file = String::new();

    for line in diff.lines() {
        if let Some(stripped) = line.strip_prefix("+++ b/") {
            current_file = stripped.to_string();
        } else if let Some(stripped) = line.strip_prefix("@@ ") {
            if let Some((start, len)) = parse_hunk_new(stripped) {
                let set = result.entry(current_file.clone()).or_default();
                for l in start..(start + len.max(1)) {
                    set.insert(l);
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use bs_core::Store;
    use std::process::Command as Cmd;
    use tempfile::TempDir;

    fn git_cmd(dir: &std::path::Path, args: &[&str]) {
        let status = Cmd::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .env("GIT_AUTHOR_DATE", "2024-01-01T00:00:00Z")
            .env("GIT_COMMITTER_DATE", "2024-01-01T00:00:00Z")
            .status()
            .expect("git failed");
        assert!(status.success(), "git {:?} failed", args);
    }

    fn write(dir: &std::path::Path, name: &str, content: &str) {
        std::fs::write(dir.join(name), content).unwrap();
    }

    fn make_fixture_repo() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let d = tmp.path();

        git_cmd(d, &["init", "-b", "main"]);
        git_cmd(d, &["config", "user.email", "test@test.com"]);
        git_cmd(d, &["config", "user.name", "Test"]);

        // Commit 1: create two files
        write(d, "a.py", "def foo():\n    pass\n");
        write(d, "b.py", "def bar():\n    pass\n");
        git_cmd(d, &["add", "."]);
        git_cmd(d, &["commit", "-m", "c1"]);

        // Commit 2: change both files → they become co-changed
        write(d, "a.py", "def foo():\n    return 1\n");
        write(d, "b.py", "def bar():\n    return 1\n");
        git_cmd(d, &["add", "."]);
        git_cmd(d, &["commit", "-m", "c2"]);

        // Commit 3: change both again → support=2 for (a.py, b.py)
        write(d, "a.py", "def foo():\n    return 2\n");
        write(d, "b.py", "def bar():\n    return 2\n");
        git_cmd(d, &["add", "."]);
        git_cmd(d, &["commit", "-m", "c3"]);

        // Commits 4-8: change a.py alone 5 more times
        for i in 4..9usize {
            write(d, "a.py", &format!("def foo():\n    return {}\n", i));
            git_cmd(d, &["add", "a.py"]);
            git_cmd(d, &["commit", "-m", &format!("c{}", i)]);
        }

        tmp
    }

    #[test]
    fn test_churn_counts() {
        let tmp = make_fixture_repo();
        let store = Store::open(tmp.path()).unwrap();
        let miner = Miner::new(tmp.path().to_path_buf());
        miner.mine(&store, true).unwrap();

        let stats = store.get_all_file_stats().unwrap();
        let a = stats.iter().find(|s| s.path == "a.py").unwrap();
        let b = stats.iter().find(|s| s.path == "b.py").unwrap();

        // a.py changed in commits c2..c8 → 7 times
        assert_eq!(a.churn, 7, "a.py churn should be 7, got {}", a.churn);
        // b.py changed in c2 and c3 → 2 times
        assert_eq!(b.churn, 2, "b.py churn should be 2, got {}", b.churn);
    }

    #[test]
    fn test_cochange_support() {
        let tmp = make_fixture_repo();
        let store = Store::open(tmp.path()).unwrap();
        let miner = Miner::new(tmp.path().to_path_buf());
        miner.mine(&store, true).unwrap();

        // (a.py, b.py) co-changed in c2 and c3 → support=2
        // Default threshold is support≥5, so get_coupled won't return it
        // Use get_all_cochange with min_support=1
        let cc = store.get_all_cochange(1).unwrap();
        let pair = cc.iter().find(|c| {
            (c.file_a == "a.py" && c.file_b == "b.py") || (c.file_a == "b.py" && c.file_b == "a.py")
        });
        assert!(pair.is_some(), "expected a.py↔b.py co-change pair");
        let pair = pair.unwrap();
        assert_eq!(pair.support, 2, "support should be 2, got {}", pair.support);
    }

    #[test]
    fn test_incremental_mine() {
        let tmp = make_fixture_repo();
        let store = Store::open(tmp.path()).unwrap();
        let miner = Miner::new(tmp.path().to_path_buf());

        // Mine once
        miner.mine(&store, true).unwrap();
        let sha_after_full = store.get_meta("git_last_sha").unwrap();
        assert!(sha_after_full.is_some());

        // Mine again — incremental (no new commits)
        miner.mine(&store, false).unwrap();
        // Stats should be unchanged
        let stats = store.get_all_file_stats().unwrap();
        let a = stats.iter().find(|s| s.path == "a.py").unwrap();
        assert_eq!(a.churn, 7);
    }

    #[test]
    fn test_delete_and_reindex() {
        let tmp = make_fixture_repo();

        let churn_first = {
            let store = Store::open(tmp.path()).unwrap();
            let miner = Miner::new(tmp.path().to_path_buf());
            miner.mine(&store, true).unwrap();
            store
                .get_all_file_stats()
                .unwrap()
                .into_iter()
                .find(|s| s.path == "a.py")
                .unwrap()
                .churn
        };

        // Delete .borescope/ and reindex
        std::fs::remove_dir_all(tmp.path().join(".borescope")).unwrap();

        let churn_second = {
            let store = Store::open(tmp.path()).unwrap();
            let miner = Miner::new(tmp.path().to_path_buf());
            miner.mine(&store, true).unwrap();
            store
                .get_all_file_stats()
                .unwrap()
                .into_iter()
                .find(|s| s.path == "a.py")
                .unwrap()
                .churn
        };

        assert_eq!(
            churn_first, churn_second,
            "delete+reindex must produce same churn"
        );
    }
}

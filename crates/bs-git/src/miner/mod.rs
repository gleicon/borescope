mod diff;

pub use diff::{FileDiffRanges, parse_diff_ranges, parse_diff_ranges_full};

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

        let mut file_churn: HashMap<String, u32> = HashMap::new();
        let mut file_last: HashMap<String, (i64, String)> = HashMap::new();
        let mut commit_files: Vec<Vec<String>> = Vec::new();

        for commit in &commits {
            let files = self.commit_files(&commit.sha)?;
            // Skip mass-change commits (renames, formatting runs) — inflate co-change noise
            if files.len() > 50 {
                continue;
            }
            let source_files: Vec<String> = files
                .into_iter()
                .filter(|f| LangId::from_path(Path::new(f)).is_source())
                .collect();
            for f in &source_files {
                *file_churn.entry(f.clone()).or_default() += 1;
                let e = file_last
                    .entry(f.clone())
                    .or_insert((commit.ts, commit.sha.clone()));
                if commit.ts > e.0 {
                    *e = (commit.ts, commit.sha.clone());
                }
            }
            if !source_files.is_empty() {
                commit_files.push(source_files);
            }
        }

        let now_ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let max_churn = file_churn.values().copied().max().unwrap_or(1) as f32;

        for (path, &churn) in &file_churn {
            let (last_ts, last_sha) = file_last
                .get(path)
                .map(|(ts, sha)| (*ts, sha.as_str()))
                .unwrap_or((0, ""));
            let age_days = ((now_ts - last_ts).max(0) / 86400) as u32;
            // Recency-weighted hotspot: churn × exp(-λ × age_days), λ=0.003 ≈ 230-day half-life
            let recency = (-0.003_f32 * age_days as f32).exp();
            let hotspot = (churn as f32 / max_churn) * recency;
            let lang = LangId::from_path(Path::new(path));
            let loc = count_loc_cached(store, path);
            let file_id = store.upsert_file(path, &lang, loc)?;
            store.upsert_git_stat(file_id, churn, age_days, Some(last_sha), Some(last_ts), hotspot)?;
        }

        self.compute_cochange(store, &commit_files, &file_churn)?;

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
                    *pair_support.entry((sorted[i].clone(), sorted[j].clone())).or_default() += 1;
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
            commits.push(CommitMeta { sha, ts, parent_count });
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
        Ok(out.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
    }

    /// Returns one entry per commit in the last `days` days, with author, date,
    /// subject line, and source files touched.
    pub fn worklog(&self, days: u32) -> Result<Vec<WorklogEntry>> {
        let out = self.git(&[
            "log".to_string(),
            "--format=COMMIT\x1f%an\x1f%aI\x1f%s".to_string(),
            "--name-only".to_string(),
            "--no-merges".to_string(),
            format!("--since={} days ago", days),
        ])?;

        let mut entries: Vec<WorklogEntry> = Vec::new();
        let mut current: Option<(String, String, String)> = None;
        let mut current_files: Vec<String> = Vec::new();

        for line in out.lines() {
            if let Some(rest) = line.strip_prefix("COMMIT\x1f") {
                flush_commit(&mut current, &mut current_files, &mut entries);
                current = parse_commit_line(rest);
            } else {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    current_files.push(trimmed.to_string());
                }
            }
        }
        flush_commit(&mut current, &mut current_files, &mut entries);
        Ok(entries)
    }

    pub(super) fn git(&self, args: &[String]) -> Result<String> {
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

fn parse_commit_line(rest: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = rest.splitn(3, '\x1f').collect();
    if parts.len() != 3 {
        return None;
    }
    let author = parts[0].trim().to_string();
    let date = parts[1].get(..10).unwrap_or(parts[1]).to_string();
    let subject = parts[2].trim().to_string();
    Some((author, date, subject))
}

fn flush_commit(
    current: &mut Option<(String, String, String)>,
    files: &mut Vec<String>,
    entries: &mut Vec<WorklogEntry>,
) {
    if let Some((author, date, subject)) = current.take() {
        let source_files: Vec<String> = files
            .drain(..)
            .filter(|f| LangId::from_path(Path::new(f)).is_source())
            .collect();
        if !source_files.is_empty() && source_files.len() <= 50 {
            entries.push(WorklogEntry { author, date, subject, files: source_files });
        }
    } else {
        files.clear();
    }
}

struct CommitMeta {
    sha: String,
    ts: i64,
    #[allow(dead_code)]
    parent_count: usize,
}

#[derive(Debug, Clone)]
pub struct WorklogEntry {
    pub author: String,
    pub date: String,
    pub subject: String,
    pub files: Vec<String>,
}

fn count_loc_cached(store: &Store, path: &str) -> u32 {
    store.file_loc(path).unwrap_or(0)
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
        write(d, "a.py", "def foo():\n    pass\n");
        write(d, "b.py", "def bar():\n    pass\n");
        git_cmd(d, &["add", "."]);
        git_cmd(d, &["commit", "-m", "c1"]);
        write(d, "a.py", "def foo():\n    return 1\n");
        write(d, "b.py", "def bar():\n    return 1\n");
        git_cmd(d, &["add", "."]);
        git_cmd(d, &["commit", "-m", "c2"]);
        write(d, "a.py", "def foo():\n    return 2\n");
        write(d, "b.py", "def bar():\n    return 2\n");
        git_cmd(d, &["add", "."]);
        git_cmd(d, &["commit", "-m", "c3"]);
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
        assert_eq!(a.churn, 7, "a.py churn should be 7, got {}", a.churn);
        assert_eq!(b.churn, 2, "b.py churn should be 2, got {}", b.churn);
    }

    #[test]
    fn test_cochange_support() {
        let tmp = make_fixture_repo();
        let store = Store::open(tmp.path()).unwrap();
        let miner = Miner::new(tmp.path().to_path_buf());
        miner.mine(&store, true).unwrap();
        let cc = store.get_all_cochange(1).unwrap();
        let pair = cc.iter().find(|c| {
            (c.file_a == "a.py" && c.file_b == "b.py") || (c.file_a == "b.py" && c.file_b == "a.py")
        });
        assert!(pair.is_some(), "expected a.py↔b.py co-change pair");
        assert_eq!(pair.unwrap().support, 2, "support should be 2");
    }

    #[test]
    fn test_incremental_mine() {
        let tmp = make_fixture_repo();
        let store = Store::open(tmp.path()).unwrap();
        let miner = Miner::new(tmp.path().to_path_buf());
        miner.mine(&store, true).unwrap();
        let sha_after_full = store.get_meta("git_last_sha").unwrap();
        assert!(sha_after_full.is_some());
        miner.mine(&store, false).unwrap();
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
            store.get_all_file_stats().unwrap().into_iter()
                .find(|s| s.path == "a.py").unwrap().churn
        };
        std::fs::remove_dir_all(tmp.path().join(".borescope")).unwrap();
        let churn_second = {
            let store = Store::open(tmp.path()).unwrap();
            let miner = Miner::new(tmp.path().to_path_buf());
            miner.mine(&store, true).unwrap();
            store.get_all_file_stats().unwrap().into_iter()
                .find(|s| s.path == "a.py").unwrap().churn
        };
        assert_eq!(churn_first, churn_second, "delete+reindex must produce same churn");
    }
}

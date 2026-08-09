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
            if commit.parent_count > 50 {
                // skip mass commits (renames/formatting runs)
                continue;
            }
            let files = self.commit_files(&commit.sha)?;
            for f in &files {
                *file_churn.entry(f.clone()).or_default() += 1;
                let e = file_last.entry(f.clone()).or_insert((commit.ts, commit.sha.clone()));
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
            store.upsert_git_stat(file_id, churn, age_days, Some(last_sha), Some(last_ts), hotspot)?;
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
            if *support < 5 {
                continue;
            }
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
        Ok(out
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect())
    }

    pub fn merge_base(&self, rev1: &str, rev2: &str) -> Result<String> {
        let out = self.git(&[
            "merge-base".to_string(),
            rev1.to_string(),
            rev2.to_string(),
        ])?;
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
    parent_count: usize,
}

fn count_loc_cached(store: &Store, path: &str) -> u32 {
    // Return existing LOC if already set; else 0 (updated by extract phase)
    store.file_loc(path).unwrap_or(0)
}

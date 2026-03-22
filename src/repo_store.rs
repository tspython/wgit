use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::Context;

const MAX_RECENT_REPOS: usize = 12;

pub fn recent_repos() -> anyhow::Result<Vec<PathBuf>> {
    let storage = storage_path()?;
    read_recent_repos_from(&storage)
}

pub fn remember_repo(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let repo = fs::canonicalize(path)
        .with_context(|| format!("failed to canonicalize repo path {}", path.display()))?;
    let storage = storage_path()?;

    let mut repos = read_recent_repos_from(&storage)?;
    repos.retain(|existing| existing != &repo);
    repos.insert(0, repo);
    repos.truncate(MAX_RECENT_REPOS);
    write_recent_repos_to(&storage, &repos)?;
    Ok(repos)
}

fn storage_path() -> anyhow::Result<PathBuf> {
    let home = env::var_os("HOME").context("HOME environment variable not set")?;
    Ok(PathBuf::from(home).join(".wgit").join("recent_repos.txt"))
}

fn read_recent_repos_from(path: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };

    let mut repos = Vec::new();
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let repo = PathBuf::from(trimmed);
        if !repos.iter().any(|existing| existing == &repo) {
            repos.push(repo);
        }
    }
    repos.truncate(MAX_RECENT_REPOS);
    Ok(repos)
}

fn write_recent_repos_to(path: &Path, repos: &[PathBuf]) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("missing parent directory for {}", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("failed to create {}", parent.display()))?;

    let body = repos
        .iter()
        .map(|repo| repo.display().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let body = if body.is_empty() {
        body
    } else {
        format!("{body}\n")
    };

    fs::write(path, body).with_context(|| format!("failed to write {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{read_recent_repos_from, write_recent_repos_to};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_file(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("wgit-{name}-{unique}.txt"))
    }

    #[test]
    fn deduplicates_and_skips_blank_lines() {
        let path = temp_file("recent-repos-read");
        fs::write(&path, "/tmp/repo-a\n\n/tmp/repo-a\n/tmp/repo-b\n").expect("write fixture");

        let repos = read_recent_repos_from(&path).expect("read recent repos");
        assert_eq!(
            repos,
            vec![PathBuf::from("/tmp/repo-a"), PathBuf::from("/tmp/repo-b")]
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn writes_one_repo_per_line() {
        let path = temp_file("recent-repos-write");
        let repos = vec![PathBuf::from("/tmp/repo-a"), PathBuf::from("/tmp/repo-b")];

        write_recent_repos_to(&path, &repos).expect("write recent repos");
        let body = fs::read_to_string(&path).expect("read written file");
        assert_eq!(body, "/tmp/repo-a\n/tmp/repo-b\n");

        let _ = fs::remove_file(path);
    }
}

mod app;
mod git_model;
mod models;
mod render;
mod repo_store;
mod theme;

use std::{env, ffi::OsString, path::PathBuf};

use anyhow::Context;
use git_model::GitModel;

fn main() {
    env_logger::init();

    let git = match open_startup_repo() {
        Ok(git) => git,
        Err(err) => {
            eprintln!("Failed to open git repository: {err}");
            std::process::exit(1);
        }
    };

    if let Err(err) = app::run(git) {
        eprintln!("Application error: {err}");
        std::process::exit(1);
    }
}

fn open_startup_repo() -> anyhow::Result<GitModel> {
    if let Some(path) = repo_arg() {
        return open_and_remember(path);
    }

    match GitModel::open() {
        Ok(git) => {
            let _ = repo_store::remember_repo(git.repo_root());
            Ok(git)
        }
        Err(current_dir_err) => {
            let recent = repo_store::recent_repos().unwrap_or_default();
            for path in recent {
                if let Ok(git) = open_and_remember(path.clone()) {
                    eprintln!(
                        "Opened recent repository {} after current directory lookup failed: {}",
                        git.repo_root().display(),
                        current_dir_err
                    );
                    return Ok(git);
                }
            }

            Err(current_dir_err)
                .context("not in a git repository and no recent repository could be reopened")
        }
    }
}

fn repo_arg() -> Option<PathBuf> {
    let mut args = env::args_os().skip(1);
    match args.next() {
        Some(flag) if flag == OsString::from("--repo") => args.next().map(PathBuf::from),
        Some(path) => Some(PathBuf::from(path)),
        None => None,
    }
}

fn open_and_remember(path: impl Into<PathBuf>) -> anyhow::Result<GitModel> {
    let path = path.into();
    let git = GitModel::open_at(&path)
        .with_context(|| format!("failed to open repository at {}", path.display()))?;
    let _ = repo_store::remember_repo(git.repo_root());
    Ok(git)
}

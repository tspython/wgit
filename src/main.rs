mod app;
mod git_model;
mod icon;
mod models;
mod render;
mod repo_store;
#[allow(dead_code)]
mod theme;

use std::{env, ffi::OsString, path::PathBuf};

use anyhow::Context;
use git_model::GitModel;

fn main() {
    env_logger::init();

    if let Err(err) = apply_theme_selection() {
        eprintln!("Theme selection failed: {err}");
    }

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

/// Resolve the active theme from `--theme NAME`, `--theme-file PATH`,
/// or `WGIT_THEME` env var. Falls back to the default `Midnight`.
/// Bundled names: `midnight`, `gruvbox`, `vercel`, `dracula`. Anything
/// else is treated as a path to a theme YAML file.
fn apply_theme_selection() -> anyhow::Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let mut theme_arg: Option<String> = None;
    let mut theme_file: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--theme" if i + 1 < args.len() => {
                theme_arg = Some(args.remove(i + 1));
                args.remove(i);
            }
            "--theme-file" if i + 1 < args.len() => {
                theme_file = Some(PathBuf::from(args.remove(i + 1)));
                args.remove(i);
            }
            _ => i += 1,
        }
    }
    if theme_arg.is_none() {
        theme_arg = env::var("WGIT_THEME").ok();
    }

    if let Some(path) = theme_file {
        let palette = theme::load_yaml_file(&path)
            .map_err(|e| anyhow::anyhow!("load theme file {}: {}", path.display(), e))?;
        theme::set_palette(palette);
        return Ok(());
    }

    if let Some(name) = theme_arg {
        if let Some(p) = theme::bundled(&name) {
            theme::set_palette(p);
            return Ok(());
        }
        // Treat as filesystem path if it isn't a known bundled name
        let path = PathBuf::from(&name);
        if path.exists() {
            let palette = theme::load_yaml_file(&path)
                .map_err(|e| anyhow::anyhow!("load theme {}: {}", path.display(), e))?;
            theme::set_palette(palette);
            return Ok(());
        }
        anyhow::bail!(
            "unknown theme {:?}: bundled themes are {:?} or pass a path to a theme YAML",
            name,
            theme::bundled_names()
        );
    }

    Ok(())
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
    while let Some(arg) = args.next() {
        if arg == OsString::from("--theme") || arg == OsString::from("--theme-file") {
            // Skip the value that follows the flag.
            args.next();
            continue;
        }
        if arg == OsString::from("--repo") {
            return args.next().map(PathBuf::from);
        }
        return Some(PathBuf::from(arg));
    }
    None
}

fn open_and_remember(path: impl Into<PathBuf>) -> anyhow::Result<GitModel> {
    let path = path.into();
    let git = GitModel::open_at(&path)
        .with_context(|| format!("failed to open repository at {}", path.display()))?;
    let _ = repo_store::remember_repo(git.repo_root());
    Ok(git)
}

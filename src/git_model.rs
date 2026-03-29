use std::{env, path::Path, path::PathBuf, process::Command};

use anyhow::Context;
use tree_sitter::{Language, Node, Parser};

use crate::models::{ColorSpan, DiffLineNumber, DocLine, Document, GitViewMeta, LineStyle};

#[derive(Clone, Debug, PartialEq, Eq)]
struct GitEntry {
    xy: String,
    path: String,
}

#[derive(Clone, Copy, Debug)]
enum SyntaxLang {
    Rust,
    Toml,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StatusSectionKind {
    Staged,
    Unstaged,
    Untracked,
}

impl StatusSectionKind {
    fn title(self) -> &'static str {
        match self {
            Self::Staged => "STAGED",
            Self::Unstaged => "UNSTAGED",
            Self::Untracked => "UNTRACKED",
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Staged => 0,
            Self::Unstaged => 1,
            Self::Untracked => 2,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusSectionSummary {
    pub kind: StatusSectionKind,
    pub count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupedStatusSection {
    pub kind: StatusSectionKind,
    pub title: &'static str,
    pub start_line: usize,
    pub item_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupedGitViewMeta {
    pub files_start_line: usize,
    pub files_count: usize,
    pub sections: Vec<GroupedStatusSection>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BranchTrackingStatus {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

#[derive(Clone, Copy, Debug)]
enum GitCommand<'a> {
    CurrentBranch,
    StatusPorcelainV2Branch,
    StatusPorcelainV1,
    Diff {
        path: &'a str,
        cached: bool,
    },
    Add {
        path: &'a str,
    },
    RestoreStaged {
        path: &'a str,
    },
    ResetPath {
        path: &'a str,
    },
    RestorePath {
        path: &'a str,
        source: Option<&'a str>,
        staged: bool,
        worktree: bool,
    },
    CleanPath {
        path: &'a str,
    },
    StageAll,
    UnstageAll,
    Commit {
        message: &'a str,
        allow_empty: bool,
    },
    Fetch {
        remote: Option<&'a str>,
    },
    Pull {
        remote: Option<&'a str>,
        branch: Option<&'a str>,
        rebase: bool,
    },
    Push {
        remote: Option<&'a str>,
        branch: Option<&'a str>,
        set_upstream: bool,
    },
    ListBranches,
    CheckoutBranch {
        name: &'a str,
    },
}

impl<'a> GitCommand<'a> {
    fn description(self) -> String {
        match self {
            Self::CurrentBranch => String::from("git rev-parse --abbrev-ref HEAD"),
            Self::StatusPorcelainV2Branch => String::from("git status --porcelain=v2 --branch"),
            Self::StatusPorcelainV1 => String::from("git status --porcelain=v1"),
            Self::Diff { path, cached } => {
                if cached {
                    format!("git diff --cached -- {}", path)
                } else {
                    format!("git diff -- {}", path)
                }
            }
            Self::Add { path } => format!("git add -- {}", path),
            Self::RestoreStaged { path } => format!("git restore --staged -- {}", path),
            Self::ResetPath { path } => format!("git reset HEAD -- {}", path),
            Self::RestorePath {
                path,
                source,
                staged,
                worktree,
            } => {
                let mut desc = String::from("git restore");
                if let Some(source) = source {
                    desc.push_str(" --source=");
                    desc.push_str(source);
                }
                if staged {
                    desc.push_str(" --staged");
                }
                if worktree {
                    desc.push_str(" --worktree");
                }
                desc.push_str(" -- ");
                desc.push_str(path);
                desc
            }
            Self::CleanPath { path } => format!("git clean -f -- {}", path),
            Self::StageAll => String::from("git add --all -- ."),
            Self::UnstageAll => String::from("git restore --staged -- ."),
            Self::Commit {
                message,
                allow_empty,
            } => {
                let mut desc = format!("git commit --message {}", compact_preview(message, 48));
                if allow_empty {
                    desc.push_str(" --allow-empty");
                }
                desc
            }
            Self::Fetch { remote } => match remote {
                Some(remote) => format!("git fetch {}", remote),
                None => String::from("git fetch"),
            },
            Self::Pull {
                remote,
                branch,
                rebase,
            } => {
                let mut desc = String::from("git pull");
                if rebase {
                    desc.push_str(" --rebase");
                }
                if let Some(remote) = remote {
                    desc.push(' ');
                    desc.push_str(remote);
                }
                if let Some(branch) = branch {
                    desc.push(' ');
                    desc.push_str(branch);
                }
                desc
            }
            Self::Push {
                remote,
                branch,
                set_upstream,
            } => {
                let mut desc = String::from("git push");
                if set_upstream {
                    desc.push_str(" -u");
                }
                if let Some(remote) = remote {
                    desc.push(' ');
                    desc.push_str(remote);
                }
                if let Some(branch) = branch {
                    desc.push(' ');
                    desc.push_str(branch);
                }
                desc
            }
            Self::ListBranches => String::from("git branch --list"),
            Self::CheckoutBranch { name } => format!("git checkout {}", name),
        }
    }

    fn configure(self, command: &mut Command) {
        match self {
            Self::CurrentBranch => {
                command.args(["rev-parse", "--abbrev-ref", "HEAD"]);
            }
            Self::StatusPorcelainV2Branch => {
                command.args(["status", "--porcelain=v2", "--branch"]);
            }
            Self::StatusPorcelainV1 => {
                command.args(["status", "--porcelain=v1"]);
            }
            Self::Diff { path, cached } => {
                command.arg("diff");
                if cached {
                    command.arg("--cached");
                }
                command.args(["--", path]);
            }
            Self::Add { path } => {
                command.args(["add", "--", path]);
            }
            Self::RestoreStaged { path } => {
                command.args(["restore", "--staged", "--", path]);
            }
            Self::ResetPath { path } => {
                command.args(["reset", "HEAD", "--", path]);
            }
            Self::RestorePath {
                path,
                source,
                staged,
                worktree,
            } => {
                command.arg("restore");
                if let Some(source) = source {
                    command.arg(format!("--source={source}"));
                }
                if staged {
                    command.arg("--staged");
                }
                if worktree {
                    command.arg("--worktree");
                }
                command.args(["--", path]);
            }
            Self::CleanPath { path } => {
                command.args(["clean", "-f", "--", path]);
            }
            Self::StageAll => {
                command.args(["add", "--all", "--", "."]);
            }
            Self::UnstageAll => {
                command.args(["restore", "--staged", "--", "."]);
            }
            Self::Commit {
                message,
                allow_empty,
            } => {
                command.arg("commit");
                if allow_empty {
                    command.arg("--allow-empty");
                }
                command.args(["--message", message]);
            }
            Self::Fetch { remote } => {
                command.arg("fetch");
                if let Some(remote) = remote {
                    command.arg(remote);
                }
            }
            Self::Pull {
                remote,
                branch,
                rebase,
            } => {
                command.arg("pull");
                if rebase {
                    command.arg("--rebase");
                }
                if let Some(remote) = remote {
                    command.arg(remote);
                }
                if let Some(branch) = branch {
                    command.arg(branch);
                }
            }
            Self::Push {
                remote,
                branch,
                set_upstream,
            } => {
                command.arg("push");
                if set_upstream {
                    command.arg("-u");
                }
                if let Some(remote) = remote {
                    command.arg(remote);
                }
                if let Some(branch) = branch {
                    command.arg(branch);
                }
            }
            Self::ListBranches => {
                command.args(["branch", "--list"]);
            }
            Self::CheckoutBranch { name } => {
                command.args(["checkout", name]);
            }
        }
    }
}

pub struct GitModel {
    repo_root: PathBuf,
    branch: String,
    tracking: BranchTrackingStatus,
    entries: Vec<GitEntry>,
    selected: usize,
    diff: String,
    ts_parser: Parser,
}

impl GitModel {
    pub fn open() -> anyhow::Result<Self> {
        Self::open_at(env::current_dir()?)
    }

    pub fn open_at(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let start = path.as_ref();
        let repo = gix::discover(start).context("not inside a git repository")?;
        let repo_root = repo
            .work_dir()
            .unwrap_or_else(|| repo.git_dir())
            .to_path_buf();

        let mut s = Self {
            repo_root,
            branch: String::new(),
            tracking: BranchTrackingStatus::default(),
            entries: Vec::new(),
            selected: 0,
            diff: String::new(),
            ts_parser: Parser::new(),
        };

        s.refresh()?;
        Ok(s)
    }

    pub fn branch(&self) -> &str {
        &self.branch
    }

    pub fn entries_len(&self) -> usize {
        self.entries.len()
    }

    pub fn selected_index(&self) -> usize {
        self.selected
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn tracking(&self) -> &BranchTrackingStatus {
        &self.tracking
    }

    pub fn commit(&mut self, message: &str) -> anyhow::Result<()> {
        let message = message.trim();
        if message.is_empty() {
            anyhow::bail!("commit message cannot be empty");
        }

        self.run_git(GitCommand::Commit {
            message,
            allow_empty: false,
        })?;
        self.refresh()
    }

    pub fn stage_all(&mut self) -> anyhow::Result<()> {
        self.run_git(GitCommand::StageAll)?;
        self.refresh()
    }

    pub fn fetch(&mut self, remote: Option<&str>) -> anyhow::Result<()> {
        self.run_git(GitCommand::Fetch { remote })?;
        self.refresh()
    }

    pub fn pull(
        &mut self,
        remote: Option<&str>,
        branch: Option<&str>,
        rebase: bool,
    ) -> anyhow::Result<()> {
        if remote.is_some() ^ branch.is_some() {
            anyhow::bail!("pull requires both remote and branch when one is provided");
        }

        self.run_git(GitCommand::Pull {
            remote,
            branch,
            rebase,
        })?;
        self.refresh()
    }

    pub fn push(
        &mut self,
        remote: Option<&str>,
        branch: Option<&str>,
        set_upstream: bool,
    ) -> anyhow::Result<()> {
        if remote.is_some() ^ branch.is_some() {
            anyhow::bail!("push requires both remote and branch when one is provided");
        }

        self.run_git(GitCommand::Push {
            remote,
            branch,
            set_upstream,
        })?;
        self.refresh()
    }

    pub fn list_branches(&self) -> anyhow::Result<Vec<String>> {
        let output = self.run_git(GitCommand::ListBranches)?;
        let mut branches = Vec::new();
        for line in output.lines() {
            let name = line.trim().trim_start_matches("* ").to_string();
            if !name.is_empty() {
                branches.push(name);
            }
        }
        Ok(branches)
    }

    pub fn checkout_branch(&mut self, name: &str) -> anyhow::Result<()> {
        self.run_git(GitCommand::CheckoutBranch { name })?;
        self.refresh()
    }

    pub fn unstage_all(&mut self) -> anyhow::Result<()> {
        self.run_git(GitCommand::UnstageAll)?;
        self.refresh()
    }

    pub fn discard_selected(&mut self) -> anyhow::Result<()> {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Ok(());
        };

        if matches!(
            classify_status_xy(&entry.xy).first(),
            Some(StatusSectionKind::Untracked)
        ) {
            self.run_git(GitCommand::CleanPath { path: &entry.path })?;
        } else {
            self.run_git(GitCommand::RestorePath {
                path: &entry.path,
                source: Some("HEAD"),
                staged: true,
                worktree: true,
            })?;
        }

        self.refresh()
    }

    fn run_git(&self, command: GitCommand<'_>) -> anyhow::Result<String> {
        let description = command.description();
        let mut git = Command::new("git");
        git.arg("-C").arg(&self.repo_root);
        command.configure(&mut git);

        let out = git.output().with_context(|| {
            format!(
                "failed to spawn {} in {}",
                description,
                self.repo_root.display()
            )
        })?;

        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            let stdout = String::from_utf8_lossy(&out.stdout);
            let detail = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            let exit = out.status.code().map_or_else(
                || String::from("terminated by signal"),
                |code| code.to_string(),
            );
            anyhow::bail!(
                "{} failed in {} with exit {}: {}",
                description,
                self.repo_root.display(),
                exit,
                detail
            );
        }

        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    fn current_branch(&self) -> anyhow::Result<String> {
        Ok(self.run_git(GitCommand::CurrentBranch)?.trim().to_string())
    }

    fn current_tracking(&self, branch: &str) -> anyhow::Result<BranchTrackingStatus> {
        let snapshot = self.run_git(GitCommand::StatusPorcelainV2Branch)?;
        let parsed = parse_branch_tracking_snapshot(&snapshot);
        Ok(BranchTrackingStatus {
            branch: branch.to_string(),
            upstream: parsed.upstream,
            ahead: parsed.ahead,
            behind: parsed.behind,
        })
    }

    fn current_status_entries(&self) -> anyhow::Result<Vec<GitEntry>> {
        let status = self.run_git(GitCommand::StatusPorcelainV1)?;
        Ok(parse_porcelain_status(&status))
    }

    #[allow(dead_code)]
    pub fn summarize_status_text(status: &str) -> Vec<StatusSectionSummary> {
        summarize_porcelain_status(status)
    }

    /// Build the file list document (left pane) and the diff document (right pane) separately.
    pub fn build_split_documents(
        &mut self,
    ) -> anyhow::Result<(Document, GroupedGitViewMeta, Document)> {
        use crate::theme;

        // ── File list document (left pane) ───────────────────
        let mut file_lines: Vec<DocLine> = Vec::new();

        let files_start_line = file_lines.len();
        let selected_entry = self.entries.get(self.selected);
        let grouped = grouped_entries(&self.entries);
        let mut sections = Vec::new();

        for kind in [
            StatusSectionKind::Staged,
            StatusSectionKind::Unstaged,
            StatusSectionKind::Untracked,
        ] {
            let start_line = file_lines.len();
            let rows = &grouped[kind.index()];

            let section_style = match kind {
                StatusSectionKind::Staged => LineStyle::SectionStaged,
                StatusSectionKind::Unstaged => LineStyle::SectionUnstaged,
                StatusSectionKind::Untracked => LineStyle::SectionUntracked,
            };
            let header_text = format!(" {} ({})", kind.title(), rows.len());
            file_lines.push(DocLine::new(header_text, section_style));

            if rows.is_empty() {
                file_lines.push(DocLine::new("   No files", LineStyle::Dim));
            } else {
                for entry in rows.iter() {
                    let is_selected = selected_entry == Some(*entry);
                    let badge = theme::badge_char_for_status(&entry.xy);
                    let text = format!("  {}  {}", badge, entry.path);
                    let badge_color = theme::badge_color_for_status(&entry.xy);
                    let spans = vec![ColorSpan {
                        start_col: 2,
                        end_col: 3,
                        color: badge_color,
                    }];
                    file_lines.push(
                        DocLine::new(
                            text,
                            if is_selected {
                                LineStyle::Selected
                            } else {
                                LineStyle::Normal
                            },
                        )
                        .with_spans(spans),
                    );
                }
            }

            sections.push(GroupedStatusSection {
                kind,
                title: kind.title(),
                start_line,
                item_count: rows.len(),
            });
        }

        let files_count = file_lines.len() - files_start_line;

        let file_doc = Document::from_lines(file_lines);
        let file_meta = GroupedGitViewMeta {
            files_start_line,
            files_count,
            sections,
        };

        // ── Diff document (right pane) ───────────────────────
        let diff_doc = self.build_diff_document()?;

        Ok((file_doc, file_meta, diff_doc))
    }

    /// Build just the diff document with line numbers.
    fn build_diff_document(&mut self) -> anyhow::Result<Document> {
        let mut lines: Vec<DocLine> = Vec::new();

        // Diff file header
        let diff_title = self
            .entries
            .get(self.selected)
            .map(|e| format!(" {}", e.path))
            .unwrap_or_else(|| String::from(" No file selected"));
        lines.push(DocLine::new(diff_title, LineStyle::DiffFileHeader));

        // Parse diff lines with line numbers
        let raw_lines: Vec<String> = self.diff.lines().map(|l| l.to_string()).collect();
        let diff_texts: Vec<String> = raw_lines.iter().map(|l| normalize_for_display(l)).collect();
        let diff_styles: Vec<LineStyle> = raw_lines.iter().map(|l| style_for_diff_line(l)).collect();
        let diff_spans = self.syntax_spans_for_diff_lines(&diff_texts, &diff_styles)?;

        // Track line numbers through hunk headers
        let mut old_line: u32 = 0;
        let mut new_line: u32 = 0;

        for (i, ((text, style), spans)) in diff_texts
            .into_iter()
            .zip(diff_styles.into_iter())
            .zip(diff_spans.into_iter())
            .enumerate()
        {
            let raw = &raw_lines[i];
            let ln = match style {
                LineStyle::DiffHunk => {
                    // Parse @@ -old,count +new,count @@ ...
                    if let Some((o, n)) = parse_hunk_header(raw) {
                        old_line = o;
                        new_line = n;
                    }
                    None
                }
                LineStyle::DiffAdd => {
                    let ln = DiffLineNumber {
                        old: None,
                        new: Some(new_line),
                    };
                    new_line += 1;
                    Some(ln)
                }
                LineStyle::DiffRemove => {
                    let ln = DiffLineNumber {
                        old: Some(old_line),
                        new: None,
                    };
                    old_line += 1;
                    Some(ln)
                }
                LineStyle::Normal => {
                    // Context line — has both line numbers
                    let ln = DiffLineNumber {
                        old: Some(old_line),
                        new: Some(new_line),
                    };
                    old_line += 1;
                    new_line += 1;
                    Some(ln)
                }
                _ => None, // metadata lines
            };

            let mut doc_line = DocLine::new(text, style).with_spans(spans);
            doc_line.line_number = ln;
            lines.push(doc_line);
        }

        Ok(Document::from_lines(lines))
    }

    /// Legacy: build a single combined document (kept for backwards compatibility).
    pub fn build_grouped_document(&mut self) -> anyhow::Result<(Document, GroupedGitViewMeta)> {
        let (file_doc, meta, _diff_doc) = self.build_split_documents()?;
        Ok((file_doc, meta))
    }

    fn diff_for_path(&self, path: &str, cached: bool) -> anyhow::Result<String> {
        self.run_git(GitCommand::Diff { path, cached })
    }

    pub fn refresh(&mut self) -> anyhow::Result<()> {
        self.branch = self.current_branch()?;
        self.tracking = self.current_tracking(&self.branch)?;
        self.entries = self.current_status_entries()?;

        if self.entries.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
        }

        self.refresh_diff()
    }

    fn refresh_diff(&mut self) -> anyhow::Result<()> {
        let Some(path) = self.entries.get(self.selected).map(|e| e.path.clone()) else {
            self.diff = String::from("Working tree clean. No changed files.");
            return Ok(());
        };

        let unstaged = self.diff_for_path(&path, false)?;
        let staged = self.diff_for_path(&path, true)?;

        let mut out = String::new();
        if !unstaged.trim().is_empty() {
            out.push_str("# Unstaged\n");
            out.push_str(&unstaged);
            if !unstaged.ends_with('\n') {
                out.push('\n');
            }
        }
        if !staged.trim().is_empty() {
            out.push_str("# Staged\n");
            out.push_str(&staged);
            if !staged.ends_with('\n') {
                out.push('\n');
            }
        }
        if out.trim().is_empty() {
            out.push_str("No diff output for selected file.");
        }
        self.diff = out;
        Ok(())
    }

    pub fn move_selection(&mut self, delta: isize) -> anyhow::Result<()> {
        if self.entries.is_empty() {
            return Ok(());
        }
        let max = (self.entries.len() - 1) as isize;
        let next = (self.selected as isize + delta).clamp(0, max) as usize;
        if next != self.selected {
            self.selected = next;
            self.refresh_diff()?;
        }
        Ok(())
    }

    pub fn stage_selected(&mut self) -> anyhow::Result<()> {
        let Some(path) = self.entries.get(self.selected).map(|e| e.path.clone()) else {
            return Ok(());
        };
        self.run_git(GitCommand::Add { path: &path })?;
        self.refresh()
    }

    pub fn unstage_selected(&mut self) -> anyhow::Result<()> {
        let Some(path) = self.entries.get(self.selected).map(|e| e.path.clone()) else {
            return Ok(());
        };

        if let Err(restore_err) = self.run_git(GitCommand::RestoreStaged { path: &path }) {
            self.run_git(GitCommand::ResetPath { path: &path })
                .with_context(|| {
                    format!(
                        "failed to unstage '{}' after git restore --staged failed: {}",
                        path, restore_err
                    )
                })?;
        }

        self.refresh()
    }

    pub fn select_file_index(&mut self, idx: usize) -> anyhow::Result<()> {
        if idx < self.entries.len() && idx != self.selected {
            self.selected = idx;
            self.refresh_diff()?;
        }
        Ok(())
    }

    fn selected_language(&self) -> Option<SyntaxLang> {
        let path = self.entries.get(self.selected)?.path.to_ascii_lowercase();
        if path.ends_with(".rs") {
            Some(SyntaxLang::Rust)
        } else if path.ends_with(".toml") {
            Some(SyntaxLang::Toml)
        } else {
            None
        }
    }

    fn syntax_spans_for_diff_lines(
        &mut self,
        diff_lines: &[String],
        diff_styles: &[LineStyle],
    ) -> anyhow::Result<Vec<Vec<ColorSpan>>> {
        let mut spans_per_line = vec![Vec::<ColorSpan>::new(); diff_lines.len()];

        let Some(lang) = self.selected_language() else {
            return Ok(spans_per_line);
        };

        let ts_lang = ts_language(lang);
        self.ts_parser
            .set_language(&ts_lang)
            .map_err(|_| anyhow::anyhow!("failed setting tree-sitter language"))?;

        let mut idx = 0usize;
        while idx < diff_lines.len() {
            if !diff_lines[idx].starts_with("@@") {
                idx += 1;
                continue;
            }

            let hunk_start = idx + 1;
            let mut hunk_end = hunk_start;
            while hunk_end < diff_lines.len() {
                if diff_lines[hunk_end].starts_with("@@") {
                    break;
                }
                hunk_end += 1;
            }

            let mut source = String::new();
            let mut maps = Vec::<(usize, usize, usize, String)>::new();

            for i in hunk_start..hunk_end {
                let style = diff_styles[i];
                if !matches!(
                    style,
                    LineStyle::DiffAdd | LineStyle::DiffRemove | LineStyle::Normal
                ) {
                    continue;
                }

                let line = &diff_lines[i];
                if line.is_empty() {
                    continue;
                }

                let prefix = line.chars().next().unwrap_or(' ');
                if !matches!(prefix, '+' | '-' | ' ') {
                    continue;
                }

                let code = normalize_for_display(&line[1..]);
                let start = source.len();
                source.push_str(&code);
                let end = source.len();
                source.push('\n');

                maps.push((i, start, end, code));
            }

            if !source.is_empty() {
                if let Some(tree) = self.ts_parser.parse(&source, None) {
                    let mut leaves = Vec::<Node>::new();
                    collect_leaf_nodes(tree.root_node(), &mut leaves);

                    for node in leaves {
                        let color = match syntax_color(lang, node.kind()) {
                            Some(c) => c,
                            None => continue,
                        };

                        let token_start = node.start_byte();
                        let token_end = node.end_byte();
                        if token_end <= token_start {
                            continue;
                        }

                        for (line_idx, line_start, line_end, code) in &maps {
                            if token_end <= *line_start || token_start >= *line_end {
                                continue;
                            }

                            let overlap_start = token_start.max(*line_start) - *line_start;
                            let overlap_end = token_end.min(*line_end) - *line_start;
                            if overlap_end <= overlap_start {
                                continue;
                            }

                            let c0 = byte_to_col(code, overlap_start);
                            let c1 = byte_to_col(code, overlap_end);
                            if c1 <= c0 {
                                continue;
                            }

                            spans_per_line[*line_idx].push(ColorSpan {
                                start_col: 1 + c0,
                                end_col: 1 + c1,
                                color,
                            });
                        }
                    }
                }
            }

            idx = hunk_end;
        }

        for spans in &mut spans_per_line {
            spans.sort_by_key(|s| s.start_col);
        }

        Ok(spans_per_line)
    }

    #[allow(dead_code)]
    pub fn build_document(&mut self) -> anyhow::Result<(Document, GitViewMeta)> {
        let mut lines: Vec<DocLine> = Vec::new();

        lines.push(DocLine::new("FILES", LineStyle::Header));

        let files_start_line = lines.len();
        if self.entries.is_empty() {
            lines.push(DocLine::new("  (working tree clean)", LineStyle::Dim));
        } else {
            for (idx, e) in self.entries.iter().enumerate() {
                let marker = if idx == self.selected { ">" } else { " " };
                lines.push(DocLine::new(
                    format!("{} {} {}", marker, e.xy, e.path),
                    if idx == self.selected {
                        LineStyle::Selected
                    } else {
                        LineStyle::Normal
                    },
                ));
            }
        }
        let files_count = self.entries.len();

        lines.push(DocLine::new("", LineStyle::Dim));

        let diff_title = self
            .entries
            .get(self.selected)
            .map(|e| format!("DIFF {}", e.path))
            .unwrap_or_else(|| String::from("DIFF"));
        lines.push(DocLine::new(diff_title, LineStyle::Header));

        let diff_lines: Vec<String> = self.diff.lines().map(normalize_for_display).collect();
        let diff_styles: Vec<LineStyle> = self.diff.lines().map(style_for_diff_line).collect();
        let diff_spans = self.syntax_spans_for_diff_lines(&diff_lines, &diff_styles)?;

        for ((text, style), spans) in diff_lines
            .into_iter()
            .zip(diff_styles.into_iter())
            .zip(diff_spans.into_iter())
        {
            lines.push(DocLine::new(text, style).with_spans(spans));
        }

        Ok((
            Document::from_lines(lines),
            GitViewMeta {
                files_start_line,
                files_count,
            },
        ))
    }
}

fn ts_language(lang: SyntaxLang) -> Language {
    match lang {
        SyntaxLang::Rust => tree_sitter_rust::LANGUAGE.into(),
        SyntaxLang::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
    }
}

fn collect_leaf_nodes<'a>(node: Node<'a>, out: &mut Vec<Node<'a>>) {
    if node.child_count() == 0 {
        out.push(node);
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_leaf_nodes(child, out);
    }
}

fn byte_to_col(s: &str, byte_offset: usize) -> usize {
    if byte_offset >= s.len() {
        return s.chars().count();
    }
    let mut safe = byte_offset;
    while safe > 0 && !s.is_char_boundary(safe) {
        safe -= 1;
    }
    s[..safe].chars().count()
}

fn syntax_color(lang: SyntaxLang, kind: &str) -> Option<[f32; 4]> {
    match lang {
        SyntaxLang::Rust => {
            if kind.contains("comment") {
                Some([0.46, 0.64, 0.50, 1.0])
            } else if matches!(
                kind,
                "string_literal" | "raw_string_literal" | "char_literal"
            ) {
                Some([0.91, 0.72, 0.48, 1.0])
            } else if matches!(kind, "integer_literal" | "float_literal") {
                Some([0.77, 0.67, 0.95, 1.0])
            } else if matches!(
                kind,
                "fn" | "let"
                    | "pub"
                    | "impl"
                    | "struct"
                    | "enum"
                    | "use"
                    | "mod"
                    | "if"
                    | "else"
                    | "match"
                    | "for"
                    | "while"
                    | "loop"
                    | "return"
                    | "async"
                    | "await"
                    | "const"
                    | "static"
                    | "trait"
                    | "where"
                    | "mut"
                    | "in"
                    | "as"
                    | "move"
                    | "crate"
                    | "super"
                    | "self"
                    | "Self"
            ) {
                Some([0.57, 0.73, 0.98, 1.0])
            } else if matches!(kind, "type_identifier" | "primitive_type") {
                Some([0.45, 0.86, 0.80, 1.0])
            } else if matches!(kind, "true" | "false") {
                Some([0.77, 0.67, 0.95, 1.0])
            } else {
                None
            }
        }
        SyntaxLang::Toml => {
            if kind.contains("comment") {
                Some([0.46, 0.64, 0.50, 1.0])
            } else if kind.contains("string") {
                Some([0.91, 0.72, 0.48, 1.0])
            } else if matches!(kind, "integer" | "float") {
                Some([0.77, 0.67, 0.95, 1.0])
            } else if kind.contains("key") {
                Some([0.45, 0.86, 0.80, 1.0])
            } else if matches!(kind, "boolean" | "true" | "false") {
                Some([0.77, 0.67, 0.95, 1.0])
            } else {
                None
            }
        }
    }
}

fn parse_porcelain_status(status: &str) -> Vec<GitEntry> {
    status
        .lines()
        .filter_map(parse_porcelain_status_line)
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct BranchTrackingSnapshot {
    upstream: Option<String>,
    ahead: usize,
    behind: usize,
}

fn parse_branch_tracking_snapshot(status: &str) -> BranchTrackingSnapshot {
    let mut snapshot = BranchTrackingSnapshot::default();

    for line in status.lines() {
        let Some(rest) = line.strip_prefix("# branch.") else {
            continue;
        };

        if let Some(upstream) = rest.strip_prefix("upstream ") {
            let upstream = upstream.trim();
            if !upstream.is_empty() {
                snapshot.upstream = Some(upstream.to_string());
            }
            continue;
        }

        if let Some(ab) = rest.strip_prefix("ab ") {
            if let Some((ahead, behind)) = parse_branch_ab_counts(ab) {
                snapshot.ahead = ahead;
                snapshot.behind = behind;
            }
        }
    }

    snapshot
}

fn parse_branch_ab_counts(value: &str) -> Option<(usize, usize)> {
    let mut ahead = None;
    let mut behind = None;

    for part in value.split_whitespace() {
        if let Some(rest) = part.strip_prefix('+') {
            ahead = rest.parse::<usize>().ok();
        } else if let Some(rest) = part.strip_prefix('-') {
            behind = rest.parse::<usize>().ok();
        }
    }

    Some((ahead?, behind?))
}

fn parse_porcelain_status_line(line: &str) -> Option<GitEntry> {
    let bytes = line.as_bytes();
    if bytes.len() < 4 || bytes.get(2) != Some(&b' ') {
        return None;
    }

    let xy = format!("{}{}", bytes[0] as char, bytes[1] as char);
    let path_part = line.get(3..)?;
    let path = path_part
        .rsplit_once(" -> ")
        .map_or(path_part, |(_, path)| path)
        .to_string();

    Some(GitEntry { xy, path })
}

pub fn classify_status_xy(xy: &str) -> Vec<StatusSectionKind> {
    let mut kinds = Vec::new();
    let mut chars = xy.chars();
    let x = chars.next().unwrap_or(' ');
    let y = chars.next().unwrap_or(' ');

    if x == '?' && y == '?' {
        kinds.push(StatusSectionKind::Untracked);
        return kinds;
    }

    if x != ' ' && x != '?' {
        kinds.push(StatusSectionKind::Staged);
    }

    if y != ' ' && y != '?' {
        kinds.push(StatusSectionKind::Unstaged);
    }

    kinds
}

#[allow(dead_code)]
pub fn summarize_porcelain_status(status: &str) -> Vec<StatusSectionSummary> {
    let mut counts = [0usize; 3];

    for line in status.lines() {
        if let Some(entry) = parse_porcelain_status_line(line) {
            if let Some(kind) = primary_status_section_kind(&entry.xy) {
                counts[kind.index()] += 1;
            }
        }
    }

    [
        StatusSectionKind::Staged,
        StatusSectionKind::Unstaged,
        StatusSectionKind::Untracked,
    ]
    .into_iter()
    .map(|kind| StatusSectionSummary {
        kind,
        count: counts[kind.index()],
    })
    .collect()
}

fn grouped_entries(entries: &[GitEntry]) -> [Vec<&GitEntry>; 3] {
    let mut grouped: [Vec<&GitEntry>; 3] = [Vec::new(), Vec::new(), Vec::new()];

    for entry in entries {
        if let Some(kind) = primary_status_section_kind(&entry.xy) {
            grouped[kind.index()].push(entry);
        }
    }

    grouped
}

fn primary_status_section_kind(xy: &str) -> Option<StatusSectionKind> {
    classify_status_xy(xy).into_iter().next()
}

fn style_for_diff_line(line: &str) -> LineStyle {
    if line.starts_with("@@") {
        LineStyle::DiffHunk
    } else if line.starts_with('+') && !line.starts_with("+++") {
        LineStyle::DiffAdd
    } else if line.starts_with('-') && !line.starts_with("---") {
        LineStyle::DiffRemove
    } else if line.starts_with("diff --git") {
        LineStyle::DiffFileHeader
    } else if line.starts_with("index ")
        || line.starts_with("--- ")
        || line.starts_with("+++ ")
        || line.starts_with("# ")
    {
        LineStyle::DiffMeta
    } else {
        LineStyle::Normal
    }
}

/// Parse `@@ -old,count +new,count @@` → (old_start, new_start)
fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    // Format: @@ -<old_start>[,<old_count>] +<new_start>[,<new_count>] @@
    let rest = line.strip_prefix("@@ -")?;
    let minus_end = rest.find(' ')?;
    let old_part = &rest[..minus_end];
    let old_start: u32 = old_part
        .split(',')
        .next()?
        .parse()
        .ok()?;

    let rest = &rest[minus_end..];
    let plus_start = rest.find('+')?;
    let rest = &rest[plus_start + 1..];
    let plus_end = rest.find(' ').unwrap_or(rest.len());
    let new_part = &rest[..plus_end];
    let new_start: u32 = new_part
        .split(',')
        .next()?
        .parse()
        .ok()?;

    Some((old_start, new_start))
}

fn normalize_for_display(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut col = 0usize;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    continue;
                }
                out.push('\n');
                col = 0;
            }
            '\n' => {
                out.push('\n');
                col = 0;
            }
            '\t' => {
                let spaces = 4 - (col % 4);
                for _ in 0..spaces {
                    out.push(' ');
                }
                col += spaces;
            }
            _ => {
                out.push(ch);
                col += 1;
            }
        }
    }

    out
}

fn compact_preview(text: &str, max_chars: usize) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = normalized.chars().count();
    if count <= max_chars {
        return normalized;
    }

    if max_chars <= 3 {
        return normalized.chars().take(max_chars).collect();
    }

    let mut out = String::with_capacity(max_chars);
    for ch in normalized.chars().take(max_chars - 3) {
        out.push(ch);
    }
    out.push_str("...");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_status_lines_and_rename_targets() {
        let status = " M src/main.rs\n?? Cargo.lock\n";

        let entries = parse_porcelain_status(status);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].xy, " M");
        assert_eq!(entries[0].path, "src/main.rs");
        assert_eq!(entries[1].xy, "??");
        assert_eq!(entries[1].path, "Cargo.lock");

        let rename = parse_porcelain_status_line("R  old name.txt -> renamed name.txt")
            .expect("rename line should parse");
        assert_eq!(rename.xy, "R ");
        assert_eq!(rename.path, "renamed name.txt");
    }

    #[test]
    fn ignores_non_status_lines() {
        let status = "\
not a status line
 M src/lib.rs
";

        let entries = parse_porcelain_status(status);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "src/lib.rs");
    }

    #[test]
    fn classifies_diff_lines() {
        assert!(matches!(
            style_for_diff_line("@@ -1,2 +1,2 @@"),
            LineStyle::DiffHunk
        ));
        assert!(matches!(
            style_for_diff_line("+added line"),
            LineStyle::DiffAdd
        ));
        assert!(matches!(
            style_for_diff_line("-removed line"),
            LineStyle::DiffRemove
        ));
        assert!(matches!(
            style_for_diff_line("+++ b/src/lib.rs"),
            LineStyle::DiffMeta
        ));
        assert!(matches!(
            style_for_diff_line("index 123..456"),
            LineStyle::DiffMeta
        ));
    }

    #[test]
    fn normalizes_tabs_and_crlf() {
        let text = "a\tb\r\nc";
        assert_eq!(normalize_for_display(text), "a   b\nc");
    }

    #[test]
    fn parses_hunk_headers() {
        assert_eq!(parse_hunk_header("@@ -10,5 +20,8 @@ fn foo()"), Some((10, 20)));
        assert_eq!(parse_hunk_header("@@ -1 +1 @@"), Some((1, 1)));
        assert_eq!(parse_hunk_header("@@ -0,0 +1,3 @@"), Some((0, 1)));
        assert_eq!(parse_hunk_header("not a hunk"), None);
    }

    #[test]
    fn classifies_status_sections() {
        assert_eq!(classify_status_xy(" M"), vec![StatusSectionKind::Unstaged]);
        assert_eq!(classify_status_xy("A "), vec![StatusSectionKind::Staged]);
        assert_eq!(classify_status_xy("??"), vec![StatusSectionKind::Untracked]);
        assert_eq!(
            classify_status_xy("MM"),
            vec![StatusSectionKind::Staged, StatusSectionKind::Unstaged]
        );
    }

    #[test]
    fn parses_branch_tracking_snapshot() {
        let status = concat!(
            "# branch.oid 1234567890abcdef\n",
            "# branch.head feature/demo\n",
            "# branch.upstream origin/main\n",
            "# branch.ab +3 -2\n",
            "1 .M N... 100644 100644 100644 123 456 src/main.rs\n",
        );

        let snapshot = parse_branch_tracking_snapshot(status);
        assert_eq!(snapshot.upstream.as_deref(), Some("origin/main"));
        assert_eq!(snapshot.ahead, 3);
        assert_eq!(snapshot.behind, 2);
    }

    #[test]
    fn parses_branch_ab_counts() {
        assert_eq!(parse_branch_ab_counts("+7 -4"), Some((7, 4)));
        assert_eq!(parse_branch_ab_counts("-4 +7"), Some((7, 4)));
        assert_eq!(parse_branch_ab_counts("+0 -0"), Some((0, 0)));
    }

    #[test]
    fn summarizes_porcelain_status_into_sections() {
        let status = concat!(
            " M src/main.rs\n",
            "A  src/lib.rs\n",
            "MM src/config.toml\n",
            "?? Cargo.lock\n",
        );

        let summaries = summarize_porcelain_status(status);
        assert_eq!(
            summaries,
            vec![
                StatusSectionSummary {
                    kind: StatusSectionKind::Staged,
                    count: 2,
                },
                StatusSectionSummary {
                    kind: StatusSectionKind::Unstaged,
                    count: 1,
                },
                StatusSectionSummary {
                    kind: StatusSectionKind::Untracked,
                    count: 1,
                },
            ]
        );
    }
}

use crate::git_model::GitModel;

/// High-level Git operations that the UI can dispatch.
#[derive(Clone, Debug)]
pub enum GitCommand {
    Refresh,
    Stage { index: usize },
    Unstage { index: usize },
    StageAll,
    UnstageAll,
    Commit { message: String },
    Fetch { remote: Option<String> },
    Pull { remote: Option<String>, branch: Option<String> },
    Push { remote: Option<String>, branch: Option<String> },
    Discard,
}

/// Classifies how risky a command is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafetyTier {
    /// Read-only: refresh, inspect.
    Safe,
    /// Modifies the index or working tree: stage, unstage, commit.
    Guarded,
    /// Hard to undo: discard, reset, force push.
    Dangerous,
}

impl GitCommand {
    /// Returns the safety classification for this command.
    pub fn safety_tier(&self) -> SafetyTier {
        match self {
            Self::Refresh => SafetyTier::Safe,
            Self::Fetch { .. } => SafetyTier::Safe,

            Self::Stage { .. }
            | Self::Unstage { .. }
            | Self::StageAll
            | Self::UnstageAll
            | Self::Commit { .. }
            | Self::Pull { .. }
            | Self::Push { .. } => SafetyTier::Guarded,

            Self::Discard => SafetyTier::Dangerous,
        }
    }

    /// A short human-readable description of the command.
    pub fn description(&self) -> String {
        match self {
            Self::Refresh => "refresh status".into(),
            Self::Stage { index } => format!("stage file at index {index}"),
            Self::Unstage { index } => format!("unstage file at index {index}"),
            Self::StageAll => "stage all files".into(),
            Self::UnstageAll => "unstage all files".into(),
            Self::Commit { message } => {
                let preview: String = message.chars().take(48).collect();
                if preview.len() < message.len() {
                    format!("commit: \"{preview}...\"")
                } else {
                    format!("commit: \"{preview}\"")
                }
            }
            Self::Fetch { remote } => match remote {
                Some(r) => format!("fetch {r}"),
                None => "fetch".into(),
            },
            Self::Pull { remote, branch } => match (remote, branch) {
                (Some(r), Some(b)) => format!("pull {r}/{b}"),
                _ => "pull".into(),
            },
            Self::Push { remote, branch } => match (remote, branch) {
                (Some(r), Some(b)) => format!("push {r}/{b}"),
                _ => "push".into(),
            },
            Self::Discard => "discard selected changes".into(),
        }
    }
}

/// The outcome of executing a [`GitCommand`].
pub struct CommandResult {
    /// Human-readable description of the command that was run.
    pub command: String,
    /// Whether the command completed successfully.
    pub success: bool,
    /// A success or error message.
    pub message: String,
}

/// Dispatches a [`GitCommand`] to the appropriate [`GitModel`] method and
/// returns a [`CommandResult`].
pub fn execute(cmd: &GitCommand, git: &mut GitModel) -> CommandResult {
    let description = cmd.description();

    let result = match cmd {
        GitCommand::Refresh => git.refresh(),
        GitCommand::Stage { index } => git
            .select_file_index(*index)
            .and_then(|()| git.stage_selected()),
        GitCommand::Unstage { index } => git
            .select_file_index(*index)
            .and_then(|()| git.unstage_selected()),
        GitCommand::StageAll => git.stage_all(),
        GitCommand::UnstageAll => git.unstage_all(),
        GitCommand::Commit { message } => git.commit(message),
        GitCommand::Fetch { remote } => git.fetch(remote.as_deref()),
        GitCommand::Pull { remote, branch } => {
            git.pull(remote.as_deref(), branch.as_deref(), false)
        }
        GitCommand::Push { remote, branch } => {
            git.push(remote.as_deref(), branch.as_deref(), false)
        }
        GitCommand::Discard => git.discard_selected(),
    };

    match result {
        Ok(()) => CommandResult {
            command: description,
            success: true,
            message: "ok".into(),
        },
        Err(err) => CommandResult {
            command: description,
            success: false,
            message: format!("{err:#}"),
        },
    }
}

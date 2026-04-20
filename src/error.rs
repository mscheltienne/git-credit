use thiserror::Error;

#[derive(Error, Debug)]
pub enum CreditError {
    #[error("failed to open repository at '{path}'")]
    RepoOpen { path: String, source: git2::Error },

    #[error(transparent)]
    Git(#[from] git2::Error),

    #[error("invalid date format '{input}': expected YYYY-MM-DD")]
    InvalidDate { input: String },

    #[error("invalid revision range '{range}'")]
    InvalidRevRange { range: String, source: git2::Error },

    #[error(transparent)]
    GitHubRequest(#[from] reqwest::Error),

    #[error("GitHub API returned {status}: {body}")]
    GitHubApi { status: u16, body: String },

    #[error("could not determine GitHub remote from repository")]
    NoGitHubRemote,

    #[error("invalid glob pattern '{pattern}': {reason}")]
    InvalidGlob { pattern: String, reason: String },

    #[error(transparent)]
    Serialize(#[from] serde_json::Error),
}

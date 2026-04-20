use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, USER_AGENT};
use serde::Deserialize;

use crate::error::CreditError;
use crate::git::{Author, FileDelta};

static GITHUB_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:https?://github\.com/|git@github\.com:)([^/]+)/([^/.]+?)(?:\.git)?$").unwrap()
});

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Parsed owner/repo from a GitHub remote URL.
#[derive(Debug, Clone)]
pub struct RepoSlug {
    pub owner: String,
    pub repo: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PrCommit {
    pub sha: String,
    pub commit: PrCommitInner,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PrCommitInner {
    pub author: PrAuthorInfo,
}

#[derive(Debug, Deserialize)]
pub(crate) struct PrAuthorInfo {
    pub name: Option<String>,
    pub email: Option<String>,
}

/// A file entry from the GitHub commit detail endpoint.
#[derive(Debug, Deserialize)]
struct GhFileEntry {
    filename: String,
    additions: u64,
    deletions: u64,
}

/// Response from `GET /repos/{owner}/{repo}/commits/{sha}`.
#[derive(Debug, Deserialize)]
struct GhCommitResponse {
    files: Option<Vec<GhFileEntry>>,
}

// ---------------------------------------------------------------------------
// Trait for testability
// ---------------------------------------------------------------------------

/// Abstraction over GitHub API calls, enabling mock implementations in tests.
pub trait GitHubApi: Send + Sync {
    fn fetch_pr_commits(&self, pr_number: u64) -> Result<Vec<(Author, String)>, CreditError>;

    fn fetch_commit_files(&self, sha: &str) -> Result<Vec<FileDelta>, CreditError>;
}

// ---------------------------------------------------------------------------
// GitHub client
// ---------------------------------------------------------------------------

pub struct GitHubClient {
    client: Client,
    token: String,
    slug: RepoSlug,
}

impl GitHubClient {
    pub fn new(token: String, slug: RepoSlug) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("failed to build HTTP client");
        Self {
            client,
            token,
            slug,
        }
    }

    fn api_url(&self, path: &str) -> String {
        format!(
            "https://api.github.com/repos/{}/{}{path}",
            self.slug.owner, self.slug.repo
        )
    }

    fn get(&self, url: &str) -> Result<reqwest::blocking::Response, CreditError> {
        let resp = self
            .client
            .get(url)
            .header(AUTHORIZATION, format!("Bearer {}", self.token))
            .header(USER_AGENT, "git-credit")
            .header(ACCEPT, "application/vnd.github+json")
            .send()?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().unwrap_or_default();
            return Err(CreditError::GitHubApi { status, body });
        }

        Ok(resp)
    }
}

impl GitHubApi for GitHubClient {
    fn fetch_pr_commits(&self, pr_number: u64) -> Result<Vec<(Author, String)>, CreditError> {
        let mut all = Vec::new();
        let mut page = 1u32;

        loop {
            let url = self.api_url(&format!(
                "/pulls/{pr_number}/commits?per_page=100&page={page}"
            ));
            let resp = self.get(&url)?;
            let commits: Vec<PrCommit> = resp.json()?;
            let count = commits.len();

            for c in commits {
                let author = Author {
                    name: c.commit.author.name.unwrap_or_else(|| "Unknown".into()),
                    email: c.commit.author.email.unwrap_or_else(|| "unknown".into()),
                };
                all.push((author, c.sha));
            }

            if count < 100 {
                break;
            }
            page += 1;
        }

        Ok(all)
    }

    fn fetch_commit_files(&self, sha: &str) -> Result<Vec<FileDelta>, CreditError> {
        let url = self.api_url(&format!("/commits/{sha}"));
        let resp = self.get(&url)?;
        let detail: GhCommitResponse = resp.json()?;

        Ok(detail
            .files
            .unwrap_or_default()
            .into_iter()
            .filter(|f| f.additions > 0 || f.deletions > 0)
            .map(|f| FileDelta {
                path: f.filename,
                additions: f.additions,
                deletions: f.deletions,
            })
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Token resolution
// ---------------------------------------------------------------------------

/// Resolve a GitHub token using the chain:
/// 1. Explicit flag value
/// 2. `GITHUB_TOKEN` environment variable
/// 3. `GH_TOKEN` environment variable
/// 4. `gh auth token` command
pub fn resolve_token(flag_token: Option<&str>) -> Option<String> {
    resolve_token_from_sources(
        flag_token,
        std::env::var("GITHUB_TOKEN").ok().as_deref(),
        std::env::var("GH_TOKEN").ok().as_deref(),
        gh_auth_token().as_deref(),
    )
}

/// Pure function for testability — takes all four token sources directly.
pub(crate) fn resolve_token_from_sources(
    flag: Option<&str>,
    github_token_env: Option<&str>,
    gh_token_env: Option<&str>,
    gh_cli_token: Option<&str>,
) -> Option<String> {
    [flag, github_token_env, gh_token_env, gh_cli_token]
        .into_iter()
        .flatten()
        .find(|t| !t.is_empty())
        .map(String::from)
}

/// Attempt to get a token from the `gh` CLI.
fn gh_auth_token() -> Option<String> {
    Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            let token = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if token.is_empty() { None } else { Some(token) }
        })
}

// ---------------------------------------------------------------------------
// Slug extraction
// ---------------------------------------------------------------------------

/// Extract the GitHub owner/repo from the repository's `origin` remote URL.
pub fn extract_slug(repo: &git2::Repository) -> Result<RepoSlug, CreditError> {
    let remote = repo
        .find_remote("origin")
        .map_err(|_| CreditError::NoGitHubRemote)?;
    let url = remote.url().ok_or(CreditError::NoGitHubRemote)?;
    parse_github_url(url).ok_or(CreditError::NoGitHubRemote)
}

/// Parse a GitHub remote URL into owner/repo.
///
/// Supports:
/// - `https://github.com/owner/repo.git`
/// - `https://github.com/owner/repo`
/// - `git@github.com:owner/repo.git`
/// - `git@github.com:owner/repo`
fn parse_github_url(url: &str) -> Option<RepoSlug> {
    GITHUB_URL_RE.captures(url).map(|cap| RepoSlug {
        owner: cap[1].to_string(),
        repo: cap[2].to_string(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_token_flag_wins() {
        let token = resolve_token_from_sources(
            Some("flag_token"),
            Some("env_token"),
            Some("gh_token"),
            Some("cli_token"),
        );
        assert_eq!(token.as_deref(), Some("flag_token"));
    }

    #[test]
    fn resolve_token_env_fallback() {
        let token = resolve_token_from_sources(None, Some("env_token"), Some("gh_token"), None);
        assert_eq!(token.as_deref(), Some("env_token"));
    }

    #[test]
    fn resolve_token_gh_env_fallback() {
        let token = resolve_token_from_sources(None, None, Some("gh_token"), None);
        assert_eq!(token.as_deref(), Some("gh_token"));
    }

    #[test]
    fn resolve_token_cli_fallback() {
        let token = resolve_token_from_sources(None, None, None, Some("cli_token"));
        assert_eq!(token.as_deref(), Some("cli_token"));
    }

    #[test]
    fn resolve_token_none() {
        let token = resolve_token_from_sources(None, None, None, None);
        assert!(token.is_none());
    }

    #[test]
    fn resolve_token_skips_empty() {
        let token = resolve_token_from_sources(Some(""), None, Some("gh"), None);
        assert_eq!(token.as_deref(), Some("gh"));
    }

    #[test]
    fn parse_github_https_url() {
        let slug = parse_github_url("https://github.com/owner/repo.git").unwrap();
        assert_eq!(slug.owner, "owner");
        assert_eq!(slug.repo, "repo");
    }

    #[test]
    fn parse_github_https_no_git_suffix() {
        let slug = parse_github_url("https://github.com/owner/repo").unwrap();
        assert_eq!(slug.owner, "owner");
        assert_eq!(slug.repo, "repo");
    }

    #[test]
    fn parse_github_ssh_url() {
        let slug = parse_github_url("git@github.com:owner/repo.git").unwrap();
        assert_eq!(slug.owner, "owner");
        assert_eq!(slug.repo, "repo");
    }

    #[test]
    fn parse_github_ssh_no_git_suffix() {
        let slug = parse_github_url("git@github.com:owner/repo").unwrap();
        assert_eq!(slug.owner, "owner");
        assert_eq!(slug.repo, "repo");
    }

    #[test]
    fn parse_non_github_url() {
        assert!(parse_github_url("https://gitlab.com/owner/repo").is_none());
    }
}

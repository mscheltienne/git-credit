use std::process::Command;
use std::sync::LazyLock;
use std::time::Duration;

use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, AUTHORIZATION, HeaderMap, RETRY_AFTER, USER_AGENT};
use serde::Deserialize;

use crate::error::CreditError;
use crate::git::{Author, FileDelta};

static GITHUB_URL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[@/])github\.com[:/]([^/]+)/([^/]+?)(?:\.git)?/?$").unwrap()
});

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Parsed owner/repo from a GitHub remote URL.
#[derive(Debug)]
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
            if is_rate_limited(status, resp.headers()) {
                return Err(CreditError::RateLimited);
            }
            let body = resp.text().unwrap_or_default();
            return Err(CreditError::GitHubApi { status, body });
        }

        Ok(resp)
    }
}

/// GitHub signals rate limiting with a 429, or a 403 carrying an exhausted quota or
/// a `retry-after` header; any other 403 is a permission error.
fn is_rate_limited(status: u16, headers: &HeaderMap) -> bool {
    status == 429
        || status == 403
            && (headers.contains_key(RETRY_AFTER)
                || headers
                    .get("x-ratelimit-remaining")
                    .is_some_and(|v| v == "0"))
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

/// Resolve a GitHub token from, in order: the `--token` flag, `GITHUB_TOKEN`,
/// `GH_TOKEN`, then `gh auth token`. Empty values are skipped.
pub fn resolve_token(flag_token: Option<&str>) -> Option<String> {
    resolve_token_from_sources(
        flag_token,
        std::env::var("GITHUB_TOKEN").ok().as_deref(),
        std::env::var("GH_TOKEN").ok().as_deref(),
        gh_auth_token,
    )
}

/// [`resolve_token`] with its sources injected; `gh_cli_token` runs only when no
/// other source has a token.
pub(crate) fn resolve_token_from_sources(
    flag: Option<&str>,
    github_token_env: Option<&str>,
    gh_token_env: Option<&str>,
    gh_cli_token: impl FnOnce() -> Option<String>,
) -> Option<String> {
    [flag, github_token_env, gh_token_env]
        .into_iter()
        .flatten()
        .find(|t| !t.is_empty())
        .map(String::from)
        .or_else(gh_cli_token)
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
    // `Remote::url()` returns `Result<&str, git2::Error>` since git2 0.21
    // (non-UTF-8 URLs surface as `Err`); previously it was `Option<&str>`.
    // Either failure mode collapses to `NoGitHubRemote` for our purposes.
    let url = remote.url().map_err(|_| CreditError::NoGitHubRemote)?;
    parse_github_url(url).ok_or(CreditError::NoGitHubRemote)
}

/// Parse a GitHub HTTPS, SSH or scp-style remote URL into owner/repo.
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

    fn unreachable_cli() -> Option<String> {
        panic!("gh auth token must not run when another source has a token")
    }

    fn resolve(flag: Option<&str>, github: Option<&str>, gh: Option<&str>) -> Option<String> {
        resolve_token_from_sources(flag, github, gh, unreachable_cli)
    }

    #[test]
    fn resolve_token_precedence() {
        let cli = || Some("cli".to_string());
        assert_eq!(
            resolve(Some("flag"), Some("env"), Some("gh")).as_deref(),
            Some("flag")
        );
        assert_eq!(
            resolve(None, Some("env"), Some("gh")).as_deref(),
            Some("env")
        );
        assert_eq!(resolve(None, None, Some("gh")).as_deref(), Some("gh"));
        assert_eq!(
            resolve_token_from_sources(None, None, None, cli).as_deref(),
            Some("cli")
        );
        assert_eq!(resolve_token_from_sources(None, None, None, || None), None);
    }

    #[test]
    fn resolve_token_skips_empty() {
        assert_eq!(resolve(Some(""), None, Some("gh")).as_deref(), Some("gh"));
    }

    #[test]
    fn rate_limit_detection() {
        let none = HeaderMap::new();
        let mut exhausted = HeaderMap::new();
        exhausted.insert("x-ratelimit-remaining", "0".parse().unwrap());
        let mut remaining = HeaderMap::new();
        remaining.insert("x-ratelimit-remaining", "12".parse().unwrap());
        let mut retry = HeaderMap::new();
        retry.insert(RETRY_AFTER, "60".parse().unwrap());

        assert!(is_rate_limited(429, &none));
        assert!(is_rate_limited(403, &exhausted));
        assert!(is_rate_limited(403, &retry));
        assert!(!is_rate_limited(403, &remaining));
        assert!(!is_rate_limited(403, &none));
        assert!(!is_rate_limited(404, &exhausted));
    }

    #[test]
    fn parse_github_urls() {
        for (url, repo) in [
            ("https://github.com/owner/repo.git", "repo"),
            ("https://github.com/owner/repo", "repo"),
            ("https://github.com/owner/repo/", "repo"),
            ("git@github.com:owner/repo.git", "repo"),
            ("git@github.com:owner/repo", "repo"),
            ("ssh://git@github.com/owner/repo.git", "repo"),
            ("https://github.com/owner/my.repo.git", "my.repo"),
            ("git@github.com:owner/my.repo", "my.repo"),
        ] {
            let slug = parse_github_url(url).unwrap_or_else(|| panic!("{url} did not parse"));
            assert_eq!(
                (slug.owner.as_str(), slug.repo.as_str()),
                ("owner", repo),
                "{url}"
            );
        }
    }

    #[test]
    fn parse_non_github_urls() {
        for url in [
            "https://gitlab.com/owner/repo",
            "https://notgithub.com/owner/repo",
        ] {
            assert!(parse_github_url(url).is_none(), "{url}");
        }
    }
}

pub mod cli;
pub mod error;
pub mod filter;
pub mod git;
pub mod github;
pub mod output;
pub mod stats;

use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use git2::Mailmap;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use cli::Cli;
use error::CreditError;
use filter::ExclusionFilter;
use git::{Author, CommitInfo, format_utc_iso8601};
use github::GitHubApi;
use stats::{Attribution, CommitReport, Report, Summary, compute_squash_attributions, strip_bots};

/// Main entry point — orchestrates the full analysis.
pub fn run(cli: &Cli) -> Result<()> {
    let repo = git::open_repo(&cli.repo).context("could not open git repository")?;
    let mailmap = load_mailmap(cli, &repo)?;
    let filter = ExclusionFilter::new(&cli.excludes).context("invalid exclusion pattern")?;
    let client = resolve_github_client(cli, &repo);

    let since = cli
        .since
        .as_deref()
        .map(git::parse_date_to_epoch)
        .transpose()
        .context("invalid --since date")?;
    let commits = git::walk_commits(&repo, cli.rev.as_deref(), since, mailmap.as_ref(), &filter)
        .context("failed to walk commits")?;

    let mut commits = match client.as_deref() {
        Some(client) => expand_squash_merges(commits, client, mailmap.as_ref(), &filter),
        None => commits.iter().map(|c| author_report(c, true)).collect(),
    };
    let total_commits_walked = commits.len() as u64;
    let squash_merges_expanded = commits.iter().filter(|c| c.is_squash_pr).count() as u64;
    let bots_excluded = if cli.bots {
        0
    } else {
        strip_bots(&mut commits)
    };
    commits.sort_by(|a, b| {
        a.author_date
            .cmp(&b.author_date)
            .then_with(|| a.sha.cmp(&b.sha))
    });

    let report = Report {
        commits,
        summary: Summary {
            total_commits_walked,
            squash_merges_expanded,
            bots_excluded,
        },
    };

    output::render(&report, &cli.format)?;
    Ok(())
}

/// Load the mailmap from disk: prefer `--mailmap-file <PATH>` if set,
/// else fall back to `repo.mailmap()` (worktree `.mailmap` → `HEAD:.mailmap`
/// → `mailmap.file` config). Returns `Ok(None)` when `--no-mailmap` is set,
/// short-circuiting both paths so the output carries raw `commit.author()`
/// identities.
fn load_mailmap(cli: &Cli, repo: &git2::Repository) -> Result<Option<Mailmap>, CreditError> {
    if cli.no_mailmap {
        return Ok(None);
    }
    if let Some(path) = &cli.mailmap_file {
        let path_str = path.display().to_string();
        let content = fs::read_to_string(path).map_err(|source| CreditError::MailmapRead {
            path: path_str.clone(),
            source,
        })?;
        let mailmap =
            Mailmap::from_buffer(&content).map_err(|source| CreditError::MailmapParse {
                path: path_str,
                source,
            })?;
        return Ok(Some(mailmap));
    }
    Ok(repo.mailmap().ok())
}

/// Build a report crediting the whole commit to its own author. `accurate: false`
/// marks a squash merge whose PR could not be expanded.
fn author_report(commit: &CommitInfo, accurate: bool) -> CommitReport {
    let attribution = Attribution {
        name: commit.author.name.clone(),
        email: commit.author.email.clone(),
        additions: commit.additions,
        deletions: commit.deletions,
        is_pr_author: false,
    };
    commit_report(commit, false, vec![attribution], accurate)
}

fn commit_report(
    commit: &CommitInfo,
    is_squash_pr: bool,
    attributions: Vec<Attribution>,
    accurate: bool,
) -> CommitReport {
    CommitReport {
        sha: commit.oid.to_string(),
        author_date: format_utc_iso8601(commit.author_time),
        is_squash_pr,
        attributions,
        accurate,
    }
}

/// Build one report per commit, crediting each squash merge to its PR's authors.
///
/// A squash merge whose PR cannot be fetched falls back to its own author with
/// `accurate: false`. The first rate-limit response stops further API calls.
fn expand_squash_merges(
    commits: Vec<CommitInfo>,
    client: &dyn GitHubApi,
    mailmap: Option<&Mailmap>,
    filter: &ExclusionFilter,
) -> Vec<CommitReport> {
    let squash_merges = commits.iter().filter(|c| c.pr_number.is_some()).count();
    let progress = ProgressBar::new(squash_merges as u64);
    progress.set_style(
        ProgressStyle::with_template("{spinner:.green} [{bar:40}] {pos}/{len} PRs")
            .expect("valid template")
            .progress_chars("=> "),
    );
    let rate_limited = AtomicBool::new(false);
    let results: Vec<_> = commits
        .into_par_iter()
        .map(|commit| {
            let weights = commit.pr_number.map(|pr_number| {
                let result = if rate_limited.load(Ordering::Relaxed) {
                    Err(CreditError::GitHubApi {
                        status: 403,
                        body: "rate limit exceeded (skipped)".into(),
                    })
                } else {
                    fetch_pr_weights(client, pr_number, filter)
                };
                if matches!(result, Err(CreditError::GitHubApi { status: 403, .. })) {
                    rate_limited.store(true, Ordering::Relaxed);
                }
                progress.inc(1);
                result
            });
            (commit, weights)
        })
        .collect();
    progress.finish_and_clear();

    let mut failures = 0;
    let reports = results
        .into_iter()
        .map(|(commit, weights)| match weights {
            None => author_report(&commit, true),
            Some(Ok(weights)) => {
                // Mailmap is not `Sync`, so it applies here rather than during the fetch.
                let weights: Vec<_> = weights
                    .into_iter()
                    .map(|(a, adds, dels)| {
                        (git::resolve_author(mailmap, &a.name, &a.email), adds, dels)
                    })
                    .collect();
                let attributions =
                    compute_squash_attributions(&weights, commit.additions, commit.deletions);
                commit_report(&commit, true, attributions, true)
            }
            Some(Err(e)) => {
                if failures == 0 {
                    let pr_number = commit.pr_number.unwrap_or_default();
                    eprintln!("warning: GitHub API error for PR #{pr_number}: {e}");
                }
                failures += 1;
                author_report(&commit, false)
            }
        })
        .collect();
    if failures > 0 {
        eprintln!("warning: {failures} PRs fell back to commit-author attribution");
    }
    reports
}

/// Weigh each author of a PR by the lines their commits changed.
///
/// Returns one `(author, additions, deletions)` entry per PR commit, counting only
/// the non-excluded files. When every commit shares one email, the per-commit file
/// fetches are skipped and that author is returned alone.
fn fetch_pr_weights(
    client: &dyn GitHubApi,
    pr_number: u64,
    filter: &ExclusionFilter,
) -> Result<Vec<(Author, u64, u64)>, CreditError> {
    let pr_commits = client.fetch_pr_commits(pr_number)?;
    let Some((first, _)) = pr_commits.first() else {
        let unknown = Author {
            name: "Unknown".into(),
            email: "unknown".into(),
        };
        return Ok(vec![(unknown, 0, 0)]);
    };
    if pr_commits.iter().all(|(a, _)| a.email == first.email) {
        return Ok(vec![(first.clone(), 0, 0)]);
    }
    pr_commits
        .into_par_iter()
        .map(|(author, sha)| {
            let (additions, deletions) = filter.line_totals(&client.fetch_commit_files(&sha)?);
            Ok((author, additions, deletions))
        })
        .collect()
}

fn resolve_github_client(cli: &Cli, repo: &git2::Repository) -> Option<Box<dyn GitHubApi>> {
    if cli.no_github {
        return None;
    }

    let Some(token) = github::resolve_token(cli.token.as_deref()) else {
        eprintln!(
            "warning: no GitHub token found, skipping squash-merge attribution\n\
             hint: set GITHUB_TOKEN, use --token, or install the `gh` CLI"
        );
        return None;
    };

    match github::extract_slug(repo) {
        Ok(slug) => Some(Box::new(github::GitHubClient::new(token, slug))),
        Err(e) => {
            eprintln!("warning: {e}, skipping GitHub lookups");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use git::FileDelta;

    /// Serves PR commit lists and commit files from memory; unknown PRs are a 404.
    #[derive(Default)]
    struct MockApi {
        prs: HashMap<u64, Vec<(Author, String)>>,
        files: HashMap<String, Vec<FileDelta>>,
    }

    impl MockApi {
        fn pr(mut self, number: u64, commits: &[(Author, &str, Vec<FileDelta>)]) -> Self {
            let mut list = Vec::new();
            for (author, sha, files) in commits {
                list.push((author.clone(), (*sha).to_string()));
                self.files.insert((*sha).to_string(), files.clone());
            }
            self.prs.insert(number, list);
            self
        }
    }

    impl GitHubApi for MockApi {
        fn fetch_pr_commits(&self, pr_number: u64) -> Result<Vec<(Author, String)>, CreditError> {
            self.prs
                .get(&pr_number)
                .cloned()
                .ok_or(CreditError::GitHubApi {
                    status: 404,
                    body: "Not Found".into(),
                })
        }

        fn fetch_commit_files(&self, sha: &str) -> Result<Vec<FileDelta>, CreditError> {
            Ok(self.files.get(sha).cloned().unwrap_or_default())
        }
    }

    fn author(name: &str, email: &str) -> Author {
        Author {
            name: name.into(),
            email: email.into(),
        }
    }

    fn delta(path: &str, additions: u64) -> Vec<FileDelta> {
        vec![FileDelta {
            path: path.into(),
            additions,
            deletions: 0,
        }]
    }

    /// A squash merge of `pr_number` adding `additions` lines, committed by Merger.
    fn squash(pr_number: u64, additions: u64) -> CommitInfo {
        CommitInfo {
            oid: git2::Oid::ZERO_SHA1,
            author: author("Merger", "merger@example.com"),
            author_time: 0,
            pr_number: Some(pr_number),
            additions,
            deletions: 0,
        }
    }

    fn expand(api: &MockApi, commit: CommitInfo, excludes: &[&str]) -> CommitReport {
        let excludes: Vec<String> = excludes.iter().map(ToString::to_string).collect();
        let filter = ExclusionFilter::new(&excludes).unwrap();
        let mut reports = expand_squash_merges(vec![commit], api, None, &filter);
        assert_eq!(reports.len(), 1);
        reports.remove(0)
    }

    fn lines(report: &CommitReport) -> Vec<(&str, u64)> {
        report
            .attributions
            .iter()
            .map(|a| (a.email.as_str(), a.additions))
            .collect()
    }

    #[test]
    fn multi_author_pr_splits_by_weight() {
        let api = MockApi::default().pr(
            1,
            &[
                (
                    author("Alice", "alice@example.com"),
                    "a1",
                    delta("a.rs", 30),
                ),
                (author("Bob", "bob@example.com"), "b1", delta("b.rs", 10)),
            ],
        );
        let report = expand(&api, squash(1, 100), &[]);
        assert!(report.is_squash_pr && report.accurate);
        assert_eq!(
            lines(&report),
            [("alice@example.com", 75), ("bob@example.com", 25)]
        );
    }

    #[test]
    fn excluded_files_do_not_weigh_pr_authors() {
        let api = MockApi::default().pr(
            1,
            &[
                (
                    author("Alice", "alice@example.com"),
                    "a1",
                    delta("Cargo.lock", 900),
                ),
                (
                    author("Bob", "bob@example.com"),
                    "b1",
                    delta("src/main.rs", 10),
                ),
            ],
        );
        let report = expand(&api, squash(1, 10), &["*.lock"]);
        assert_eq!(
            lines(&report),
            [("alice@example.com", 0), ("bob@example.com", 10)]
        );
    }

    #[test]
    fn unknown_pr_falls_back_to_the_squash_author() {
        let report = expand(&MockApi::default(), squash(1, 10), &[]);
        assert!(!report.is_squash_pr && !report.accurate);
        assert_eq!(lines(&report), [("merger@example.com", 10)]);
        assert!(!report.attributions[0].is_pr_author);
    }
}

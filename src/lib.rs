pub mod cli;
pub mod error;
pub mod filter;
pub mod git;
pub mod github;
pub mod output;
pub mod stats;

use std::collections::HashSet;
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use git2::Mailmap;
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use cli::Cli;
use error::CreditError;
use filter::ExclusionFilter;
use git::{CommitInfo, WalkOptions, format_utc_iso8601};
use github::GitHubApi;
use stats::{Attribution, CommitReport, Report, Summary, compute_squash_attributions, filter_bots};

/// Result of attributing a squash-merge PR to its individual authors.
enum PrAttribution {
    /// All PR commits have the same author — skip per-commit file fetches.
    SingleAuthor(git::Author),
    /// Multiple distinct authors — full per-commit file deltas needed.
    MultiAuthor(Vec<(git::Author, Vec<git::FileDelta>)>),
}

/// Main entry point — orchestrates the full analysis.
pub fn run(cli: &Cli) -> Result<()> {
    let repo = git::open_repo(&cli.repo).context("could not open git repository")?;
    let mailmap = load_mailmap(cli, &repo)?;
    let filter = ExclusionFilter::new(&cli.excludes).context("invalid exclusion pattern")?;
    let gh_client: Option<Box<dyn GitHubApi>> = resolve_github_client(cli, &repo);

    let since = cli
        .since
        .as_deref()
        .map(git::parse_date_to_epoch)
        .transpose()
        .context("invalid --since date")?;
    let walk_opts = WalkOptions {
        rev_range: cli.rev.clone(),
        since,
    };

    let commits =
        git::walk_commits(&repo, &walk_opts, mailmap.as_ref()).context("failed to walk commits")?;

    // Partition into owned vecs to avoid cloning deltas later.
    let mut regular = Vec::new();
    let mut squash_merges = Vec::new();
    for commit in commits {
        if let Some(pr_number) = git::is_squash_merge(&commit) {
            squash_merges.push((commit, pr_number));
        } else {
            regular.push(commit);
        }
    }

    let total = regular.len() + squash_merges.len();
    let spinner = ProgressBar::new(total as u64);
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.green} [{bar:40}] {pos}/{len} commits")
            .expect("valid template")
            .progress_chars("=> "),
    );

    let mut emitted: Vec<CommitReport> = Vec::with_capacity(total);
    let mut squash_merges_expanded: u64 = 0;

    for commit in regular {
        emitted.push(direct_commit_report(&filter, commit));
        spinner.inc(1);
    }

    if let Some(ref client) = gh_client {
        process_squash_merges(
            client.as_ref(),
            squash_merges,
            &filter,
            mailmap.as_ref(),
            &spinner,
            &mut emitted,
            &mut squash_merges_expanded,
        );
    } else {
        // No GitHub client → attribute every squash merge to the merge author.
        for (commit, _) in squash_merges {
            emitted.push(direct_commit_report(&filter, commit));
            spinner.inc(1);
        }
    }

    spinner.finish_and_clear();

    let total_commits_walked = emitted.len() as u64;

    // Bot filtering — strip per-attribution; drop the whole commit if all
    // attributions are bots. Track the *unique* set of bot emails seen.
    let mut bot_emails: HashSet<String> = HashSet::new();
    if !cli.bots {
        let mut kept = Vec::with_capacity(emitted.len());
        for commit in emitted {
            for a in &commit.attributions {
                if git::is_bot_email(&a.email) {
                    bot_emails.insert(a.email.clone());
                }
            }
            if let Some(commit) = filter_bots(commit) {
                kept.push(commit);
            }
        }
        emitted = kept;
    }

    // Stable order: by author_date ascending, sha as a tie-breaker.
    emitted.sort_by(|a, b| {
        a.author_date
            .cmp(&b.author_date)
            .then_with(|| a.sha.cmp(&b.sha))
    });

    let report = Report {
        commits: emitted,
        summary: Summary {
            total_commits_walked,
            squash_merges_expanded,
            bots_excluded: bot_emails.len() as u64,
        },
    };

    output::render(&report, &cli.format)?;
    Ok(())
}

/// Load the mailmap from disk: prefer `--mailmap-file <PATH>` if set,
/// else fall back to `repo.mailmap()` (worktree `.mailmap` → `HEAD:.mailmap`
/// → `mailmap.file` config).
fn load_mailmap(cli: &Cli, repo: &git2::Repository) -> Result<Option<Mailmap>, CreditError> {
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

/// Build a [`CommitReport`] for a direct commit (no squash-merge expansion).
fn direct_commit_report(filter: &ExclusionFilter, commit: CommitInfo) -> CommitReport {
    let deltas = filter.filter_deltas(commit.deltas);
    let additions: u64 = deltas.iter().map(|d| d.additions).sum();
    let deletions: u64 = deltas.iter().map(|d| d.deletions).sum();
    CommitReport {
        sha: commit.oid.to_string(),
        author_date: format_utc_iso8601(commit.author_time),
        is_squash_pr: false,
        attributions: vec![Attribution {
            name: commit.author.name,
            email: commit.author.email,
            additions,
            deletions,
            is_pr_author: false,
        }],
        accurate: true,
    }
}

#[allow(clippy::too_many_arguments)]
fn process_squash_merges(
    client: &dyn GitHubApi,
    squash_merges: Vec<(git::CommitInfo, u64)>,
    filter: &ExclusionFilter,
    mailmap: Option<&git2::Mailmap>,
    spinner: &ProgressBar,
    emitted: &mut Vec<CommitReport>,
    squash_merges_expanded: &mut u64,
) {
    let rate_limit_flag = AtomicBool::new(false);
    let pr_results: Vec<_> = squash_merges
        .par_iter()
        .map(|(_, pr_number)| {
            let result = if rate_limit_flag.load(Ordering::Relaxed) {
                Err(error::CreditError::GitHubApi {
                    status: 403,
                    body: "rate limit exceeded (skipped)".into(),
                })
            } else {
                let r = fetch_pr_attribution(client, *pr_number);
                if matches!(&r, Err(error::CreditError::GitHubApi { status: 403, .. })) {
                    rate_limit_flag.store(true, Ordering::Relaxed);
                }
                r
            };
            spinner.inc(1);
            result
        })
        .collect();

    let mut api_errors: u64 = 0;
    let mut rate_limited = false;
    for ((commit, pr_number), result) in squash_merges.into_iter().zip(pr_results) {
        let sha = commit.oid.to_string();
        let author_date = format_utc_iso8601(commit.author_time);
        let deltas = filter.filter_deltas(commit.deltas.clone());

        match result {
            Ok(PrAttribution::SingleAuthor(author)) => {
                let resolved = git::resolve_author(mailmap, &author.name, &author.email);
                let additions: u64 = deltas.iter().map(|d| d.additions).sum();
                let deletions: u64 = deltas.iter().map(|d| d.deletions).sum();
                emitted.push(CommitReport {
                    sha,
                    author_date,
                    is_squash_pr: true,
                    attributions: vec![Attribution {
                        name: resolved.name,
                        email: resolved.email,
                        additions,
                        deletions,
                        is_pr_author: true,
                    }],
                    accurate: true,
                });
                *squash_merges_expanded += 1;
            }
            Ok(PrAttribution::MultiAuthor(pr_author_deltas)) => {
                let resolved: Vec<_> = pr_author_deltas
                    .into_iter()
                    .map(|(a, d)| (git::resolve_author(mailmap, &a.name, &a.email), d))
                    .collect();
                let attributions = compute_squash_attributions(&resolved, &deltas);
                emitted.push(CommitReport {
                    sha,
                    author_date,
                    is_squash_pr: true,
                    attributions,
                    accurate: true,
                });
                *squash_merges_expanded += 1;
            }
            Err(error::CreditError::GitHubApi { status: 403, .. }) => {
                rate_limited = true;
                api_errors += 1;
                emitted.push(fallback_commit_report(
                    sha,
                    author_date,
                    &commit.author,
                    &deltas,
                ));
            }
            Err(e) => {
                if api_errors == 0 {
                    eprintln!("warning: GitHub API error for PR #{pr_number}: {e}");
                }
                api_errors += 1;
                emitted.push(fallback_commit_report(
                    sha,
                    author_date,
                    &commit.author,
                    &deltas,
                ));
            }
        }
    }
    if rate_limited {
        eprintln!(
            "warning: GitHub API rate limit exceeded, \
             {api_errors} PRs fell back to commit-author attribution"
        );
    } else if api_errors > 0 {
        eprintln!(
            "warning: {api_errors} GitHub API errors, \
             PRs fell back to commit-author attribution"
        );
    }
}

/// Build a [`CommitReport`] for a squash-merge whose GitHub re-attribution
/// failed — attribute everything to the merge commit's own author. The
/// `accurate: false` flag tells consumers this row is a fallback that
/// should be retried once the API is available again.
fn fallback_commit_report(
    sha: String,
    author_date: String,
    author: &git::Author,
    deltas: &[git::FileDelta],
) -> CommitReport {
    let additions: u64 = deltas.iter().map(|d| d.additions).sum();
    let deletions: u64 = deltas.iter().map(|d| d.deletions).sum();
    CommitReport {
        sha,
        author_date,
        is_squash_pr: false,
        attributions: vec![Attribution {
            name: author.name.clone(),
            email: author.email.clone(),
            additions,
            deletions,
            is_pr_author: false,
        }],
        accurate: false,
    }
}

/// Fetch PR attribution, optimizing for single-author PRs.
///
/// Makes 1 API call to list PR commits. If all commits share the same
/// email, returns `SingleAuthor` (skipping N per-commit file fetches).
/// Otherwise fetches per-commit file stats and returns `MultiAuthor`.
fn fetch_pr_attribution(
    client: &dyn GitHubApi,
    pr_number: u64,
) -> Result<PrAttribution, error::CreditError> {
    let pr_commits = client.fetch_pr_commits(pr_number)?;

    if pr_commits.is_empty() {
        return Ok(PrAttribution::SingleAuthor(git::Author {
            name: "Unknown".into(),
            email: "unknown".into(),
        }));
    }

    // Check if all commits have the same author (by raw email).
    let first_email = &pr_commits[0].0.email;
    let all_same = pr_commits.iter().all(|(a, _)| a.email == *first_email);

    if all_same {
        return Ok(PrAttribution::SingleAuthor(
            pr_commits.into_iter().next().unwrap().0,
        ));
    }

    // Multi-author: fetch per-commit file deltas in parallel.
    let author_deltas: Result<Vec<_>, error::CreditError> = pr_commits
        .par_iter()
        .map(|(author, sha)| {
            let deltas = client.fetch_commit_files(sha)?;
            Ok((author.clone(), deltas))
        })
        .collect();
    Ok(PrAttribution::MultiAuthor(author_deltas?))
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

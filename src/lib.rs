pub mod cli;
pub mod error;
pub mod filter;
pub mod git;
pub mod github;
pub mod output;
pub mod stats;

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use cli::Cli;
use filter::ExclusionFilter;
use git::WalkOptions;
use github::GitHubApi;
use stats::StatsAccumulator;

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
    let mailmap = repo.mailmap().ok();
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

    let total = (regular.len() + squash_merges.len()) as u64;
    let spinner = ProgressBar::new(total);
    spinner.set_style(
        ProgressStyle::with_template("{spinner:.green} [{bar:40}] {pos}/{len} commits")
            .expect("valid template")
            .progress_chars("=> "),
    );

    let mut acc = StatsAccumulator::default();

    for commit in regular {
        acc.record_commit();
        let deltas = filter.filter_deltas(commit.deltas);
        acc.attribute(&commit.author, &deltas);
        spinner.inc(1);
    }

    if let Some(ref client) = gh_client {
        process_squash_merges(
            client.as_ref(),
            squash_merges,
            &filter,
            mailmap.as_ref(),
            &spinner,
            &mut acc,
        );
    } else {
        for (commit, _) in squash_merges {
            acc.record_commit();
            let deltas = filter.filter_deltas(commit.deltas);
            acc.attribute(&commit.author, &deltas);
            spinner.inc(1);
        }
    }

    spinner.finish_and_clear();

    let mut report = acc.finalize();
    if !cli.bots {
        let total = report.authors.len();
        report.authors.retain(|a| !git::is_bot_email(&a.email));
        report.bots_excluded = (total - report.authors.len()) as u64;
    }
    output::render(&report, &cli.format)?;
    Ok(())
}

fn process_squash_merges(
    client: &dyn GitHubApi,
    squash_merges: Vec<(git::CommitInfo, u64)>,
    filter: &ExclusionFilter,
    mailmap: Option<&git2::Mailmap>,
    spinner: &ProgressBar,
    acc: &mut StatsAccumulator,
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
        acc.record_commit();
        let deltas = filter.filter_deltas(commit.deltas);
        match result {
            Ok(PrAttribution::SingleAuthor(author)) => {
                let resolved = git::resolve_author(mailmap, &author.name, &author.email);
                acc.attribute(&resolved, &deltas);
                acc.mark_pr(&resolved);
                acc.record_squash_expansion();
            }
            Ok(PrAttribution::MultiAuthor(pr_author_deltas)) => {
                let resolved: Vec<_> = pr_author_deltas
                    .into_iter()
                    .map(|(a, d)| (git::resolve_author(mailmap, &a.name, &a.email), d))
                    .collect();
                acc.attribute_squash_merge(&resolved, &deltas);
                acc.record_squash_expansion();
            }
            Err(error::CreditError::GitHubApi { status: 403, .. }) => {
                rate_limited = true;
                api_errors += 1;
                acc.attribute(&commit.author, &deltas);
            }
            Err(e) => {
                if api_errors == 0 {
                    eprintln!("warning: GitHub API error for PR #{pr_number}: {e}");
                }
                api_errors += 1;
                acc.attribute(&commit.author, &deltas);
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

pub mod cli;
pub mod error;
pub mod filter;
pub mod git;
pub mod github;
pub mod output;
pub mod stats;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;

use cli::Cli;
use filter::ExclusionFilter;
use git::WalkOptions;
use github::GitHubApi;
use stats::StatsAccumulator;

/// Main entry point — orchestrates the full analysis.
pub fn run(cli: &Cli) -> Result<()> {
    let repo = git::open_repo(&cli.repo).context("could not open git repository")?;
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

    let commits = git::walk_commits(&repo, &walk_opts).context("failed to walk commits")?;

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
        // Fetch PR data in parallel across squash merges.
        let pr_results: Vec<_> = squash_merges
            .par_iter()
            .map(|(_, pr_number)| {
                let result = fetch_pr_attribution(client.as_ref(), *pr_number);
                spinner.inc(1);
                result
            })
            .collect();

        for ((commit, pr_number), result) in squash_merges.into_iter().zip(pr_results) {
            acc.record_commit();
            let deltas = filter.filter_deltas(commit.deltas);
            match result {
                Ok(pr_author_deltas) => {
                    acc.attribute_squash_merge(&pr_author_deltas, &deltas);
                    acc.record_squash_expansion();
                }
                Err(e) => {
                    eprintln!("warning: GitHub API error for PR #{pr_number}: {e}");
                    acc.attribute(&commit.author, &deltas);
                }
            }
        }
    } else {
        for (commit, _) in squash_merges {
            acc.record_commit();
            let deltas = filter.filter_deltas(commit.deltas);
            acc.attribute(&commit.author, &deltas);
            spinner.inc(1);
        }
    }

    spinner.finish_and_clear();

    let report = acc.finalize();
    output::render(&report, &cli.format)?;
    Ok(())
}

fn fetch_pr_attribution(
    client: &dyn GitHubApi,
    pr_number: u64,
) -> Result<Vec<(git::Author, Vec<git::FileDelta>)>, error::CreditError> {
    let pr_commits = client.fetch_pr_commits(pr_number)?;
    let mut author_deltas = Vec::new();
    for (author, sha) in &pr_commits {
        let deltas = client.fetch_commit_files(sha)?;
        author_deltas.push((author.clone(), deltas));
    }
    Ok(author_deltas)
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

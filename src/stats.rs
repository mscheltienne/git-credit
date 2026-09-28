use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use crate::git::{Author, is_bot_email};

/// One commit of the report.
///
/// `attributions` has a single entry, except for a squash merge expanded through the
/// GitHub API (`is_squash_pr: true`), which has one entry per PR author, each with
/// `is_pr_author: true`. `accurate` is `false` when that expansion failed (rate limit,
/// API error, or a PR without non-merge commits): the row then credits the squash
/// commit's author, and consumers may retry it later. Without GitHub access
/// (`--no-github`, no token, no GitHub remote), squash merges are reported as regular
/// commits with `accurate: true`.
#[derive(Debug, Clone, Serialize)]
pub struct CommitReport {
    pub sha: String,
    pub author_date: String,
    pub is_squash_pr: bool,
    pub attributions: Vec<Attribution>,
    pub accurate: bool,
}

/// One author's share of a commit's line changes.
#[derive(Debug, Clone, Serialize)]
pub struct Attribution {
    pub name: String,
    pub email: String,
    pub additions: u64,
    pub deletions: u64,
    pub is_pr_author: bool,
}

/// Run-level counters.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Summary {
    /// Commits reported before bot filtering; merge commits are not walked.
    pub total_commits_walked: u64,
    /// Squash merges credited to their PR's authors via the GitHub API.
    pub squash_merges_expanded: u64,
    /// Distinct bot emails removed from the attributions.
    pub bots_excluded: u64,
}

/// The full report produced by a run.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Report {
    pub commits: Vec<CommitReport>,
    pub summary: Summary,
}

/// Split a squash merge's line totals across the PR's authors.
///
/// `weights` holds one `(author, additions, deletions)` entry per PR commit. Entries
/// whose emails match case-insensitively are merged under the first one's identity,
/// and each author gets the share of `additions` and `deletions` their weights
/// represent, or an equal split when the weights are all zero. Returns one
/// attribution per author, ordered by lowercased email. Integer division rounds
/// down, so the shares may sum to slightly less than the totals.
#[must_use]
pub fn compute_squash_attributions(
    weights: &[(Author, u64, u64)],
    additions: u64,
    deletions: u64,
) -> Vec<Attribution> {
    let mut per_author: BTreeMap<String, (&Author, u64, u64)> = BTreeMap::new();
    for (author, adds, dels) in weights {
        let entry = per_author
            .entry(author.email.to_lowercase())
            .or_insert((author, 0, 0));
        entry.1 += adds;
        entry.2 += dels;
    }
    let (weight_adds, weight_dels) = per_author
        .values()
        .fold((0, 0), |(a, d), e| (a + e.1, d + e.2));
    let num_authors = (per_author.len() as u64).max(1);

    per_author
        .into_values()
        .map(|(author, adds, dels)| Attribution {
            name: author.name.clone(),
            email: author.email.clone(),
            additions: (additions * adds)
                .checked_div(weight_adds)
                .unwrap_or(additions / num_authors),
            deletions: (deletions * dels)
                .checked_div(weight_dels)
                .unwrap_or(deletions / num_authors),
            is_pr_author: true,
        })
        .collect()
}

/// Remove bot attributions, then the commits left without any.
///
/// Returns the number of distinct bot emails removed, compared case-insensitively.
pub fn strip_bots(commits: &mut Vec<CommitReport>) -> u64 {
    let mut bots = HashSet::new();
    commits.retain_mut(|commit| {
        commit.attributions.retain(|a| {
            let is_bot = is_bot_email(&a.email);
            if is_bot {
                bots.insert(a.email.to_lowercase());
            }
            !is_bot
        });
        !commit.attributions.is_empty()
    });
    bots.len() as u64
}

/// Aggregated stats for a single author, computed from a [`Report`].
#[derive(Debug, Default)]
pub struct AuthorStats {
    pub name: String,
    pub email: String,
    pub contributions: u64,
    pub prs: u64,
    pub additions: u64,
    pub deletions: u64,
}

/// Per-author totals for the table output, keyed by case-insensitive email and sorted
/// by additions + deletions, descending.
#[must_use]
pub fn rollup_by_author(report: &Report) -> Vec<AuthorStats> {
    let mut map: BTreeMap<String, AuthorStats> = BTreeMap::new();
    for commit in &report.commits {
        for attribution in &commit.attributions {
            let entry = map
                .entry(attribution.email.to_lowercase())
                .or_insert_with(|| AuthorStats {
                    name: attribution.name.clone(),
                    email: attribution.email.clone(),
                    ..Default::default()
                });
            entry.contributions += 1;
            entry.additions += attribution.additions;
            entry.deletions += attribution.deletions;
            if attribution.is_pr_author {
                entry.prs += 1;
            }
        }
    }
    let mut rolled: Vec<AuthorStats> = map.into_values().collect();
    rolled.sort_by_key(|a| std::cmp::Reverse(a.additions + a.deletions));
    rolled
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> Author {
        Author {
            name: "Alice".into(),
            email: "alice@example.com".into(),
        }
    }

    fn bob() -> Author {
        Author {
            name: "Bob".into(),
            email: "bob@example.com".into(),
        }
    }

    fn commit_with(sha: &str, attributions: Vec<Attribution>) -> CommitReport {
        CommitReport {
            sha: sha.into(),
            author_date: "2025-01-01T00:00:00Z".into(),
            is_squash_pr: false,
            attributions,
            accurate: true,
        }
    }

    fn direct(name: &str, email: &str, adds: u64, dels: u64) -> Attribution {
        Attribution {
            name: name.into(),
            email: email.into(),
            additions: adds,
            deletions: dels,
            is_pr_author: false,
        }
    }

    #[test]
    fn squash_proportional_two_authors_sorted_by_email() {
        let weights = vec![(bob(), 25, 0), (alice(), 75, 0)];
        let result = compute_squash_attributions(&weights, 100, 0);
        assert_eq!(result[0].email, "alice@example.com");
        assert_eq!(result[0].additions, 75);
        assert_eq!(result[1].email, "bob@example.com");
        assert_eq!(result[1].additions, 25);
        assert!(result.iter().all(|a| a.is_pr_author));
    }

    #[test]
    fn squash_zero_weights_falls_back_to_equal_split() {
        let weights = vec![(alice(), 0, 0), (bob(), 0, 0)];
        let result = compute_squash_attributions(&weights, 10, 4);
        for a in &result {
            assert_eq!((a.additions, a.deletions), (5, 2));
        }
    }

    #[test]
    fn squash_same_author_multiple_commits_ignoring_email_case() {
        let shouted = Author {
            name: "Alice".into(),
            email: "Alice@Example.COM".into(),
        };
        let weights = vec![(shouted, 30, 0), (alice(), 40, 0), (alice(), 30, 0)];
        let result = compute_squash_attributions(&weights, 100, 0);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].email, "Alice@Example.COM");
        assert_eq!(result[0].additions, 100);
    }

    #[test]
    fn strip_bots_removes_bot_attributions_and_empty_commits() {
        let dependabot = "dependabot[bot]@users.noreply.github.com";
        let mut commits = vec![
            commit_with(
                "c1",
                vec![direct(
                    "dependabot",
                    "Dependabot[bot]@users.noreply.github.com",
                    10,
                    0,
                )],
            ),
            commit_with(
                "c2",
                vec![
                    direct("Alice", "alice@example.com", 10, 5),
                    direct("dependabot", dependabot, 100, 50),
                    direct("ci", "ci[bot]@users.noreply.github.com", 1, 0),
                ],
            ),
            commit_with("c3", vec![direct("Bob", "bob@example.com", 1, 1)]),
        ];
        assert_eq!(strip_bots(&mut commits), 2);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].sha, "c2");
        assert_eq!(commits[0].attributions.len(), 1);
        assert_eq!(commits[1].sha, "c3");
    }

    #[test]
    fn rollup_two_authors_sorted_by_total_desc() {
        let report = Report {
            commits: vec![
                commit_with("c1", vec![direct("Alice", "alice@example.com", 5, 5)]),
                commit_with("c2", vec![direct("Bob", "bob@example.com", 20, 10)]),
            ],
            summary: Summary::default(),
        };
        let rolled = rollup_by_author(&report);
        assert_eq!(rolled[0].name, "Bob");
        assert_eq!(rolled[1].name, "Alice");
    }

    #[test]
    fn rollup_counts_prs_only_when_is_pr_author() {
        let mut commit = commit_with("c1", vec![direct("Alice", "alice@example.com", 10, 0)]);
        commit.attributions[0].is_pr_author = true;
        commit.is_squash_pr = true;
        let report = Report {
            commits: vec![
                commit,
                commit_with("c2", vec![direct("Alice", "Alice@Example.com", 5, 0)]),
            ],
            summary: Summary::default(),
        };
        let rolled = rollup_by_author(&report);
        assert_eq!(rolled.len(), 1);
        assert_eq!(rolled[0].contributions, 2);
        assert_eq!(rolled[0].prs, 1);
        assert_eq!(rolled[0].additions, 15);
    }
}

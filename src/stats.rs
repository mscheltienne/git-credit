use std::collections::HashMap;

use serde::Serialize;

use crate::git::{Author, FileDelta, is_bot_email};

// ---------------------------------------------------------------------------
// JSON output types
// ---------------------------------------------------------------------------

/// One emitted commit (or one squash-merge with multiple authors).
///
/// For regular direct commits `attributions` has a single entry.
/// For successfully-expanded squash-merge PRs it has one entry per
/// re-attributed author, each marked `is_pr_author: true`.
/// For failed squash-merge expansion (no token / API error) it falls back
/// to a single entry with `is_squash_pr: false`.
#[derive(Debug, Clone, Serialize)]
pub struct CommitReport {
    pub sha: String,
    pub author_date: String,
    pub is_squash_pr: bool,
    pub attributions: Vec<Attribution>,
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
    pub total_commits_walked: u64,
    pub squash_merges_expanded: u64,
    pub bots_excluded: u64,
}

/// The full report produced by a run.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Report {
    pub commits: Vec<CommitReport>,
    pub summary: Summary,
}

// ---------------------------------------------------------------------------
// Squash-merge proportional attribution
// ---------------------------------------------------------------------------

/// Compute proportional per-author attributions for a squash-merge.
///
/// `pr_author_deltas` is the list of (author, file deltas) per PR commit
/// (may contain duplicates for the same author across multiple PR commits).
/// `squash_deltas` is the merge commit's filtered file deltas.
///
/// Returns one [`Attribution`] per unique author in the PR, with their
/// proportional share of the squash commit's totals. Each entry is marked
/// `is_pr_author: true`. The sum of per-author additions/deletions may be
/// slightly less than the squash totals due to integer-division rounding
/// (preserved 0.2.0 contract).
#[must_use]
pub fn compute_squash_attributions(
    pr_author_deltas: &[(Author, Vec<FileDelta>)],
    squash_deltas: &[FileDelta],
) -> Vec<Attribution> {
    let total_squash_adds: u64 = squash_deltas.iter().map(|d| d.additions).sum();
    let total_squash_dels: u64 = squash_deltas.iter().map(|d| d.deletions).sum();

    // Aggregate by unique author (email) — fixes double-counting when the
    // same author has multiple commits in a single PR.
    let mut aggregated: HashMap<String, (Author, u64, u64)> = HashMap::new();
    let mut grand_adds: u64 = 0;
    let mut grand_dels: u64 = 0;

    for (author, deltas) in pr_author_deltas {
        let adds: u64 = deltas.iter().map(|d| d.additions).sum();
        let dels: u64 = deltas.iter().map(|d| d.deletions).sum();
        grand_adds += adds;
        grand_dels += dels;
        let entry = aggregated
            .entry(author.email.clone())
            .or_insert_with(|| (author.clone(), 0, 0));
        entry.1 += adds;
        entry.2 += dels;
    }

    let num_authors = aggregated.len() as u64;
    let equal_adds = total_squash_adds / num_authors.max(1);
    let equal_dels = total_squash_dels / num_authors.max(1);

    // Stable order: sort by email ascending.
    let mut entries: Vec<_> = aggregated.into_values().collect();
    entries.sort_by(|a, b| a.0.email.cmp(&b.0.email));

    entries
        .into_iter()
        .map(|(author, adds, dels)| {
            let attributed_adds = (total_squash_adds * adds)
                .checked_div(grand_adds)
                .unwrap_or(equal_adds);
            let attributed_dels = (total_squash_dels * dels)
                .checked_div(grand_dels)
                .unwrap_or(equal_dels);
            Attribution {
                name: author.name,
                email: author.email,
                additions: attributed_adds,
                deletions: attributed_dels,
                is_pr_author: true,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Bot exclusion
// ---------------------------------------------------------------------------

/// Strip bot attributions from a [`CommitReport`].
///
/// Returns `None` when *every* attribution was a bot (drop the whole commit
/// from the output), otherwise returns the commit with bot attributions
/// removed.
pub fn filter_bots(mut commit: CommitReport) -> Option<CommitReport> {
    commit.attributions.retain(|a| !is_bot_email(&a.email));
    if commit.attributions.is_empty() {
        None
    } else {
        Some(commit)
    }
}

// ---------------------------------------------------------------------------
// Per-author rollup (used by the table renderer)
// ---------------------------------------------------------------------------

/// Aggregated stats for a single author, computed from a [`Report`].
#[derive(Debug, Clone, Default, Serialize)]
pub struct AuthorStats {
    pub name: String,
    pub email: String,
    pub contributions: u64,
    pub prs: u64,
    pub additions: u64,
    pub deletions: u64,
}

/// Fold a [`Report`] into a per-author rollup, sorted by total lines
/// (additions + deletions) descending.
///
/// Used by the table renderer; not part of the JSON output.
#[must_use]
pub fn rollup_by_author(report: &Report) -> Vec<AuthorStats> {
    let mut map: HashMap<String, AuthorStats> = HashMap::new();
    for commit in &report.commits {
        for attribution in &commit.attributions {
            let entry = map
                .entry(attribution.email.clone())
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

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

    fn delta(path: &str, adds: u64, dels: u64) -> FileDelta {
        FileDelta {
            path: path.into(),
            additions: adds,
            deletions: dels,
        }
    }

    fn commit_with(sha: &str, attributions: Vec<Attribution>) -> CommitReport {
        CommitReport {
            sha: sha.into(),
            author_date: "2025-01-01T00:00:00Z".into(),
            is_squash_pr: false,
            attributions,
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

    // -----------------------------------------------------------------------
    // compute_squash_attributions
    // -----------------------------------------------------------------------

    #[test]
    fn squash_proportional_two_authors() {
        let pr_deltas = vec![
            (alice(), vec![delta("a.rs", 75, 0)]),
            (bob(), vec![delta("b.rs", 25, 0)]),
        ];
        let squash = vec![delta("merged.rs", 100, 0)];

        let result = compute_squash_attributions(&pr_deltas, &squash);

        let a = result.iter().find(|a| a.name == "Alice").unwrap();
        let b = result.iter().find(|a| a.name == "Bob").unwrap();
        assert_eq!(a.additions, 75);
        assert_eq!(b.additions, 25);
        assert!(a.is_pr_author);
        assert!(b.is_pr_author);
    }

    #[test]
    fn squash_zero_totals_falls_back_to_equal_split() {
        let pr_deltas = vec![
            (alice(), vec![delta("a.rs", 0, 0)]),
            (bob(), vec![delta("b.rs", 0, 0)]),
        ];
        let squash = vec![delta("merged.rs", 10, 4)];

        let result = compute_squash_attributions(&pr_deltas, &squash);
        let a = result.iter().find(|a| a.name == "Alice").unwrap();
        let b = result.iter().find(|a| a.name == "Bob").unwrap();
        assert_eq!(a.additions, 5);
        assert_eq!(b.additions, 5);
        assert_eq!(a.deletions, 2);
        assert_eq!(b.deletions, 2);
    }

    #[test]
    fn squash_same_author_multiple_commits() {
        let pr_deltas = vec![
            (alice(), vec![delta("a.rs", 30, 0)]),
            (alice(), vec![delta("b.rs", 40, 0)]),
            (alice(), vec![delta("c.rs", 30, 0)]),
        ];
        let squash = vec![delta("merged.rs", 100, 0)];

        let result = compute_squash_attributions(&pr_deltas, &squash);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].additions, 100);
    }

    #[test]
    fn squash_attributions_sorted_by_email() {
        let pr_deltas = vec![
            (bob(), vec![delta("b.rs", 50, 0)]),
            (alice(), vec![delta("a.rs", 50, 0)]),
        ];
        let squash = vec![delta("merged.rs", 100, 0)];
        let result = compute_squash_attributions(&pr_deltas, &squash);
        assert_eq!(result[0].email, "alice@example.com");
        assert_eq!(result[1].email, "bob@example.com");
    }

    // -----------------------------------------------------------------------
    // filter_bots
    // -----------------------------------------------------------------------

    #[test]
    fn filter_bots_drops_full_bot_commit() {
        let commit = commit_with(
            "abc",
            vec![direct(
                "dependabot",
                "dependabot[bot]@users.noreply.github.com",
                10,
                0,
            )],
        );
        assert!(filter_bots(commit).is_none());
    }

    #[test]
    fn filter_bots_strips_individual_bot_attributions() {
        let commit = commit_with(
            "abc",
            vec![
                direct("Alice", "alice@example.com", 10, 5),
                direct("bot", "ci[bot]@users.noreply.github.com", 100, 50),
            ],
        );
        let filtered = filter_bots(commit).unwrap();
        assert_eq!(filtered.attributions.len(), 1);
        assert_eq!(filtered.attributions[0].email, "alice@example.com");
    }

    #[test]
    fn filter_bots_passthrough_when_no_bots() {
        let commit = commit_with("abc", vec![direct("Alice", "alice@example.com", 10, 5)]);
        assert!(filter_bots(commit).is_some());
    }

    // -----------------------------------------------------------------------
    // rollup_by_author
    // -----------------------------------------------------------------------

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
                commit_with("c2", vec![direct("Alice", "alice@example.com", 5, 0)]),
            ],
            summary: Summary::default(),
        };
        let rolled = rollup_by_author(&report);
        assert_eq!(rolled[0].contributions, 2);
        assert_eq!(rolled[0].prs, 1);
        assert_eq!(rolled[0].additions, 15);
    }
}

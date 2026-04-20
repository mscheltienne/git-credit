use std::collections::HashMap;

use serde::Serialize;

use crate::git::{Author, FileDelta};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Aggregated stats for a single author.
#[derive(Debug, Clone, Default, Serialize)]
pub struct AuthorStats {
    pub name: String,
    pub email: String,
    pub contributions: u64,
    pub prs: u64,
    pub additions: u64,
    pub deletions: u64,
}

/// The final report produced by the tool.
#[derive(Debug, Default, Serialize)]
pub struct CreditReport {
    pub authors: Vec<AuthorStats>,
    pub total_commits_walked: u64,
    pub squash_merges_expanded: u64,
    pub bots_excluded: u64,
}

// ---------------------------------------------------------------------------
// Accumulator
// ---------------------------------------------------------------------------

/// Internal accumulator used during the commit walk.
#[derive(Default)]
pub struct StatsAccumulator {
    map: HashMap<String, AuthorStats>,
    total_commits_walked: u64,
    squash_merges_expanded: u64,
}

impl StatsAccumulator {
    pub fn record_commit(&mut self) {
        self.total_commits_walked += 1;
    }

    pub fn record_squash_expansion(&mut self) {
        self.squash_merges_expanded += 1;
    }

    /// Attribute a set of file deltas to a single author (regular commit).
    pub fn attribute(&mut self, author: &Author, deltas: &[FileDelta]) {
        let entry = self.get_or_insert(author);
        entry.contributions += 1;
        for d in deltas {
            entry.additions += d.additions;
            entry.deletions += d.deletions;
        }
    }

    /// Increment the PR counter for an author without changing
    /// contributions or line counts.
    pub fn mark_pr(&mut self, author: &Author) {
        let entry = self.get_or_insert(author);
        entry.prs += 1;
    }

    /// Attribute a squash-merge proportionally to individual PR authors.
    ///
    /// `pr_author_deltas` contains one entry per PR commit (may have
    /// duplicates for the same author). This method aggregates by unique
    /// author before computing proportional attribution.
    pub fn attribute_squash_merge(
        &mut self,
        pr_author_deltas: &[(Author, Vec<FileDelta>)],
        squash_deltas: &[FileDelta],
    ) {
        let total_squash_adds: u64 = squash_deltas.iter().map(|d| d.additions).sum();
        let total_squash_dels: u64 = squash_deltas.iter().map(|d| d.deletions).sum();

        // Aggregate by unique author (email) — fixes double-counting bug.
        let mut aggregated: HashMap<String, (&Author, u64, u64)> = HashMap::new();
        let mut grand_adds: u64 = 0;
        let mut grand_dels: u64 = 0;

        for (author, deltas) in pr_author_deltas {
            let adds: u64 = deltas.iter().map(|d| d.additions).sum();
            let dels: u64 = deltas.iter().map(|d| d.deletions).sum();
            grand_adds += adds;
            grand_dels += dels;
            let entry = aggregated
                .entry(author.email.clone())
                .or_insert((author, 0, 0));
            entry.1 += adds;
            entry.2 += dels;
        }

        let num_authors = aggregated.len() as u64;
        let equal_adds = total_squash_adds / num_authors.max(1);
        let equal_dels = total_squash_dels / num_authors.max(1);
        for (author, adds, dels) in aggregated.values() {
            let attributed_adds = (total_squash_adds * adds)
                .checked_div(grand_adds)
                .unwrap_or(equal_adds);
            let attributed_dels = (total_squash_dels * dels)
                .checked_div(grand_dels)
                .unwrap_or(equal_dels);

            let entry = self.get_or_insert(author);
            entry.contributions += 1;
            entry.prs += 1;
            entry.additions += attributed_adds;
            entry.deletions += attributed_dels;
        }
    }

    /// Finalize into a sorted `CreditReport`.
    /// Authors are sorted by total lines changed (additions + deletions),
    /// descending.
    pub fn finalize(self) -> CreditReport {
        let mut authors: Vec<AuthorStats> = self.map.into_values().collect();
        authors.sort_by_key(|a| std::cmp::Reverse(a.additions + a.deletions));
        CreditReport {
            authors,
            total_commits_walked: self.total_commits_walked,
            squash_merges_expanded: self.squash_merges_expanded,
            bots_excluded: 0,
        }
    }

    fn get_or_insert(&mut self, author: &Author) -> &mut AuthorStats {
        self.map
            .entry(author.email.clone())
            .or_insert_with(|| AuthorStats {
                name: author.name.clone(),
                email: author.email.clone(),
                ..Default::default()
            })
    }
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

    #[test]
    fn attribute_single_author() {
        let mut acc = StatsAccumulator::default();
        acc.attribute(&alice(), &[delta("a.rs", 10, 2)]);
        acc.attribute(&alice(), &[delta("b.rs", 5, 1)]);
        let report = acc.finalize();
        assert_eq!(report.authors.len(), 1);
        assert_eq!(report.authors[0].contributions, 2);
        assert_eq!(report.authors[0].additions, 15);
        assert_eq!(report.authors[0].deletions, 3);
    }

    #[test]
    fn attribute_two_authors() {
        let mut acc = StatsAccumulator::default();
        acc.attribute(&alice(), &[delta("a.rs", 10, 0)]);
        acc.attribute(&bob(), &[delta("b.rs", 20, 5)]);
        let report = acc.finalize();
        assert_eq!(report.authors.len(), 2);
        assert_eq!(report.authors[0].name, "Bob");
        assert_eq!(report.authors[1].name, "Alice");
    }

    #[test]
    fn attribute_squash_merge_proportional() {
        let mut acc = StatsAccumulator::default();
        let pr_deltas = vec![
            (alice(), vec![delta("a.rs", 75, 0)]),
            (bob(), vec![delta("b.rs", 25, 0)]),
        ];
        let squash_deltas = vec![delta("merged.rs", 100, 0)];

        acc.attribute_squash_merge(&pr_deltas, &squash_deltas);
        acc.record_squash_expansion();
        let report = acc.finalize();

        assert_eq!(report.squash_merges_expanded, 1);
        assert_eq!(report.authors.len(), 2);

        let alice_stats = report.authors.iter().find(|a| a.name == "Alice").unwrap();
        let bob_stats = report.authors.iter().find(|a| a.name == "Bob").unwrap();

        assert_eq!(alice_stats.additions, 75);
        assert_eq!(bob_stats.additions, 25);
        assert_eq!(alice_stats.prs, 1);
        assert_eq!(bob_stats.prs, 1);
        assert_eq!(alice_stats.contributions, 1);
        assert_eq!(bob_stats.contributions, 1);
    }

    #[test]
    fn attribute_squash_merge_zero_totals() {
        let mut acc = StatsAccumulator::default();
        let pr_deltas = vec![
            (alice(), vec![delta("a.rs", 0, 0)]),
            (bob(), vec![delta("b.rs", 0, 0)]),
        ];
        let squash_deltas = vec![delta("merged.rs", 10, 4)];
        acc.attribute_squash_merge(&pr_deltas, &squash_deltas);
        let report = acc.finalize();

        let alice_stats = report.authors.iter().find(|a| a.name == "Alice").unwrap();
        let bob_stats = report.authors.iter().find(|a| a.name == "Bob").unwrap();

        assert_eq!(alice_stats.additions, 5);
        assert_eq!(bob_stats.additions, 5);
        assert_eq!(alice_stats.deletions, 2);
        assert_eq!(bob_stats.deletions, 2);
    }

    #[test]
    fn attribute_squash_merge_same_author_multiple_commits() {
        // Alice has 3 commits in the same PR — should get prs=1, not 3.
        let mut acc = StatsAccumulator::default();
        let pr_deltas = vec![
            (alice(), vec![delta("a.rs", 30, 0)]),
            (alice(), vec![delta("b.rs", 40, 0)]),
            (alice(), vec![delta("c.rs", 30, 0)]),
        ];
        let squash_deltas = vec![delta("merged.rs", 100, 0)];
        acc.attribute_squash_merge(&pr_deltas, &squash_deltas);
        let report = acc.finalize();

        assert_eq!(report.authors.len(), 1);
        assert_eq!(report.authors[0].prs, 1);
        assert_eq!(report.authors[0].contributions, 1);
        assert_eq!(report.authors[0].additions, 100);
    }

    #[test]
    fn mark_pr_increments_only_prs() {
        let mut acc = StatsAccumulator::default();
        acc.attribute(&alice(), &[delta("a.rs", 10, 0)]);
        acc.mark_pr(&alice());
        let report = acc.finalize();
        assert_eq!(report.authors[0].contributions, 1);
        assert_eq!(report.authors[0].prs, 1);
        assert_eq!(report.authors[0].additions, 10);
    }

    #[test]
    fn finalize_sorts_by_total_descending() {
        let mut acc = StatsAccumulator::default();
        acc.attribute(&alice(), &[delta("a.rs", 5, 5)]); // total: 10
        acc.attribute(&bob(), &[delta("b.rs", 20, 10)]); // total: 30
        let report = acc.finalize();
        assert_eq!(report.authors[0].name, "Bob");
        assert_eq!(report.authors[1].name, "Alice");
    }
}

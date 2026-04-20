use std::path::Path;
use std::sync::LazyLock;

use git2::{Diff, DiffOptions, Repository, Revwalk, Sort};
use regex::Regex;

use crate::error::CreditError;

static PR_NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(#(\d+)\)").unwrap());

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Identifies a commit author.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Author {
    pub name: String,
    pub email: String,
}

/// A single file change in a diff.
#[derive(Debug, Clone)]
pub struct FileDelta {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
}

/// A processed commit with its diff stats.
#[derive(Debug)]
pub struct CommitInfo {
    pub oid: git2::Oid,
    pub author: Author,
    pub message: String,
    pub parent_count: usize,
    pub deltas: Vec<FileDelta>,
}

/// Options controlling the commit walk.
pub struct WalkOptions {
    pub rev_range: Option<String>,
    pub since: Option<i64>,
}

// ---------------------------------------------------------------------------
// Public functions
// ---------------------------------------------------------------------------

/// Open a git repository at the given path.
pub fn open_repo(path: &Path) -> Result<Repository, CreditError> {
    Repository::discover(path).map_err(|source| CreditError::RepoOpen {
        path: path.display().to_string(),
        source,
    })
}

/// Walk commits according to the given options, computing diffs for each.
pub fn walk_commits(repo: &Repository, opts: &WalkOptions) -> Result<Vec<CommitInfo>, CreditError> {
    let mut revwalk = setup_revwalk(repo, opts)?;
    let mut commits = Vec::new();

    for oid_result in &mut revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;

        if let Some(since) = opts.since {
            if commit.time().seconds() < since {
                continue;
            }
        }

        let sig = commit.author();
        let author = Author {
            name: sig.name().unwrap_or("Unknown").to_string(),
            email: sig.email().unwrap_or("unknown").to_string(),
        };
        let message = commit.message().unwrap_or("").to_string();
        let parent_count = commit.parent_count();
        let deltas = diff_commit(repo, &commit)?;

        commits.push(CommitInfo {
            oid,
            author,
            message,
            parent_count,
            deltas,
        });
    }

    Ok(commits)
}

/// Compute the diff stats for a single commit against its first parent
/// (or against an empty tree for root commits).
pub fn diff_commit(
    repo: &Repository,
    commit: &git2::Commit,
) -> Result<Vec<FileDelta>, CreditError> {
    let tree = commit.tree()?;
    let parent_tree = if commit.parent_count() > 0 {
        Some(commit.parent(0)?.tree()?)
    } else {
        None
    };

    let mut opts = DiffOptions::new();
    let diff: Diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;

    let mut deltas = Vec::new();

    for i in 0..diff.deltas().len() {
        let delta = diff.get_delta(i).expect("delta in range");
        let path = delta
            .new_file()
            .path()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();

        // Use the patch to count additions and deletions per file.
        let file_patch = git2::Patch::from_diff(&diff, i)?;
        if let Some(file_patch) = file_patch {
            let (_, adds, dels) = file_patch.line_stats()?;
            if adds > 0 || dels > 0 {
                deltas.push(FileDelta {
                    path,
                    additions: adds as u64,
                    deletions: dels as u64,
                });
            }
        }
    }

    Ok(deltas)
}

/// Extract a PR number from a commit message if it ends with `(#NNN)`.
pub fn extract_pr_number(message: &str) -> Option<u64> {
    let first_line = message.lines().next().unwrap_or("");
    PR_NUMBER_RE
        .captures_iter(first_line)
        .last()
        .and_then(|cap| cap[1].parse().ok())
}

/// Determine if a commit is a squash-merge candidate.
/// Returns the PR number if the commit has exactly one parent and contains
/// a `(#NNN)` reference in its message.
pub fn is_squash_merge(commit: &CommitInfo) -> Option<u64> {
    if commit.parent_count == 1 {
        extract_pr_number(&commit.message)
    } else {
        None
    }
}

/// Parse a `YYYY-MM-DD` date string into seconds since the Unix epoch
/// (midnight UTC).
pub fn parse_date_to_epoch(date_str: &str) -> Result<i64, CreditError> {
    parse_date_inner(date_str).ok_or_else(|| CreditError::InvalidDate {
        input: date_str.to_string(),
    })
}

fn parse_date_inner(date_str: &str) -> Option<i64> {
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return None;
    }
    let year: i64 = parts[0].parse().ok()?;
    let month: i64 = parts[1].parse().ok()?;
    let day: i64 = parts[2].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86400)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

fn setup_revwalk<'a>(repo: &'a Repository, opts: &WalkOptions) -> Result<Revwalk<'a>, CreditError> {
    let mut revwalk = repo.revwalk()?;
    revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;

    if let Some(ref range) = opts.rev_range {
        revwalk
            .push_range(range)
            .map_err(|source| CreditError::InvalidRevRange {
                range: range.clone(),
                source,
            })?;
    } else {
        revwalk.push_head()?;
    }

    Ok(revwalk)
}

/// Convert a civil date to days since the Unix epoch (1970-01-01).
/// Algorithm from Howard Hinnant's `chrono`-compatible date library.
#[allow(clippy::similar_names)]
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let yr = if month <= 2 { year - 1 } else { year };
    let era = yr.div_euclid(400);
    let year_of_era = yr.rem_euclid(400);
    let day_of_year = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_pr_number_standard() {
        assert_eq!(extract_pr_number("feat: add login (#42)"), Some(42));
    }

    #[test]
    fn extract_pr_number_no_match() {
        assert_eq!(extract_pr_number("no pr here"), None);
    }

    #[test]
    fn extract_pr_number_multiple_takes_last() {
        assert_eq!(extract_pr_number("fix: issue (#1) resolved (#2)"), Some(2));
    }

    #[test]
    fn extract_pr_number_non_numeric() {
        assert_eq!(extract_pr_number("(#abc)"), None);
    }

    #[test]
    fn extract_pr_number_multiline_uses_first_line() {
        assert_eq!(
            extract_pr_number("feat: add feature (#10)\n\nCo-authored-by: X"),
            Some(10)
        );
    }

    #[test]
    fn is_squash_merge_with_pr() {
        let commit = CommitInfo {
            oid: git2::Oid::zero(),
            author: Author {
                name: "Test".into(),
                email: "test@test.com".into(),
            },
            message: "feat: add thing (#42)".into(),
            parent_count: 1,
            deltas: vec![],
        };
        assert_eq!(is_squash_merge(&commit), Some(42));
    }

    #[test]
    fn is_squash_merge_merge_commit() {
        let commit = CommitInfo {
            oid: git2::Oid::zero(),
            author: Author {
                name: "Test".into(),
                email: "test@test.com".into(),
            },
            message: "Merge pull request #42".into(),
            parent_count: 2,
            deltas: vec![],
        };
        assert_eq!(is_squash_merge(&commit), None);
    }

    #[test]
    fn is_squash_merge_no_pr() {
        let commit = CommitInfo {
            oid: git2::Oid::zero(),
            author: Author {
                name: "Test".into(),
                email: "test@test.com".into(),
            },
            message: "just a regular commit".into(),
            parent_count: 1,
            deltas: vec![],
        };
        assert_eq!(is_squash_merge(&commit), None);
    }

    #[test]
    fn parse_date_valid() {
        // 2025-01-01 00:00:00 UTC = 1735689600
        let epoch = parse_date_to_epoch("2025-01-01").unwrap();
        assert_eq!(epoch, 1_735_689_600_i64);
    }

    #[test]
    fn parse_date_epoch() {
        let epoch = parse_date_to_epoch("1970-01-01").unwrap();
        assert_eq!(epoch, 0);
    }

    #[test]
    fn parse_date_invalid_format() {
        assert!(parse_date_to_epoch("2025/01/01").is_err());
        assert!(parse_date_to_epoch("not-a-date").is_err());
        assert!(parse_date_to_epoch("2025-13-01").is_err());
        assert!(parse_date_to_epoch("2025-01-32").is_err());
    }

    #[test]
    fn diff_commit_on_tempdir_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();

        // Configure author.
        let sig = git2::Signature::now("Test Author", "test@example.com").unwrap();

        // Create initial commit with one file.
        let blob = repo.blob(b"line1\nline2\n").unwrap();
        let mut builder = repo.treebuilder(None).unwrap();
        builder.insert("file.txt", blob, 0o100_644).unwrap();
        let tree_oid = builder.write().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let first_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();

        // Create second commit adding a line.
        let blob2 = repo.blob(b"line1\nline2\nline3\n").unwrap();
        let mut builder2 = repo.treebuilder(None).unwrap();
        builder2.insert("file.txt", blob2, 0o100_644).unwrap();
        let second_tree_oid = builder2.write().unwrap();
        let tree2 = repo.find_tree(second_tree_oid).unwrap();
        let first_commit = repo.find_commit(first_oid).unwrap();
        let second_oid = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "add line3",
                &tree2,
                &[&first_commit],
            )
            .unwrap();

        let second_commit = repo.find_commit(second_oid).unwrap();
        let deltas = diff_commit(&repo, &second_commit).unwrap();

        assert_eq!(deltas.len(), 1);
        assert_eq!(deltas[0].path, "file.txt");
        assert_eq!(deltas[0].additions, 1);
        assert_eq!(deltas[0].deletions, 0);
    }

    #[test]
    fn walk_commits_on_tempdir_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Alice", "alice@example.com").unwrap();

        // Create two commits.
        let blob1 = repo.blob(b"hello\n").unwrap();
        let mut tb1 = repo.treebuilder(None).unwrap();
        tb1.insert("file.txt", blob1, 0o100_644).unwrap();
        let tree1 = repo.find_tree(tb1.write().unwrap()).unwrap();
        let c1 = repo
            .commit(Some("HEAD"), &sig, &sig, "first", &tree1, &[])
            .unwrap();

        let blob2 = repo.blob(b"hello\nworld\n").unwrap();
        let mut tb2 = repo.treebuilder(None).unwrap();
        tb2.insert("file.txt", blob2, 0o100_644).unwrap();
        let tree2 = repo.find_tree(tb2.write().unwrap()).unwrap();
        let c1_commit = repo.find_commit(c1).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "second", &tree2, &[&c1_commit])
            .unwrap();

        let opts = WalkOptions {
            rev_range: None,
            since: None,
        };
        let commits = walk_commits(&repo, &opts).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].message, "second");
        assert_eq!(commits[1].message, "first");
        assert_eq!(commits[0].author.name, "Alice");
    }
}

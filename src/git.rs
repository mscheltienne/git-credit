use std::path::Path;
use std::sync::LazyLock;

use git2::{DiffFindOptions, DiffOptions, Mailmap, Repository, Revwalk, Sort};
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

impl Author {
    pub fn is_bot(&self) -> bool {
        is_bot_email(&self.email)
    }
}

pub fn is_bot_email(email: &str) -> bool {
    email.contains("[bot]@")
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
    /// Author time in epoch seconds (UTC), from `commit.author().when()`.
    /// Distinct from committer time — matches what `git log --format='%aI'` shows.
    pub author_time: i64,
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

/// Resolve an author through a mailmap, falling back to the original
/// name/email when no mailmap is provided or resolution fails.
pub fn resolve_author(mailmap: Option<&Mailmap>, name: &str, email: &str) -> Author {
    if let Some(mm) = mailmap
        && let Ok(sig) = git2::Signature::new(name, email, &git2::Time::new(0, 0))
        && let Ok(resolved) = mm.resolve_signature(&sig)
    {
        return Author {
            name: resolved.name().unwrap_or(name).to_string(),
            email: resolved.email().unwrap_or(email).to_string(),
        };
    }
    Author {
        name: name.to_string(),
        email: email.to_string(),
    }
}

/// Walk commits according to the given options, computing diffs for each.
pub fn walk_commits(
    repo: &Repository,
    opts: &WalkOptions,
    mailmap: Option<&Mailmap>,
) -> Result<Vec<CommitInfo>, CreditError> {
    let mut revwalk = setup_revwalk(repo, opts)?;
    let mut commits = Vec::new();

    for oid_result in &mut revwalk {
        let oid = oid_result?;
        let commit = repo.find_commit(oid)?;

        let sig = commit.author();
        let author_time = sig.when().seconds();

        if let Some(since) = opts.since
            && author_time < since
        {
            continue;
        }

        let author = resolve_author(
            mailmap,
            sig.name().unwrap_or("Unknown"),
            sig.email().unwrap_or("unknown"),
        );
        let message = commit.message().unwrap_or("").to_string();
        let parent_count = commit.parent_count();
        let deltas = diff_commit(repo, &commit)?;

        commits.push(CommitInfo {
            oid,
            author,
            author_time,
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
    let mut diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut opts))?;

    // Collapse delete+add pairs that look like a rename (≥50% similar, libgit2
    // default) into a single rename-delta with the in-place line stats. Without
    // this, a `git mv` shows up as `−N + N` for every renamed file's full
    // content, dwarfing the actual line edits in the commit. Matches the
    // behavior of `git diff -M`, which is git CLI's default.
    let mut find_opts = DiffFindOptions::new();
    find_opts.renames(true);
    diff.find_similar(Some(&mut find_opts))?;

    let mut deltas = Vec::new();

    for i in 0..diff.deltas().len() {
        let delta = diff.get_delta(i).expect("delta in range");
        let path = delta
            .new_file()
            .path()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();

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

/// Convert days since the Unix epoch (1970-01-01) back to a civil date.
/// Inverse of [`days_from_civil`]; same Howard Hinnant algorithm.
///
/// Returned `(year, month, day)` with `month ∈ 1..=12` and `day ∈ 1..=31`.
#[allow(clippy::similar_names)]
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let yr = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { yr + 1 } else { yr };
    (year, month, day)
}

/// Format an epoch-seconds (UTC) value as an ISO 8601 string with `Z` suffix
/// and second precision: `YYYY-MM-DDTHH:MM:SSZ`.
pub fn format_utc_iso8601(epoch: i64) -> String {
    let days = epoch.div_euclid(86_400);
    let seconds_of_day = epoch.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3600;
    let minute = (seconds_of_day % 3600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
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

    fn make_commit(message: &str, parent_count: usize) -> CommitInfo {
        CommitInfo {
            oid: git2::Oid::ZERO_SHA1,
            author: Author {
                name: "Test".into(),
                email: "test@test.com".into(),
            },
            author_time: 0,
            message: message.into(),
            parent_count,
            deltas: vec![],
        }
    }

    #[test]
    fn is_squash_merge_with_pr() {
        assert_eq!(
            is_squash_merge(&make_commit("feat: add thing (#42)", 1)),
            Some(42)
        );
    }

    #[test]
    fn is_squash_merge_merge_commit() {
        assert_eq!(
            is_squash_merge(&make_commit("Merge pull request #42", 2)),
            None
        );
    }

    #[test]
    fn is_squash_merge_no_pr() {
        assert_eq!(
            is_squash_merge(&make_commit("just a regular commit", 1)),
            None
        );
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

    /// A pure rename (identical content, just a different path) must not
    /// double-count the file content as `−N + N`. `find_similar` collapses
    /// the delete+add pair into a single rename delta with 0 additions and
    /// 0 deletions, matching `git diff -M`.
    #[test]
    fn diff_commit_pure_rename_has_zero_line_stats() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Test Author", "test@example.com").unwrap();

        // ~600 bytes of identical content in both commits — well above the
        // libgit2 default similarity floor, so this is unambiguously a rename.
        let content: Vec<u8> = (0..30)
            .flat_map(|i| format!("line {i}\n").into_bytes())
            .collect();
        let blob = repo.blob(&content).unwrap();

        let mut tb1 = repo.treebuilder(None).unwrap();
        tb1.insert("old.txt", blob, 0o100_644).unwrap();
        let tree1 = repo.find_tree(tb1.write().unwrap()).unwrap();
        let c1 = repo
            .commit(Some("HEAD"), &sig, &sig, "initial", &tree1, &[])
            .unwrap();

        let mut tb2 = repo.treebuilder(None).unwrap();
        tb2.insert("new.txt", blob, 0o100_644).unwrap();
        let tree2 = repo.find_tree(tb2.write().unwrap()).unwrap();
        let c1_commit = repo.find_commit(c1).unwrap();
        let c2 = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "rename file",
                &tree2,
                &[&c1_commit],
            )
            .unwrap();

        let deltas = diff_commit(&repo, &repo.find_commit(c2).unwrap()).unwrap();

        // A delta with both additions == 0 and deletions == 0 is dropped by
        // `diff_commit` (the `if adds > 0 || dels > 0` guard), so the pure
        // rename produces zero deltas — the strongest possible assertion that
        // we are not counting the file content as churn.
        assert!(
            deltas.is_empty(),
            "pure rename emitted deltas: {deltas:?} — find_similar didn't collapse them"
        );
    }

    /// A rename combined with an in-place edit reports only the edit's line
    /// stats, attributed to the new path. Without `find_similar` this would
    /// be `−full_old_size + full_new_size`, drastically inflating the totals.
    #[test]
    fn diff_commit_rename_with_edit_reports_in_place_stats() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Test Author", "test@example.com").unwrap();

        let original: Vec<u8> = (0..30)
            .flat_map(|i| format!("line {i}\n").into_bytes())
            .collect();
        let blob_old = repo.blob(&original).unwrap();
        let mut tb1 = repo.treebuilder(None).unwrap();
        tb1.insert("old.txt", blob_old, 0o100_644).unwrap();
        let tree1 = repo.find_tree(tb1.write().unwrap()).unwrap();
        let c1 = repo
            .commit(Some("HEAD"), &sig, &sig, "initial", &tree1, &[])
            .unwrap();

        // Append two lines at the new path — 2 additions, 0 deletions.
        let mut edited = original.clone();
        edited.extend_from_slice(b"new-line-a\nnew-line-b\n");
        let blob_new = repo.blob(&edited).unwrap();
        let mut tb2 = repo.treebuilder(None).unwrap();
        tb2.insert("new.txt", blob_new, 0o100_644).unwrap();
        let tree2 = repo.find_tree(tb2.write().unwrap()).unwrap();
        let c1_commit = repo.find_commit(c1).unwrap();
        let c2 = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                "rename + append",
                &tree2,
                &[&c1_commit],
            )
            .unwrap();

        let deltas = diff_commit(&repo, &repo.find_commit(c2).unwrap()).unwrap();

        assert_eq!(deltas.len(), 1, "expected single rename delta");
        assert_eq!(deltas[0].path, "new.txt", "attribution goes to new path");
        assert_eq!(deltas[0].additions, 2);
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
        let commits = walk_commits(&repo, &opts, None).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].message, "second");
        assert_eq!(commits[1].message, "first");
        assert_eq!(commits[0].author.name, "Alice");
    }

    #[test]
    fn walk_commits_with_mailmap() {
        let dir = tempfile::tempdir().unwrap();
        let repo = Repository::init(dir.path()).unwrap();
        let sig = git2::Signature::now("Alice Old", "alice-old@example.com").unwrap();

        let blob = repo.blob(b"hello\n").unwrap();
        let mut tb = repo.treebuilder(None).unwrap();
        tb.insert("file.txt", blob, 0o100_644).unwrap();
        let tree = repo.find_tree(tb.write().unwrap()).unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "first", &tree, &[])
            .unwrap();

        let mut mm = Mailmap::new().unwrap();
        mm.add_entry(
            Some("Alice New"),
            Some("alice-new@example.com"),
            Some("Alice Old"),
            "alice-old@example.com",
        )
        .unwrap();

        let opts = WalkOptions {
            rev_range: None,
            since: None,
        };
        let commits = walk_commits(&repo, &opts, Some(&mm)).unwrap();
        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].author.name, "Alice New");
        assert_eq!(commits[0].author.email, "alice-new@example.com");
    }

    #[test]
    fn resolve_author_without_mailmap() {
        let author = resolve_author(None, "Alice", "alice@example.com");
        assert_eq!(author.name, "Alice");
        assert_eq!(author.email, "alice@example.com");
    }

    #[test]
    fn format_utc_iso8601_epoch() {
        assert_eq!(format_utc_iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn format_utc_iso8601_round_numbers() {
        // 2025-01-01 00:00:00 UTC = 1735689600 (matches parse_date_valid above).
        assert_eq!(format_utc_iso8601(1_735_689_600), "2025-01-01T00:00:00Z");
    }

    #[test]
    fn format_utc_iso8601_end_of_day() {
        assert_eq!(format_utc_iso8601(1_735_775_999), "2025-01-01T23:59:59Z");
    }

    #[test]
    fn format_utc_iso8601_leap_day() {
        // 2020-02-29 00:00:00 UTC = 1582934400.
        assert_eq!(format_utc_iso8601(1_582_934_400), "2020-02-29T00:00:00Z");
    }

    #[test]
    fn format_utc_iso8601_pre_epoch() {
        // 1969-12-31 00:00:00 UTC = -86400.
        assert_eq!(format_utc_iso8601(-86_400), "1969-12-31T00:00:00Z");
    }

    #[test]
    fn format_utc_iso8601_mid_day() {
        // 2026-04-17 14:23:51 UTC.
        let epoch = days_from_civil(2026, 4, 17) * 86_400 + 14 * 3600 + 23 * 60 + 51;
        assert_eq!(format_utc_iso8601(epoch), "2026-04-17T14:23:51Z");
    }
}

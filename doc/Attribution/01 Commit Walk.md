---
aliases:
  - Commit Walk
tags:
  - attribution
---
The walk visits the requested commits once, drops merge commits and commits authored before `--since`, and turns every other commit into a `CommitInfo`: its SHA, its mailmapped author, its author time, its PR number if it is a squash-merge candidate, and its line totals over the non-excluded files.

```rust
// src/git.rs — walk_commits (abridged)
let mut revwalk = repo.revwalk()?;
revwalk.set_sorting(Sort::TOPOLOGICAL | Sort::TIME)?;
if let Some(range) = rev_range {
    revwalk.push_range(range)?; // mapped to CreditError::InvalidRevRange
} else {
    revwalk.push_head()?;
}
for oid in revwalk {
    let commit = repo.find_commit(oid?)?;
    let sig = commit.author();
    let author_time = sig.when().seconds();
    if commit.parent_count() > 1 || since.is_some_and(|since| author_time < since) {
        continue;
    }
    let pr_number = extract_pr_number(commit.message().unwrap_or(""));
    let (additions, deletions) = filter.line_totals(&diff_commit(repo, &commit)?);
    commits.push(CommitInfo { /* oid, resolve_author(mailmap, …), author_time, pr_number, … */ });
}
```

## Which commits are walked

- **`HEAD` and all its ancestors by default.** Without `--rev`, the walk pushes `HEAD`.
- **`--rev A..B` walks the commits reachable from `B` but not from `A`.**
  - The value goes to libgit2's `push_range`, which accepts only the two-dot form. A single revision (`HEAD~50`) fails with "range not provided", and the three-dot form (`A...B`) fails with "symmetric differences not implemented in revwalk"; both surface as the fatal `invalid revision range` error.
  - `--rev <sha>^..<sha>` selects exactly one commit.
- **Order is topological, then by commit time.** The order matters only for the walk itself; the report is re-sorted by author date before output — [Pipeline](../Overview/02%20Pipeline.md).

### Merge commits are skipped

A [merge commit](../Glossary/Merge%20commit.md), one with more than one parent, is never reported.

- **Its diff against the first parent repeats the merged branch's lines**, which the walk already credits to the branch commits' own authors. Counting the merge too would credit those lines twice: once to their authors, once to whoever merged.
- **The same rule applies inside a PR.** When GitHub lists a PR's commits, merge commits (such as the base branch merged into the PR) are dropped before weighing — [PR Expansion and Split](03%20PR%20Expansion%20and%20Split.md#listing-the-pr-commits).
- **The cost:** edits made while resolving a merge conflict live only in the merge commit, so nobody is credited for them.

### The `--since` cut-off

- **The date is `YYYY-MM-DD` in UTC, and the cut-off is inclusive.**
  - `parse_date_to_epoch` converts it to seconds since the Unix epoch at midnight UTC; commits whose author time is earlier are skipped, commits at or after it are kept.
- **Author time, not committer time.** A rebased or cherry-picked commit keeps its original author date, so it is filtered by when it was written.
- **Impossible dates are rejected.** The parser converts the date to a day number and back; a date such as `2025-02-30` or `2025-02-29` does not survive the round trip and fails with `invalid --since date`, instead of silently rolling over into March.
  - The input must be three `-`-separated integers; anything else fails the same way.
- **The walk does not stop early.** Because a topological walk does not visit commits in author-date order, every commit is visited and tested; `--since` filters, it does not shorten the walk.

## Diffing a commit

`diff_commit` diffs the commit's tree against its first parent's tree, or against the empty tree for a root commit, and returns one `FileDelta { path, additions, deletions }` per changed file.

- **Rename detection is on.** `find_similar` with renames enabled pairs a deleted and an added file that are at least 50% similar (libgit2's default threshold, as with `git diff -M`).
  - A pure rename then yields no line changes, and a rename with edits yields only the edited lines, under the new path.
  - Copies are not detected; a copied file counts as all-added.
- **Files without added or deleted lines are omitted**: binary files, mode-only changes and pure renames.
- **The path is the new-side path**, which is what [exclusion globs](../Glossary/Exclusion%20glob.md) match against — [File Exclusions](07%20File%20Exclusions.md).
- **Totals are summed immediately.** `ExclusionFilter::line_totals` drops excluded paths and sums the rest, so a `CommitInfo` carries two numbers, not a file list.

## The commit's author

- **The git author is credited, never the committer.** The author's name and email go through [`resolve_author`](05%20Mailmap%20and%20Identities.md#resolving-one-identity) with the loaded mailmap, if any.
- **Missing or non-UTF-8 identity fields** become `Unknown` (name) and `unknown` (email).
- **The author time is kept as epoch seconds** and later formatted as the JSON `author_date`, in UTC — [JSON Output](../Usage/02%20JSON%20Output.md).

## The PR number

Each walked commit's message is scanned for a squash-merge marker; a match stores the PR number in `CommitInfo.pr_number` — [Squash-Merge Detection](02%20Squash-Merge%20Detection.md).

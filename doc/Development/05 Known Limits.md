---
aliases:
  - Known Limits
tags:
  - attribution
  - github-api
---
git-credit's numbers are estimates with known blind spots. Each limit below follows from the code at the linked section; none is a bug to be fixed by accident, but each is a place a future change could improve.

## GitHub API caps

- **At most 250 commits per PR.** GitHub's PR-commits endpoint stops listing there, so the later commits of a longer PR weigh nothing — [PR Expansion and Split](../Attribution/03%20PR%20Expansion%20and%20Split.md#listing-the-pr-commits).
- **At most 300 files per PR commit.** `fetch_commit_files` reads only the first page of a commit's files, so a PR commit touching more files is undercounted in the weights — [PR Expansion and Split](../Attribution/03%20PR%20Expansion%20and%20Split.md#fetching-a-commits-files).
- **No retries or backoff.** One failed request fails the whole PR, which falls back with `accurate: false`; a rate limit stops all later lookups of the run — [Fallback and the Accurate Flag](../Attribution/04%20Fallback%20and%20the%20Accurate%20Flag.md#the-rate-limit-short-circuit).
- **The short-circuit is per PR.** Requests already in flight, including the per-commit file fetches of a PR being weighed, still go out after the first rate-limit response.

## Range and date selection

- **`--rev` accepts only `A..B`.** A single revision or a three-dot range is a fatal error; to walk from a revision to the root, omit `--rev` or check that revision out — [Commit Walk](../Attribution/01%20Commit%20Walk.md#which-commits-are-walked).
- **`--since` does not shorten the walk.** Every commit in the range is still visited, though older ones are not diffed; on a large history, `--rev` is the way to bound the work.
- **Dates are whole UTC days.** There is no time-of-day or time-zone option.

## Squash-merge detection

- **The `(#N)` rule is a heuristic** — [Squash-Merge Detection](../Attribution/02%20Squash-Merge%20Detection.md#false-positives).
  - Reverts and cherry-picks of a squash merge are credited to the original PR's authors.
  - An issue number in a subject makes a permanent `accurate: false` row.
- **Only `origin` on github.com.** Another remote name, GitHub Enterprise, or an aliased host disables expansion with a warning — [CLI Reference](../Usage/01%20CLI%20Reference.md#remote-url-parsing).
- **Rebase-merged and merge-committed PRs need no expansion**, since their commits keep their own authors; but a merge commit's conflict-resolution edits are credited to nobody.

## Split accuracy

- **Weights are lines changed per PR commit**, not lines surviving in the squash: an author whose lines were later rewritten in the same PR still weighs by what they wrote.
- **Rounding loses lines.** Shares round down, so an expanded squash merge's attributions can sum to slightly less than the commit — [PR Expansion and Split](../Attribution/03%20PR%20Expansion%20and%20Split.md#the-proportional-split).
- **Two diff engines.** Commit totals come from libgit2's local diff with rename detection; PR weights from GitHub's per-commit stats. They can disagree on renames and binary files; only the ratios of the weights matter, so the effect is limited to the split.
- **`Co-authored-by` trailers are ignored.** Pair-programmed commits credit only their git author.

## Identities

- **libgit2 matches mailmap entries case-sensitively**, unlike `git`, so an entry must use the exact capitalisation of the recorded email — [Mailmap and Identities](../Attribution/05%20Mailmap%20and%20Identities.md#case-sensitivity).
- **JSON rows keep each commit's email spelling**; consumers merge case variants themselves.
- **Bot detection is the literal substring `[bot]@`**: bot accounts with other address shapes are counted as people — [Bots](../Attribution/06%20Bots.md).

## Output

- **The table's summary line does not pluralise** (`1 authors`) — [Table Output](../Usage/03%20Table%20Output.md#summary-line).
- **Non-UTF-8 paths** are matched against exclusion globs as the empty string, so only a glob that matches an empty path (`*`, `**`) excludes them, and it then excludes all of them (inferred from `diff_commit` falling back to `""` when a path is not valid UTF-8).

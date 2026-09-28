---
aliases:
  - Squash-Merge Detection
tags:
  - attribution
---
git-credit treats a walked commit as a [squash-merge candidate](../Glossary/Squash-merge%20candidate.md) when the first line of its message contains `(#N)`; `N` is the pull request to expand. The rule matches the subject GitHub writes for a squash merge — the PR title followed by ` (#N)` — and nothing else about the commit is checked.

```rust
// src/git.rs
static PR_NUMBER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(#(\d+)\)").unwrap());

/// PR number from the last `(#NNN)` on the message's first line.
pub fn extract_pr_number(message: &str) -> Option<u64> {
    let first_line = message.lines().next().unwrap_or("");
    PR_NUMBER_RE
        .captures_iter(first_line)
        .last()
        .and_then(|cap| cap[1].parse().ok())
}
```

## The matching rule

- **Only the first line counts.** A `(#N)` in the body — a reference to a related PR, say — is ignored.
- **The last match wins.** GitHub appends the PR number after the title, so when the title itself mentions another PR, the trailing one is the squash merge's.
- **Digits only.** `(#abc)` does not match; a number too large for `u64` does not either.

| First line | PR number |
| --- | --- |
| `feat: add login (#42)` | 42 |
| `fix: issue (#1) resolved (#2)` | 2 |
| `no pr here` | none |
| `(#abc)` | none |
| `feat: add feature (#10)` followed by a body | 10 |
| `feat: add feature`, body contains `See (#10)` | none |

These cases are the table-driven unit test `extract_pr_number_from_first_line`.

## What a candidate becomes

The candidate flag only records a PR number; what happens to the commit depends on whether the run has GitHub access.

| GitHub access | Outcome | `is_squash_pr` | `accurate` |
| --- | --- | --- | --- |
| Client available, PR fetched | Split across the PR's authors — [PR Expansion and Split](03%20PR%20Expansion%20and%20Split.md) | `true` | `true` |
| Client available, PR lookup failed | Credited to the squash commit's author — [Fallback and the Accurate Flag](04%20Fallback%20and%20the%20Accurate%20Flag.md) | `false` | `false` |
| No client (`--no-github`, no token, no GitHub `origin`) | Reported as a regular commit | `false` | `true` |

## False positives

The rule is a heuristic, so some commits that are not squash merges also match:

- **Reverts and cherry-picks of a squash merge** keep the `(#N)` in their subject (`Revert "feat: add login (#42)"`), so they are expanded against PR #42 and credited to its authors rather than to whoever reverted or cherry-picked.
- **A subject citing an issue** (`fix crash (#123)` where #123 is an issue) makes the PR lookup return 404, so the commit falls back with `accurate: false`, and retrying never changes that.
- **A PR number from another repository** — a commit imported from a fork or a monorepo split — is looked up in the `origin` repository, where it names an unrelated PR or none.

Merge commits never become candidates: they are skipped before the message is read — [Commit Walk](01%20Commit%20Walk.md#merge-commits-are-skipped).

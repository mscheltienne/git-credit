---
aliases:
  - Fallback and the Accurate Flag
tags:
  - attribution
  - github-api
---
When a squash merge's PR cannot be expanded, git-credit still reports the commit, credited to the squash commit's own author, and marks the row `accurate: false`. The [accurate flag](../Glossary/Accurate%20flag.md) exists so that a consumer can tell a trustworthy row from a stopgap and recompute the stopgaps later.

## When a row is accurate

| Commit | GitHub access | `is_squash_pr` | Attributions | `accurate` |
| --- | --- | --- | --- | --- |
| Regular commit | Any | `false` | Its author, `is_pr_author: false` | `true` |
| Squash-merge candidate, PR expanded | Client available | `true` | One per PR author, `is_pr_author: true` | `true` |
| Squash-merge candidate, lookup failed | Client available | `false` | The squash commit's author, `is_pr_author: false` | `false` |
| Squash-merge candidate | No client | `false` | The squash commit's author, `is_pr_author: false` | `true` |

- **A failed lookup is marked; missing access is not.** `accurate: false` means "this commit should have been expanded and was not". A run without GitHub access never tries, and the doc comment on `CommitReport` states that such rows are regular commits with `accurate: true`.
  - A run has no client with `--no-github`, when no token resolves, or when `origin` is missing or not a github.com URL — [CLI Reference](../Usage/01%20CLI%20Reference.md#github-access).
- **The fallback row looks like a regular commit.** [Fallback attribution](../Glossary/Fallback%20attribution.md) uses the same `author_report` builder as a regular commit: the commit's full totals, the walk's mailmapped author, `is_squash_pr: false`, `is_pr_author: false`. Only `accurate` tells them apart.

## What counts as a failed lookup

Any error from `fetch_pr_weights` falls back:

| Error | Cause |
| --- | --- |
| `CreditError::RateLimited` | A rate-limit response, or the lookup was skipped after an earlier one — [[#Detecting a rate limit]] |
| `CreditError::GitHubApi { status, body }` | Any other non-success status: 404 for a number that is not a PR of `origin`, 401 for a bad token, 403 without rate-limit signals for a token that cannot read the repository |
| `CreditError::GitHubRequest` | A `reqwest` failure: network error, the 30-second timeout, or a response body that does not parse as the expected JSON |
| `CreditError::EmptyPr` | The PR lists no non-merge commits |

Only the first failure's message is printed, as `warning: GitHub API error for PR #N: …`; at the end, one more warning gives the number of PRs that fell back to commit-author attribution.

### Detecting a rate limit

```rust
// src/github.rs
/// GitHub signals rate limiting with a 429, or a 403 carrying an exhausted quota or
/// a `retry-after` header; any other 403 is a permission error.
fn is_rate_limited(status: u16, headers: &HeaderMap) -> bool {
    status == 429
        || status == 403
            && (headers.contains_key(RETRY_AFTER)
                || headers
                    .get("x-ratelimit-remaining")
                    .is_some_and(|v| v == "0"))
}
```

- **A 429 is always a rate limit.**
- **A 403 is a rate limit only with a signal**: an `x-ratelimit-remaining: 0` header (the primary quota is spent) or a `retry-after` header (a secondary rate limit). A bare 403 is a permission error and stays a `GitHubApi` error, so one unreadable PR does not stop the others.

### The rate-limit short-circuit

Once one lookup is rate-limited, the others would be too, so git-credit stops asking — the [rate-limit short-circuit](../Glossary/Rate-limit%20short-circuit.md).

```rust
// src/lib.rs — expand_squash_merges (abridged)
let rate_limited = AtomicBool::new(false);
// in the parallel map, per squash-merge candidate:
let result = if rate_limited.load(Ordering::Relaxed) {
    Err(CreditError::RateLimited)
} else {
    fetch_pr_weights(client, pr_number, filter)
};
if matches!(result, Err(CreditError::RateLimited)) {
    rate_limited.store(true, Ordering::Relaxed);
}
```

- **Lookups that start after the flag is set make no request** and fall back at once, with `accurate: false`.
- **Lookups already in flight finish.** The flag is checked once per PR, before `fetch_pr_weights`; per-commit file fetches inside a PR that is already running do not check it.
- **Other errors do not set the flag.**

## Retrying inaccurate rows

A consumer that stores the JSON rows can recompute the `accurate: false` ones once GitHub is reachable again — for example with `--rev <sha>^..<sha>` for each such commit, replacing the stored row with the new one. A row that failed for a permanent reason, such as a `(#N)` that names an issue rather than a PR, stays inaccurate however often it is retried — [Squash-Merge Detection](02%20Squash-Merge%20Detection.md#false-positives).

---
aliases:
  - Testing
tags:
  - testing
---
`cargo test` runs two layers: unit tests inside each source module, and integration tests that run the compiled binary against throwaway repositories. Neither touches the network: the GitHub side is tested through a mock of the `GitHubApi` trait, and the binary tests pass `--no-github`.

## Unit tests

Each module in `src/` ends with a `#[cfg(test)] mod tests`, so unit tests can reach private functions.

| Module | Covers |
| --- | --- |
| `cli` | Defaults, repeated `--exclude`, most options parsed in one invocation, `--no-mailmap` alone and conflicting with `--mailmap-file` |
| `git` | PR-number extraction (table-driven), `--since` parsing and impossible dates, timestamp formatting (table-driven), diffs on temporary repositories — a plain edit, a pure rename, a rename with an edit — and walks: order and PR numbers, merge commits skipped, mailmap applied |
| `github` | Token precedence and empty-value skipping, the lazy `gh` call, merge commits dropped from a PR commit list, rate-limit detection, remote URL parsing |
| `filter` | Glob translation: `*`, `**`, `?`, directory prefixes, several patterns, the excluded-line totals |
| `stats` | The proportional split and its ordering, the equal-split fallback, case-insensitive author merging, bot stripping, the rollup's sort and PR counting |
| `output` | Thousands separators, the JSON shape |
| `lib` | Squash-merge expansion end to end through the mock API — [[#The mock GitHub API]] |

### Temporary repositories

Tests that need git history build it with `git2` directly: `Repository::init` in a `tempfile::tempdir()`, blobs and trees through `treebuilder`, and commits with explicit signatures and parents. No `git` binary is needed, and author times are fixed, so dates and order are deterministic.

### The mock GitHub API

`lib.rs`'s tests implement the [GitHub API trait](../Glossary/GitHub%20API%20trait.md) with an in-memory `MockApi`:

```rust
// src/lib.rs — tests (abridged)
#[derive(Default)]
struct MockApi {
    prs: HashMap<u64, Vec<(Author, String)>>,   // PR number → (author, sha) per commit
    files: HashMap<String, Vec<FileDelta>>,      // sha → file deltas
}

impl GitHubApi for MockApi {
    fn fetch_pr_commits(&self, pr_number: u64) -> Result<Vec<(Author, String)>, CreditError> {
        self.prs.get(&pr_number).cloned().ok_or(CreditError::GitHubApi {
            status: 404,
            body: "Not Found".into(),
        })
    }
    fn fetch_commit_files(&self, sha: &str) -> Result<Vec<FileDelta>, CreditError> {
        Ok(self.files.get(sha).cloned().unwrap_or_default())
    }
}
```

- **`MockApi::default().pr(n, &[(author, sha, files), …])`** registers a PR and its commits' files in one builder call.
- **An unregistered PR is a 404**, which exercises the fallback path; a PR registered with no commits exercises `EmptyPr`.
- **The `expand` helper** calls `expand_squash_merges` on one synthetic squash commit authored by `Merger <merger@example.com>`, with the given exclusion globs and no mailmap, and returns its report.
- **The cases**: a two-author proportional split, excluded files not weighing on the split, case-insensitive author merging (including the single-author fast path), and fallback to the squash author for an unknown or empty PR.

### Injected token sources

`resolve_token_from_sources` takes the flag and both environment values as arguments and the `gh` lookup as a closure, so the tests never read the real environment or spawn `gh`. The closure `unreachable_cli` panics if called, which proves `gh` runs only when the other sources are empty.

## Integration tests

[`tests/cli.rs`](../../tests/cli.rs) runs the built binary with `assert_cmd` and checks exit status, stdout and stderr.

- **Fixtures** in [`tests/common/mod.rs`](../../tests/common/mod.rs):
  - `create_test_repo` — three commits by Alice and Bob on fixed, increasing author dates, including a `data.lock` file for the exclusion test; `tests/cli.rs` asserts the date order at compile time.
  - `create_repo_with_unmapped_alice` — one commit by `Alice Old <alice-old@example.com>`, for the mailmap tests.
- **Cases**: `--help` and `--version`; table output; JSON shape and date order; `--exclude` lowering the totals; `--mailmap-file` overriding an in-repository `.mailmap`; `--no-mailmap` ignoring it; a missing and an unparsable mailmap file; a nonexistent repository; an invalid `--since`.
- **Every integration test passes `--no-github`**, so a developer's token is never used.

## What is not tested

- **`GitHubClient` itself**: the HTTP requests, pagination, JSON decoding and the 30-second timeout have no test; only the pure `is_rate_limited` and `non_merge_authors` helpers do.
- **The rate-limit short-circuit** across several PRs.
- **Mailmap resolution of PR authors**: the mock tests pass no mailmap.
- **Bare repositories and `mailmap.blob` / `mailmap.file`**, which are libgit2's behaviour.
- **The progress bar and the warning text.**

When you change the split, the fallback or the weights, extend the `MockApi` cases; when you change a flag, add both a `cli` parse test and, if it changes output, an integration test.

## Coverage

CI runs `cargo llvm-cov --all-features --workspace --lcov` and uploads to Codecov — [Linting and CI](03%20Linting%20and%20CI.md#ci-workflow). [`.codecov.yaml`](../../.codecov.yaml) sets project and patch targets as informational only, ignores `tests/`, and posts no PR comment, so coverage never blocks a merge.

# git-credit

[![CI](https://github.com/mscheltienne/git-credit/actions/workflows/ci.yaml/badge.svg?branch=main)](https://github.com/mscheltienne/git-credit/actions/workflows/ci.yaml)
[![codecov](https://codecov.io/gh/mscheltienne/git-credit/graph/badge.svg?token=ez0tTYjMnY)](https://codecov.io/gh/mscheltienne/git-credit)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![MSRV](https://img.shields.io/badge/MSRV-1.88-blue.svg)](Cargo.toml)
[![pre-commit.ci](https://results.pre-commit.ci/badge/github/mscheltienne/git-credit/main.svg)](https://results.pre-commit.ci/latest/github/mscheltienne/git-credit/main)

> Give credit where it's due -- precise per-author contribution stats that see
> through squash merges.

A contribution analysis tool that accurately attributes lines of code to
individual authors, even across squash-merged pull requests, with support for
file exclusion filters.

## Installation

### Homebrew (macOS / Linux)

```sh
brew tap mscheltienne/tap
brew install mscheltienne/tap/git-credit
```

### From crates.io

```sh
cargo install git-credit
```

### Pre-built binaries

Pre-built binaries for Linux, macOS, and Windows are available on the
[GitHub Releases](https://github.com/mscheltienne/git-credit/releases) page.

### From source

```sh
git clone https://github.com/mscheltienne/git-credit.git
cd git-credit
cargo install --path .
```

## Usage

```sh
# Analyze the current repository
git-credit

# Analyze a specific repository
git-credit --repo /path/to/repo

# Limit to the last 50 commits
git-credit --rev HEAD~50..HEAD

# Only include commits after a date
git-credit --since 2025-01-01

# Exclude files from stats (repeatable)
git-credit --exclude "*.lock" --exclude "*.generated.*"

# Output as JSON instead of a table
git-credit --format json

# Include bot accounts (dependabot, pre-commit-ci, etc. are excluded by default)
git-credit --bots

# Use an external .mailmap (overrides any .mailmap inside the repository)
git-credit --mailmap-file /path/to/.mailmap

# Skip GitHub API lookups (faster, but squash merges are attributed to the merge author
# only)
git-credit --no-github
```

### Mailmap

By default git-credit applies the repository's `.mailmap` (or the file
referenced by `mailmap.file` in git config) to canonicalize author identities.
Pass `--mailmap-file <PATH>` to use an external file instead — useful when
invoking git-credit against a clone you don't want to mutate, or when
aggregating mailmap entries across many repositories outside git-credit.

The external mailmap **replaces** the repo's own; the two are not merged.

### Bot filtering

Bot accounts (identified by `[bot]@` in their email address) are excluded from
the output by default. Use `--bots` to include them. When bots are excluded,
the summary line shows how many were filtered.

### GitHub authentication

To attribute squash-merged PRs to individual authors, git-credit needs a
GitHub token. It resolves the token in this order:

1. `--token` flag
2. `GITHUB_TOKEN` environment variable
3. `GH_TOKEN` environment variable
4. `gh auth token` (the [GitHub CLI](https://cli.github.com/))

If no token is found, git-credit runs in `--no-github` mode automatically
with a warning.

### Example table output

```text
╭──────────────────────────┬───────────────┬─────┬───────┬─────┬───────╮
│ Author                   ┆ Contributions ┆ PRs ┆ +     ┆ -   ┆ Total │
╞══════════════════════════╪═══════════════╪═════╪═══════╪═════╪═══════╡
│ Alice <alice@example.com>┆ 12            ┆ 8   ┆ 1,542 ┆ 389 ┆ 1,931 │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌┼╌╌╌╌╌╌╌┤
│ Bob <bob@example.com>    ┆ 8             ┆ 5   ┆ 876   ┆ 201 ┆ 1,077 │
╰──────────────────────────┴───────────────┴─────┴───────┴─────┴───────╯

2 authors (3 bots excluded), 20 commits walked, 13 squash merges expanded
```

### JSON output

`--format json` emits one record per processed commit. For squash-merge PRs
`attributions` carries one entry per re-attributed author; for regular commits
the array has a single entry. `commits[]` is sorted by `author_date` ascending,
with `sha` as a tie-breaker.

Each commit also carries an `accurate` flag. It is `true` for normally-processed
commits (regular commits and successfully-expanded squash-merge PRs) and `false`
when a squash-merge could not be expanded — e.g. the GitHub API rate-limited
the run, returned an error, or no token was available — and the commit fell
back to merge-author attribution. Consumers can use this flag to retry later
when the API recovers.

```json
{
  "commits": [
    {
      "sha": "abc1234567890abcdef1234567890abcdef12345",
      "author_date": "2026-04-17T14:23:51Z",
      "is_squash_pr": false,
      "attributions": [
        {
          "name": "Alice Smith",
          "email": "alice@example.com",
          "additions": 42,
          "deletions": 7,
          "is_pr_author": false
        }
      ],
      "accurate": true
    },
    {
      "sha": "def987...",
      "author_date": "2026-04-18T09:12:00Z",
      "is_squash_pr": true,
      "attributions": [
        {
          "name": "Alice Smith",
          "email": "alice@example.com",
          "additions": 75,
          "deletions": 12,
          "is_pr_author": true
        },
        {
          "name": "Bob Jones",
          "email": "bob@example.com",
          "additions": 25,
          "deletions": 4,
          "is_pr_author": true
        }
      ],
      "accurate": true
    },
    {
      "sha": "fed321...",
      "author_date": "2026-04-19T16:05:22Z",
      "is_squash_pr": false,
      "attributions": [
        {
          "name": "Alice Smith",
          "email": "alice@example.com",
          "additions": 50,
          "deletions": 10,
          "is_pr_author": false
        }
      ],
      "accurate": false
    }
  ],
  "summary": {
    "total_commits_walked": 3,
    "squash_merges_expanded": 1,
    "bots_excluded": 0
  }
}
```

In the example above, the third commit was a squash-merge whose PR expansion
failed (rate limit, API error, or missing token); git-credit emits it with
`is_squash_pr: false`, the merge committer as the sole attribution, and
`accurate: false` so the caller knows to retry.

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (MSRV: 1.88)
- [pre-commit](https://pre-commit.com/)
- [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) (optional, for
  dependency auditing)
- [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) (optional, for
  coverage)

### Setup

```sh
git clone https://github.com/mscheltienne/git-credit.git
cd git-credit
pre-commit install
cargo build
```

### Commands

```sh
cargo build              # Build
cargo test               # Run all tests
cargo clippy             # Lint
cargo fmt                # Format
cargo deny check         # Audit dependencies
cargo llvm-cov           # Coverage report
```

## License

[MIT](LICENSE)

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

## How it works

1. **Walk.** git-credit walks the history (`HEAD`, or `--rev A..B`) and diffs
   each commit against its parent, with rename detection, counting only the
   files not matched by `--exclude`. Merge commits are skipped: their lines
   are already credited through the commits they merge.
2. **Detect squash merges.** A commit whose first line contains `(#N)`, as
   GitHub writes it when squash-merging PR `#N`, is a squash-merge candidate.
3. **Expand through GitHub.** For each candidate, git-credit lists the PR's
   commits (merge commits skipped) and splits the squash commit's lines among
   their authors, in proportion to the non-excluded lines each author changed
   in the PR. Emails are compared case-insensitively, so one author committing
   under two spellings is credited once. Shares are rounded down.
4. **Fall back.** If the PR can't be fetched, the squash commit is credited to
   its own author and flagged `accurate: false` so a consumer can retry later.

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

# Limit to a commit range (A..B only)
git-credit --rev HEAD~50..HEAD

# Only include commits authored on or after a date (midnight UTC)
git-credit --since 2025-01-01

# Exclude files from stats (repeatable); `*` stays within one directory,
# `**/` spans directories
git-credit --exclude "**/*.lock" --exclude "docs/**"

# Output as JSON instead of a table
git-credit --format json

# Include bot accounts (dependabot, pre-commit-ci, etc. are excluded by default)
git-credit --bots

# Use an external .mailmap (overrides any .mailmap inside the repository)
git-credit --mailmap-file /path/to/.mailmap

# Skip mailmap resolution entirely and report identities as recorded
git-credit --no-mailmap

# Skip GitHub API lookups (faster, but each squash merge is then credited to its
# own author)
git-credit --no-github
```

### Mailmap

By default git-credit applies the repository's mailmap, which libgit2 builds
from the worktree `.mailmap`, the `mailmap.blob` config and the `mailmap.file`
config. Pass `--mailmap-file <PATH>` to use an external file instead — useful
when invoking git-credit against a clone you don't want to mutate, or when
aggregating mailmap entries across many repositories outside git-credit.

The external mailmap **replaces** the repository's; the two are not merged.

Pass `--no-mailmap` to skip mailmap resolution entirely, for both the git
commit authors and the PR commit authors fetched from GitHub. The JSON output
then carries the identities as recorded, which suits a consumer that applies
the mailmap at read time. `--no-mailmap` and `--mailmap-file` are mutually
exclusive.

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

If no token is found, or the `origin` remote is not on GitHub, git-credit runs
in `--no-github` mode automatically with a warning.

When GitHub rate-limits the run (a 429, or a 403 with an exhausted quota),
git-credit stops calling the API and every remaining squash merge falls back
to `accurate: false`.

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

`--format json` emits one record per walked commit (merge commits are not
walked). For a squash merge expanded through GitHub (`is_squash_pr: true`),
`attributions` carries one entry per PR author, each with `is_pr_author: true`;
otherwise the array has a single entry. `commits[]` is sorted by `author_date`
(UTC, `YYYY-MM-DDTHH:MM:SSZ`) ascending, with `sha` as a tie-breaker.

Each commit also carries an `accurate` flag. It is `false` when a squash merge
could not be expanded because the GitHub API rate-limited the run, returned an
error, or listed no commits for the PR. The commit is then credited to the
squash commit's own author, and consumers can retry it once the API recovers.
Every other commit is `accurate: true`, including squash merges reported as
regular commits because GitHub lookups were off (`--no-github`, no token, or
no GitHub remote).

The `summary` counts the commits reported before bot filtering, the squash
merges expanded, and the distinct bot emails removed.

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

In the example above, the third commit was a squash merge whose PR expansion
failed; git-credit emits it with `is_squash_pr: false`, the squash commit's
author as the sole attribution, and `accurate: false` so the caller knows to
retry.

### Limits

- GitHub lists at most 250 commits per PR, so later commits of a larger PR
  don't weigh on the split.
- Only the first page of files (up to 300) of each PR commit is read, so the
  weight of a larger commit is undercounted.
- `--rev` accepts only a range (`A..B`), not a single revision or `A...B`.

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
cargo build                                  # Build
cargo test                                   # Run all tests
cargo clippy --all-targets -- -D warnings    # Lint, as CI does
cargo fmt                                    # Format
pre-commit run --all-files                   # typos, yamllint, fmt, clippy
cargo deny check                             # Audit dependencies
cargo llvm-cov                               # Coverage report
```

### Documentation

[`doc/`](doc/README.md) is an [Obsidian](https://obsidian.md/) vault covering
the attribution pipeline, the CLI and JSON contract, and development in more
depth. Open the folder as a vault in Obsidian, or read the Markdown directly.

## License

[MIT](LICENSE)

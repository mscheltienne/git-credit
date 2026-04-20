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

# Skip GitHub API lookups (faster, but squash merges are attributed to the merge author
# only)
git-credit --no-github
```

### GitHub authentication

To attribute squash-merged PRs to individual authors, git-credit needs a
GitHub token. It resolves the token in this order:

1. `--token` flag
2. `GITHUB_TOKEN` environment variable
3. `GH_TOKEN` environment variable
4. `gh auth token` (the [GitHub CLI](https://cli.github.com/))

If no token is found, git-credit runs in `--no-github` mode automatically
with a warning.

### Example output

```text
╭──────────────────────────┬───────────────┬─────┬───────┬─────┬───────╮
│ Author                   ┆ Contributions ┆ PRs ┆ +     ┆ -   ┆ Total │
╞══════════════════════════╪═══════════════╪═════╪═══════╪═════╪═══════╡
│ Alice <alice@example.com>┆ 12            ┆ 8   ┆ 1,542 ┆ 389 ┆ 1,931 │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌╌╌┼╌╌╌╌╌╌╌┤
│ Bob <bob@example.com>    ┆ 8             ┆ 5   ┆ 876   ┆ 201 ┆ 1,077 │
╰──────────────────────────┴───────────────┴─────┴───────┴─────┴───────╯

20 commits walked, 13 squash merges expanded
```

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

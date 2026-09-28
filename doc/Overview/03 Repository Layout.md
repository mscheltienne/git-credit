---
aliases:
  - Repository Layout
tags:
  - overview
  - tooling
---
The repository is a single Cargo package with a library crate, `git_credit`, and a thin binary, `git-credit`, on top of it. Each source module owns one stage of the [pipeline](02%20Pipeline.md).

## Source modules

| Module | Owns |
| --- | --- |
| [`src/main.rs`](../../src/main.rs) | Parses the CLI and calls `git_credit::run` |
| [`src/lib.rs`](../../src/lib.rs) | `run`, mailmap loading, GitHub client resolution, squash-merge expansion and fallback, and the mock-API tests |
| [`src/cli.rs`](../../src/cli.rs) | The clap `Cli` struct and `OutputFormat` |
| [`src/git.rs`](../../src/git.rs) | Opening the repository, the commit walk, per-commit diffs, PR-number extraction, mailmap resolution of one identity, date parsing and formatting, bot detection |
| [`src/github.rs`](../../src/github.rs) | The `GitHubApi` trait, the `GitHubClient` HTTP implementation, rate-limit detection, token resolution, remote URL parsing |
| [`src/filter.rs`](../../src/filter.rs) | `ExclusionFilter`: glob-to-regex translation and excluded-file line totals |
| [`src/stats.rs`](../../src/stats.rs) | The report types, the proportional split, bot stripping, and the per-author rollup for the table |
| [`src/output.rs`](../../src/output.rs) | Table and JSON rendering |
| [`src/error.rs`](../../src/error.rs) | The `CreditError` enum |

Every module is `pub` in the library, so tests and potential embedders reach the same functions the binary uses. Unit tests live in a `#[cfg(test)] mod tests` at the bottom of each module.

## Tests

- [`tests/cli.rs`](../../tests/cli.rs) — integration tests that run the compiled binary.
- [`tests/common/mod.rs`](../../tests/common/mod.rs) — fixtures that build small repositories with known authors, dates and files.

Both are described in [Testing](../Development/02%20Testing.md).

## Configuration files

| File | Purpose |
| --- | --- |
| [`Cargo.toml`](../../Cargo.toml) | Package metadata, MSRV (`rust-version`), dependencies, clippy and rustc lint levels, release profile |
| [`Cargo.lock`](../../Cargo.lock) | Locked dependency versions; release builds use `--locked` |
| [`deny.toml`](../../deny.toml) | cargo-deny policy: licences, bans, sources, advisories |
| [`typos.toml`](../../typos.toml) | Spell-checker config; excludes `Cargo.lock` |
| [`.pre-commit-config.yaml`](../../.pre-commit-config.yaml) | typos, yamllint, and local `cargo fmt` / `cargo clippy` hooks |
| [`.yamllint.yaml`](../../.yamllint.yaml) | yamllint rules for the workflow and config YAML |
| [`.codecov.yaml`](../../.codecov.yaml) | Informational coverage targets |
| [`.github/workflows/`](../../.github/workflows/) | `ci`, `audit`, `bot` and `release` workflows |
| [`.github/dependabot.yaml`](../../.github/dependabot.yaml) | Weekly cargo and GitHub Actions updates |
| [`.github/release.yaml`](../../.github/release.yaml) | Generated release notes leave out bot-authored PRs |
| [`doc/`](../) | This vault |

The workflows and tools are covered in [Linting and CI](../Development/03%20Linting%20and%20CI.md) and [Release](../Development/04%20Release.md).

## Dependencies

| Crate | Used for |
| --- | --- |
| `git2` | libgit2 bindings: revwalk, diffs, mailmap. Built with `default-features = false`, so without libgit2's network transports — git-credit never fetches over git |
| `reqwest` | Blocking HTTPS client for the GitHub API, with `json` and `rustls` (no system OpenSSL, which keeps the musl release builds simple) |
| `rayon` | Parallel PR lookups |
| `clap` | CLI parsing, with `derive` |
| `serde`, `serde_json` | JSON output and GitHub response parsing |
| `regex` | PR-number and remote-URL patterns, compiled exclusion globs |
| `comfy-table` | The table output |
| `indicatif` | The PR-lookup progress bar |
| `thiserror`, `anyhow` | Typed library errors, contextual errors in `run` and `main` |

Dev-dependencies: `assert_cmd` and `predicates` for running the binary in tests, `tempfile` for throwaway repositories.

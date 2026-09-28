---
aliases:
  - Setup and Commands
tags:
  - tooling
---
git-credit builds with a stock Rust toolchain and Cargo; the only extra tools are pre-commit for the git hooks and, optionally, cargo-deny and cargo-llvm-cov. Nothing needs network access at test time.

## Prerequisites

| Tool | Needed for |
| --- | --- |
| Rust via rustup, at least the [MSRV](../Glossary/Minimum%20supported%20Rust%20version.md) in `Cargo.toml`'s `rust-version` | Building, testing, `cargo fmt`, `cargo clippy` (edition 2024) |
| [uv](https://docs.astral.sh/uv/) | Running pre-commit as `uvx pre-commit` without installing it |
| [cargo-deny](https://github.com/EmbarkStudios/cargo-deny) (optional) | The licence, source, ban and advisory checks CI runs |
| [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov) (optional) | Local coverage reports |
| [GitHub CLI](https://cli.github.com/) (optional) | A token for manual runs with squash-merge expansion |

The pre-commit `cargo-fmt` and `cargo-clippy` hooks are `language: system`: they call whatever `cargo` is on your `PATH`.

## First run

```sh
git clone https://github.com/mscheltienne/git-credit.git
cd git-credit
uvx pre-commit install        # optional: run the hooks on every commit
cargo build
cargo test
```

## Commands

| Command | Does |
| --- | --- |
| `cargo build` | Debug build at `target/debug/git-credit` |
| `cargo build --release` | Optimised build: thin LTO, one codegen unit, symbols stripped (`[profile.release]`) |
| `cargo run -- <flags>` | Build and run, e.g. `cargo run -- --repo ../some-repo --no-github` |
| `cargo test` | Unit and integration tests — [Testing](02%20Testing.md) |
| `cargo fmt` | Format with rustfmt's defaults (the repository has no `rustfmt.toml`) |
| `cargo clippy --all-targets -- -D warnings` | Lint as the hook and CI do — [Linting and CI](03%20Linting%20and%20CI.md) |
| `uvx pre-commit run --all-files` | Every hook: typos, yamllint, `cargo fmt`, `cargo clippy` |
| `cargo deny check` | All cargo-deny checks, advisories included |
| `cargo doc --no-deps` | API docs; CI builds them with warnings as errors |
| `cargo llvm-cov` | Coverage summary |

## Trying a change against a real repository

- **Start without GitHub.** `cargo run -- --repo <path> --no-github --format json` exercises the walk, exclusions, mailmap and bots, with no token and no rate limit.
- **Then with GitHub.** Drop `--no-github`; the token comes from `--token`, `GITHUB_TOKEN`, `GH_TOKEN` or `gh auth token` — [CLI Reference](../Usage/01%20CLI%20Reference.md#token-resolution). Narrow the run with `--rev` to spare the rate limit.
- **Compare JSON, not tables.** The table rolls rows up and hides per-commit changes — [JSON Output](../Usage/02%20JSON%20Output.md).

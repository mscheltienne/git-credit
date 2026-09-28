---
aliases:
  - Linting and CI
tags:
  - ci-cd
  - tooling
---
Formatting, clippy, the spell checker and cargo-deny gate every change, locally through pre-commit and in CI through GitHub Actions. CI also tests on three operating systems, checks the minimum supported Rust version, and builds the API docs.

## Lints

`Cargo.toml` sets the lint levels for every target:

```toml
# Cargo.toml (excerpt)
[lints.clippy]
pedantic = { level = "warn", priority = -1 }
# Allow specific noisy pedantic lints
missing_errors_doc = "allow"
missing_panics_doc = "allow"
module_name_repetitions = "allow"
must_use_candidate = "allow"

[lints.rust]
unsafe_code = "deny"
```

- **Clippy's pedantic group is on**, minus four noisy lints; `-D warnings` in the hook and in CI turns every warning into an error.
- **Local exceptions are `#[allow(...)]` on the item**, as on the date functions in `git.rs` (`clippy::similar_names`).
- **`unsafe` code is forbidden.**

## pre-commit

[`.pre-commit-config.yaml`](../../.pre-commit-config.yaml) defines four hooks; run them all with `uvx pre-commit run --all-files`.

| Hook | Checks |
| --- | --- |
| `typos` | Spelling in every tracked file, this vault included; [`typos.toml`](../../typos.toml) excludes only `Cargo.lock` |
| `yamllint` | `--strict` on `.github/`, `.codecov.yaml`, `.pre-commit-config.yaml` and `.yamllint.yaml`, with line length and document start disabled |
| `cargo-fmt` | `cargo fmt` over the crate (a local, `language: system` hook) |
| `cargo-clippy` | `cargo clippy --all-targets -- -D warnings` (local, `language: system`) |

pre-commit.ci also runs the config on pull requests: it opens autofix PRs, updates hook versions monthly, and skips the two cargo hooks, which need a Rust toolchain and run in GitHub Actions instead.

## cargo-deny

[`deny.toml`](../../deny.toml) sets the dependency policy:

- **Licences**: an allow-list of permissive licences (MIT, Apache-2.0 and its LLVM exception, BSD-3-Clause, ISC, Unicode-3.0, CDLA-Permissive-2.0).
- **Bans**: wildcard version requirements are denied; duplicate crate versions only warn.
- **Sources**: crates.io only; unknown registries and git sources warn.
- **Advisories**: RustSec advisories, with no ignores.

CI runs `licenses sources bans` on every change and advisories on a schedule — [[#Security audit workflow]].

## CI workflow

[`.github/workflows/ci.yaml`](../../.github/workflows/ci.yaml) runs on pushes to `main`, on pull requests and on manual dispatch; a newer run on the same ref cancels the older one. All jobs are independent.

| Job | Runs | Toolchain |
| --- | --- | --- |
| formatting | `cargo fmt --check` | stable |
| clippy | `cargo clippy --all-targets --all-features -- -D warnings` | stable |
| test | `cargo test --all-features` on Ubuntu, macOS and Windows | stable |
| coverage | `cargo llvm-cov` and the Codecov upload — [Testing](02%20Testing.md#coverage) | stable |
| MSRV | `cargo check --all-features` | pinned to the MSRV |
| cargo deny | `cargo deny check licenses sources bans` | — |
| documentation | `cargo doc --no-deps --all-features` with `RUSTDOCFLAGS=-D warnings` | stable |
| spell check | typos | — |

`Swatinem/rust-cache` caches build artefacts; the test matrix saves its cache only from `main`.

### The MSRV pin

The [MSRV](../Glossary/Minimum%20supported%20Rust%20version.md) job selects its toolchain through the `dtolnay/rust-toolchain` action's ref (`@<version>`), which must match `rust-version` in `Cargo.toml`; a comment in the workflow says so.

- **Keep the two in sync by hand.** Raising the MSRV means editing both `rust-version` and the job's ref.
- **Dependabot ignores `dtolnay/rust-toolchain`.** Its refs name toolchains, not action releases, so a Dependabot bump once moved the MSRV job to a toolchain that does not exist and silently stopped checking `rust-version`. [`.github/dependabot.yaml`](../../.github/dependabot.yaml) now excludes that action.

### Security audit workflow

[`.github/workflows/audit.yaml`](../../.github/workflows/audit.yaml) runs `cargo deny check advisories` daily at midnight UTC, on any push that changes a `Cargo.toml` or `Cargo.lock`, and on manual dispatch. It is separate from CI so that a newly published advisory surfaces without a code change.

## Dependency updates

- **Dependabot** opens weekly PRs for cargo dependencies and GitHub Actions, at most ten open per ecosystem.
- **The bot workflow** ([`.github/workflows/bot.yaml`](../../.github/workflows/bot.yaml)) enables auto-merge, as a squash merge, on PRs opened by `dependabot[bot]` or `pre-commit-ci[bot]`; they merge once the required checks pass.
- **Release notes skip them.** [`.github/release.yaml`](../../.github/release.yaml) excludes both bots from GitHub's generated release notes.

A dependency bump that changes an API still needs a manual fix — the comfy-table major upgrade, for example, broke the table renderer until it was adapted.

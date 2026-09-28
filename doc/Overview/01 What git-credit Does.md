---
aliases:
  - What git-credit Does
tags:
  - overview
  - attribution
---
git-credit is a Rust command-line tool that walks a git repository's history and reports, for every commit, which authors added and deleted how many lines. What sets it apart from `git log --numstat` is that it sees through squash merges: a squash-merged pull request is split across the authors of the pull request's original commits instead of being credited to one person.

## The problem it solves

A squash merge collapses a pull request (PR) into one commit on the base branch, recorded with a single author. Every other contributor to the PR vanishes from `git log`, and line counts computed from the history credit them nothing.

- **The original commits survive only on GitHub.**
  - After the squash, the PR's commits are no longer reachable from the base branch, but GitHub still lists them under the PR.
- **git-credit re-attributes the squash commit.**
  - It recognises a [squash-merge candidate](../Glossary/Squash-merge%20candidate.md) by the `(#N)` suffix GitHub puts on the commit subject, asks the GitHub API for PR #N's commits, weighs each author by the lines their commits changed, and splits the squash commit's line counts proportionally — [PR Expansion and Split](../Attribution/03%20PR%20Expansion%20and%20Split.md).

## What it computes

- **One row per non-merge commit** in the walked range, with the commit's author date and line counts against its first parent — [Commit Walk](../Attribution/01%20Commit%20Walk.md).
  - Merge commits are skipped: the lines they bring in are already credited to the commits they merge.
  - Renames count only their edited lines, and files matching an [exclusion glob](../Glossary/Exclusion%20glob.md) count nothing — [File Exclusions](../Attribution/07%20File%20Exclusions.md).
- **One [attribution](../Glossary/Attribution.md) per author in the row.**
  - A regular commit has one attribution: its author.
  - An expanded squash merge has one attribution per PR author, each holding that author's share of the commit's lines.
- **Canonical identities.**
  - Author names and emails go through the repository's [mailmap](../Glossary/Mailmap.md), or an external one, or none — [Mailmap and Identities](../Attribution/05%20Mailmap%20and%20Identities.md).
  - Bot accounts are dropped unless asked for — [Bots](../Attribution/06%20Bots.md).
- **An honesty marker.**
  - The [accurate flag](../Glossary/Accurate%20flag.md) is `false` on a squash merge whose PR could not be fetched, so a consumer knows which rows to recompute later — [Fallback and the Accurate Flag](../Attribution/04%20Fallback%20and%20the%20Accurate%20Flag.md).

## Inputs and outputs

| Input | Source |
| --- | --- |
| Commit history | The local repository, read through libgit2 (the `git2` crate); no network access for git |
| PR commits and their per-file stats | The GitHub REST API, authenticated with a token |
| Identity canonicalisation | `.mailmap` sources, or `--mailmap-file` |
| Scope | `--rev A..B`, `--since YYYY-MM-DD`, `--exclude <glob>` |

| Output | Shape |
| --- | --- |
| `--format table` (default) | One row per author with commit, PR and line totals — [Table Output](../Usage/03%20Table%20Output.md) |
| `--format json` | One record per commit with its attributions, plus run counters — [JSON Output](../Usage/02%20JSON%20Output.md) |

Warnings and the progress bar go to stderr; only the report goes to stdout.

## What it does not do

- **No line survival or blame.** A line counts when a commit adds or deletes it, whether or not it survives to `HEAD`.
- **No `Co-authored-by` trailers.** Only git authors count: the author of each walked commit, and the author of each PR commit GitHub lists.
- **No merge-commit credit.** Edits made while resolving a merge conflict belong to a skipped merge commit, so nobody gets them.
- **No storage.** Each run recomputes from scratch. A consumer that wants history can store the JSON rows, keyed by commit SHA, and re-run the commits whose `accurate` is `false`.

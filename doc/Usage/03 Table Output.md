---
aliases:
  - Table Output
tags:
  - output
  - cli
---
The default `--format table` rolls the report up to one row per author and prints it as a Unicode table, followed by a one-line summary. It is meant for people; scripts should use the [JSON output](02%20JSON%20Output.md), which the table is computed from.

A run with `--no-github` over three commits — one by Alice, one by Bob, one by Dependabot:

```text
╭─────────────────────────────────┬───────────────┬─────┬───────┬───┬───────╮
│ Author                          ┆ Contributions ┆ PRs ┆ +     ┆ - ┆ Total │
╞═════════════════════════════════╪═══════════════╪═════╪═══════╪═══╪═══════╡
│ Alice Smith <alice@example.com> ┆ 1             ┆ 0   ┆ 1,542 ┆ 0 ┆ 1,542 │
├╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┼╌╌╌╌╌┼╌╌╌╌╌╌╌┼╌╌╌┼╌╌╌╌╌╌╌┤
│ Bob Jones <bob@example.com>     ┆ 1             ┆ 0   ┆ 876   ┆ 0 ┆ 876   │
╰─────────────────────────────────┴───────────────┴─────┴───────┴───┴───────╯

2 authors (1 bots excluded), 3 commits walked, 0 squash merges expanded
```

The Dependabot commit is gone from the table but still counted in `3 commits walked`.

## Columns

`rollup_by_author` in [`src/stats.rs`](../../src/stats.rs) builds the rows from the report's attributions, after bot stripping:

| Column | Value |
| --- | --- |
| Author | `Name <email>` |
| Contributions | Number of commit reports in which the author has an attribution — regular commits, fallback rows and expanded squash merges alike |
| PRs | Number of those attributions with `is_pr_author: true`, i.e. expanded squash merges the author is credited in |
| `+` | Sum of credited additions |
| `-` | Sum of credited deletions |
| Total | `+` plus `-` |

- **Authors are keyed by lowercased email.** Differently capitalised emails merge into one row, shown with the name and email of the first attribution in report order — the earliest commit by author date.
- **Rows are sorted by Total, descending.** Ties keep the order of the lowercased email, since the sort is stable over a `BTreeMap`.
- **A fallback row counts as a contribution, not a PR**, because its attribution has `is_pr_author: false`.
- **An author with a zero-weight share of a squash merge** still gets a contribution and a PR for it, with zero lines.
- **Numbers use `,` thousands separators.**
- **The table adapts to the terminal width** (comfy-table's dynamic content arrangement), wrapping long author cells.

## Summary line

```text
{authors} authors[ ({bots} bots excluded)], {walked} commits walked, {expanded} squash merges expanded
```

- **`authors`** is the number of table rows.
- **The bots clause** appears only when `bots_excluded` is above zero.
- **`walked` and `expanded`** are the `total_commits_walked` and `squash_merges_expanded` [run counters](../Glossary/Run%20counters.md), both taken before bot stripping — [JSON Output](02%20JSON%20Output.md#summary-counters).
- **Nouns are not pluralised by count**: one author prints as `1 authors`.

An empty report prints `No contributions found.` and no summary line.

---
aliases:
  - JSON Output
  - JSON contract
tags:
  - output
  - cli
---
`--format json` prints one pretty-printed JSON object to stdout: a `commits` array with one [commit report](../Glossary/Commit%20report.md) per reported commit, and a `summary` object with three run counters. It is the machine-readable contract of git-credit; the types are `Report`, `CommitReport`, `Attribution` and `Summary` in [`src/stats.rs`](../../src/stats.rs), serialised by serde with their field names unchanged.

## Example

A regular commit, an expanded squash merge, and a squash merge whose expansion failed:

```json
{
  "commits": [
    {
      "sha": "1111111111111111111111111111111111111111",
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
      "sha": "2222222222222222222222222222222222222222",
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
      "sha": "3333333333333333333333333333333333333333",
      "author_date": "2026-04-19T16:05:22Z",
      "is_squash_pr": false,
      "attributions": [
        {
          "name": "Carol White",
          "email": "carol@example.com",
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

The third commit is a squash merge that could not be expanded: it carries the squash commit's author as a regular commit would, and `accurate: false` marks it for a retry.

## Commit fields

| Field | Type | Meaning |
| --- | --- | --- |
| `sha` | string | The full hexadecimal commit id |
| `author_date` | string | The git author time in UTC, `YYYY-MM-DDTHH:MM:SSZ`; the author's time-zone offset is not kept |
| `is_squash_pr` | bool | `true` only for a squash merge expanded through the GitHub API |
| `attributions` | array | One entry per credited author; never empty |
| `accurate` | bool | `false` only for a squash merge whose PR lookup failed — [Fallback and the Accurate Flag](../Attribution/04%20Fallback%20and%20the%20Accurate%20Flag.md) |

## Attribution fields

| Field | Type | Meaning |
| --- | --- | --- |
| `name` | string | Author name after mailmap resolution (unless `--no-mailmap`) |
| `email` | string | Author email after mailmap resolution, original case preserved |
| `additions` | integer | Added lines credited to this author, excluded files not counted |
| `deletions` | integer | Deleted lines credited to this author, excluded files not counted |
| `is_pr_author` | bool | `true` when the attribution comes from a PR expansion |

## Invariants

- **Order.** `commits` is sorted by `author_date` ascending, then by `sha`. In an expanded squash merge, `attributions` is sorted by lowercased email.
- **Shape by kind.** A report with `is_squash_pr: false` has exactly one attribution, with `is_pr_author: false`. A report with `is_squash_pr: true` has one or more, all with `is_pr_author: true`, and `accurate: true`.
  - After bot stripping, an expanded squash merge can be left with a single human attribution; it keeps `is_squash_pr: true`.
- **Sums.** For a regular or fallback commit, the attribution holds the commit's full totals. For an expanded squash merge, the attributions sum to at most the commit's totals: the split rounds each share down — [PR Expansion and Split](../Attribution/03%20PR%20Expansion%20and%20Split.md#the-proportional-split).
- **No merge commits** appear, and no commits authored before `--since`.
- **No bot attributions** appear unless `--bots`; a commit whose attributions were all bots is absent.
- **An empty run** prints `{"commits": [], "summary": {…}}` with zero counters, not an error.
- **Emails are not case-normalised across commits.** Group by lowercased email if one person may appear under several capitalisations.

## Summary counters

| Counter | Counts |
| --- | --- |
| `total_commits_walked` | Commit reports produced, before bot stripping: every walked commit that is not a merge commit and not before `--since` |
| `squash_merges_expanded` | Reports with `is_squash_pr: true`, before bot stripping: squash merges successfully split through GitHub |
| `bots_excluded` | Distinct bot emails removed, compared case-insensitively; 0 with `--bots` |

Because the first two are taken before bot stripping, `total_commits_walked` can exceed the length of `commits` — [Bots](../Attribution/06%20Bots.md#effect-on-the-counters). The number of fallback rows is not a counter; count the `accurate: false` rows.

## Consuming the output

- **Key rows by `sha`.** A commit's row depends only on the commit, the flags, the mailmap and GitHub's answer, so re-running with the same inputs reproduces it, and a recomputed row replaces the stored one.
- **Retry the inaccurate rows.** A consumer that stores the rows can recompute `accurate: false` commits once GitHub is reachable again, for example with `--rev <sha>^..<sha>`.
- **Read stdout only.** Warnings and the progress bar go to stderr, so stdout is always valid JSON when the exit status is 0.

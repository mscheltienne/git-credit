---
aliases:
  - File Exclusions
tags:
  - attribution
---
`--exclude <glob>` (repeatable) removes matching files from every line count: the walked commits' own totals and the weights of PR authors. An excluded file still exists in the history; its lines just count for nobody.

## Glob syntax

`ExclusionFilter::new` translates each [exclusion glob](../Glossary/Exclusion%20glob.md) into a regular expression anchored at both ends, so a glob must match the whole repository-relative path.

| Glob element | Matches | Regex |
| --- | --- | --- |
| `*` | Any run of characters within one path component | `[^/]*` |
| `?` | One character other than `/` | `[^/]` |
| `**/` | Zero or more leading directories | `(.*/)?` |
| `**` not followed by `/` | Anything, `/` included | `.*` |
| Anything else, `[`, `]`, `{`, `}` included | Itself, literally | escaped |

There are no character classes, brace alternatives or negations. A path is excluded when any glob matches it.

| Glob | Excludes | Keeps |
| --- | --- | --- |
| `*.lock` | `Cargo.lock`, `uv.lock` | `sub/Cargo.lock`, `locks/file.txt` |
| `**/*.lock` | `Cargo.lock`, `sub/Cargo.lock` | — |
| `**/*.generated.rs` | `file.generated.rs`, `src/deep/file.generated.rs` | `src/main.rs` |
| `docs/*` | `docs/README.md` | `src/docs/foo`, `docs/a/b.md` |
| `file?.txt` | `file1.txt`, `fileA.txt` | `file10.txt` |

The most common mistake is `*.lock` where `**/*.lock` is meant: `*` never crosses a `/`, so `*.lock` matches only top-level files. The `--exclude` help text says so.

## Where the filter applies

```rust
// src/filter.rs
pub fn line_totals(&self, deltas: &[FileDelta]) -> (u64, u64) {
    deltas
        .iter()
        .filter(|d| !self.is_excluded(&d.path))
        .fold((0, 0), |(a, d), f| (a + f.additions, d + f.deletions))
}
```

`line_totals` is the only place the filter is used, and it is called twice:

- **During the walk**, on each commit's local diff — [Commit Walk](01%20Commit%20Walk.md#diffing-a-commit).
  - The path matched is the new-side path, so a renamed file is matched under its new name.
- **During PR expansion**, on each PR commit's file list from GitHub — [PR Expansion and Split](03%20PR%20Expansion%20and%20Split.md#weighing-the-authors).
  - Applying it here keeps a weight consistent with the total it divides: an author who only regenerated a lock file does not take most of the credit for another author's code.
  - If every PR commit touched only excluded files, the total weight is zero and the split falls back to equal shares.

## Invalid patterns

A glob that fails to compile would stop the run with `invalid exclusion pattern`. In practice every character is either translated or escaped, so any glob compiles to a valid regex; the error path exists for safety.

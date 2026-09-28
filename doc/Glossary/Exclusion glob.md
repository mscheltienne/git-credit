---
aliases:
  - exclude
  - --exclude
  - ExclusionFilter
tags:
  - attribution
---
An exclusion glob is a `--exclude` pattern matched against a file's whole repository-relative path, where `*` and `?` stay within one directory and `**` spans directories; the lines of matching files count for nobody, in commit totals and PR weights alike.

Covered in: [Glob syntax](../Attribution/07%20File%20Exclusions.md#glob-syntax), [Where the filter applies](../Attribution/07%20File%20Exclusions.md#where-the-filter-applies)

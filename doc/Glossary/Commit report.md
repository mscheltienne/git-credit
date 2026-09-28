---
aliases:
  - CommitReport
  - commit row
tags:
  - output
---
A commit report is one element of the JSON `commits` array: a commit's SHA, UTC author date, `is_squash_pr`, its [attributions](Attribution.md) and the [accurate flag](Accurate%20flag.md). git-credit emits one per walked non-merge commit, minus those left with only bot attributions.

Covered in: [Commit fields](../Usage/02%20JSON%20Output.md#commit-fields)

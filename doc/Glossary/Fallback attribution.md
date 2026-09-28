---
aliases:
  - fallback
  - fallback row
tags:
  - attribution
---
A fallback attribution credits a squash merge whose PR lookup failed to the squash commit's own (mailmapped) author, exactly like a regular commit, with `is_squash_pr: false` and the [accurate flag](Accurate%20flag.md) set to `false`.

Covered in: [When a row is accurate](../Attribution/04%20Fallback%20and%20the%20Accurate%20Flag.md#when-a-row-is-accurate), [What counts as a failed lookup](../Attribution/04%20Fallback%20and%20the%20Accurate%20Flag.md#what-counts-as-a-failed-lookup)

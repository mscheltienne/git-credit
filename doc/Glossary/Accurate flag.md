---
aliases:
  - accurate
  - inaccurate row
tags:
  - attribution
  - output
---
The accurate flag is the `accurate` field of each JSON commit report: `false` when git-credit had GitHub access but could not expand a squash merge's PR and fell back to the squash commit's author, `true` otherwise. It tells a consumer which rows to recompute later.

Covered in: [When a row is accurate](../Attribution/04%20Fallback%20and%20the%20Accurate%20Flag.md#when-a-row-is-accurate)

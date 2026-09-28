---
aliases:
  - split
  - equal split
  - compute_squash_attributions
tags:
  - attribution
---
The proportional split divides an expanded squash commit's additions and deletions among the PR's authors in proportion to their [PR weights](PR%20weight.md), rounding each share down, and falls back to an equal split when the total weight is zero. Authors are merged by lowercased email first.

Covered in: [The proportional split](../Attribution/03%20PR%20Expansion%20and%20Split.md#the-proportional-split)

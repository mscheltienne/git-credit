---
aliases:
  - squash merge
  - pr_number
tags:
  - attribution
---
A squash-merge candidate is a walked, non-merge commit whose message's first line contains `(#N)`; git-credit takes the last such `N` as the pull request to expand. Whether the candidate is actually expanded depends on GitHub access and on the lookup succeeding.

Covered in: [The matching rule](../Attribution/02%20Squash-Merge%20Detection.md#the-matching-rule), [What a candidate becomes](../Attribution/02%20Squash-Merge%20Detection.md#what-a-candidate-becomes)

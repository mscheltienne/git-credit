---
aliases:
  - fast path
tags:
  - attribution
  - github-api
---
The single-author fast path is the shortcut in `fetch_pr_weights` that, when every PR commit has the same email ignoring case, skips the per-commit file requests and credits that one author with the whole squash commit.

Covered in: [Weighing the authors](../Attribution/03%20PR%20Expansion%20and%20Split.md#weighing-the-authors)

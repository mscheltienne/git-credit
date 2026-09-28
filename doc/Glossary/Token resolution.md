---
aliases:
  - GitHub token
  - GITHUB_TOKEN
  - GH_TOKEN
tags:
  - github-api
  - cli
---
Token resolution is the order in which git-credit looks for a GitHub token: `--token`, then `GITHUB_TOKEN`, then `GH_TOKEN`, then `gh auth token`, skipping empty values and running `gh` only when the others yield nothing. Without a token the run skips GitHub with a warning.

Covered in: [Token resolution](../Usage/01%20CLI%20Reference.md#token-resolution)

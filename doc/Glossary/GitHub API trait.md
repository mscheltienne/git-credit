---
aliases:
  - GitHubApi
  - GitHubClient
  - MockApi
tags:
  - github-api
  - testing
---
The GitHub API trait, `GitHubApi`, is the two-method interface — list a PR's non-merge commits, fetch a commit's file stats — through which git-credit reaches GitHub. `GitHubClient` implements it over HTTPS; the tests implement it with an in-memory `MockApi`.

Covered in: [The GitHub API seam](../Attribution/03%20PR%20Expansion%20and%20Split.md#the-github-api-seam), [The mock GitHub API](../Development/02%20Testing.md#the-mock-github-api)

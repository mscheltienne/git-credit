---
aliases:
  - slug
  - RepoSlug
tags:
  - github-api
---
The repository slug is the `owner/repo` pair git-credit parses from the `origin` remote's github.com URL (HTTPS, `ssh://` or scp-style) to address the GitHub API. Without one, the run skips GitHub with a warning.

Covered in: [Remote URL parsing](../Usage/01%20CLI%20Reference.md#remote-url-parsing)

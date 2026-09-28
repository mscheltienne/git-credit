---
aliases:
  - .mailmap
  - gitmailmap
tags:
  - identity
---
A mailmap is a file in git's [gitmailmap(5)](https://git-scm.com/docs/gitmailmap) format that maps the names and emails recorded in commits to one canonical identity per person. git-credit applies either the repository's mailmap (as libgit2 assembles it), a `--mailmap-file`, or none, to both walked commit authors and PR commit authors.

Covered in: [Where the mailmap comes from](../Attribution/05%20Mailmap%20and%20Identities.md#where-the-mailmap-comes-from)

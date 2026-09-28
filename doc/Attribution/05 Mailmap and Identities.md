---
aliases:
  - Mailmap and Identities
tags:
  - attribution
  - identity
---
git-credit identifies an author by name and email, canonicalised through a [mailmap](../Glossary/Mailmap.md) when one is loaded. One mailmap applies to both sources of authors: the git authors of walked commits and the git authors of PR commits fetched from GitHub. Where git-credit itself compares identities — merging PR authors, the single-author fast path, the table rollup, the bot count — it compares lowercased emails.

## Where the mailmap comes from

```rust
// src/lib.rs — load_mailmap
fn load_mailmap(cli: &Cli, repo: &git2::Repository) -> Result<Option<Mailmap>, CreditError> {
    if cli.no_mailmap {
        return Ok(None);
    }
    if let Some(path) = &cli.mailmap_file {
        let content = fs::read_to_string(path) /* → CreditError::MailmapRead */?;
        let mailmap = Mailmap::from_buffer(&content) /* → CreditError::MailmapParse */?;
        return Ok(Some(mailmap));
    }
    Ok(repo.mailmap().ok())
}
```

| Flags | Mailmap used |
| --- | --- |
| none | The repository's, as libgit2 assembles it — [[#The repository's mailmap]] |
| `--mailmap-file PATH` | Only the file at `PATH`; the repository's sources are ignored, not merged |
| `--no-mailmap` | None: identities are reported exactly as recorded |
| `--no-mailmap --mailmap-file PATH` | Rejected by clap as conflicting arguments |

- **An external file is strict.** A missing or unreadable file fails the run with `could not read mailmap file`, and a file `Mailmap::from_buffer` rejects (such as one containing a NUL byte) with `invalid mailmap file`.
- **The repository's mailmap is lenient.** If libgit2 fails to build it, `.ok()` turns the error into "no mailmap", and the run continues without canonicalisation.
- **`--mailmap-file` suits read-only use.** It canonicalises a clone without writing a `.mailmap` into it, and lets one mailmap be maintained across many repositories.
- **`--no-mailmap` suits consumers that canonicalise later.** The rows then carry raw identities, and the consumer can apply its own mailmap at read time without re-running git-credit.

### The repository's mailmap

`Repository::mailmap` wraps libgit2's `git_mailmap_from_repository`, which loads up to three sources in this order, later entries overriding earlier ones for the same identity:

1. `.mailmap` at the root of the working tree (skipped in a bare repository).
2. The blob named by the `mailmap.blob` config — `HEAD:.mailmap` by default in a bare repository.
3. The file named by the `mailmap.file` config.

The order and the override rule are libgit2's (`mailmap_add_from_repository`), not git-credit code.

## Resolving one identity

```rust
// src/git.rs
pub fn resolve_author(mailmap: Option<&Mailmap>, name: &str, email: &str) -> Author {
    if let Some(mm) = mailmap
        && let Ok(sig) = git2::Signature::new(name, email, &git2::Time::new(0, 0))
        && let Ok(resolved) = mm.resolve_signature(&sig)
    {
        return Author {
            name: resolved.name().unwrap_or(name).to_string(),
            email: resolved.email().unwrap_or(email).to_string(),
        };
    }
    Author { name: name.to_string(), email: email.to_string() }
}
```

- **Any failure keeps the raw identity.** If libgit2 refuses to build a signature from the name and email, or resolution fails, the identity passes through unchanged rather than failing the run.
- **Walked commits are resolved during the walk**; PR commit authors after the parallel GitHub fetch, because `Mailmap` is not `Sync` — [PR Expansion and Split](03%20PR%20Expansion%20and%20Split.md#mailmap-after-the-fetch).
- **The committer is never looked at.** Only authors are credited.

### Case sensitivity

- **libgit2 matches mailmap entries case-sensitively.** Its lookup compares emails and names with a plain `strcmp`, unlike `git` itself, which matches emails case-insensitively; an entry for `<alice@example.com>` does not rewrite `Alice@Example.com`.
- **git-credit's own comparisons ignore case:**

| Where | Key | Displayed identity |
| --- | --- | --- |
| Single-author fast path | Lowercased raw GitHub email | The first PR commit's author, then mailmapped |
| Merging PR authors in the split | Lowercased mailmapped email | First-seen name and email |
| Table rollup | Lowercased email | First-seen name and email, in report order |
| `bots_excluded` count | Lowercased email | — |

- **JSON rows are not case-normalised.** Two regular commits authored as `Alice@Example.com` and `alice@example.com` keep their own spellings; merging them is the consumer's choice.

## The PR author flag

`is_pr_author` is `true` on every attribution of an expanded squash merge and `false` everywhere else, fallback rows included. It records how the attribution was made, not whether the person opened the PR: every author of a non-merge PR commit gets it — [JSON Output](../Usage/02%20JSON%20Output.md).

---
aliases:
  - Bots
tags:
  - attribution
  - identity
---
By default git-credit removes [bot accounts](../Glossary/Bot%20account.md) from the report; `--bots` keeps them. A bot is any attribution whose email contains `[bot]@`, the shape of GitHub App identities such as `dependabot[bot]@users.noreply.github.com`.

```rust
// src/git.rs
pub fn is_bot_email(email: &str) -> bool {
    email.contains("[bot]@")
}
```

- **The test is a case-sensitive substring match on the final email**, after mailmap resolution.
  - A mailmap can therefore turn a bot into a human identity, or a human into a `[bot]@` address.
  - `Dependabot[bot]@…` matches, since the `[bot]@` part is lowercase; a hypothetical `[BOT]@` would not.

## When bots are stripped

`strip_bots` runs after expansion and the summary counters, and before sorting:

```rust
// src/stats.rs — strip_bots
commits.retain_mut(|commit| {
    commit.attributions.retain(|a| {
        let is_bot = is_bot_email(&a.email);
        if is_bot {
            bots.insert(a.email.to_lowercase());
        }
        !is_bot
    });
    !commit.attributions.is_empty()
});
bots.len() as u64
```

- **Bot attributions are removed, then commits left with none.**
  - A bot's own commit disappears from the report.
  - In an expanded squash merge that a bot co-authored, the bot's share is dropped and not redistributed: the humans keep exactly the shares the split gave them.
- **`bots_excluded` counts distinct bot emails**, compared case-insensitively — not commits, not attributions.
  - With `--bots`, nothing is stripped and `bots_excluded` is 0.

## Effect on the counters

The two other [run counters](../Glossary/Run%20counters.md) are taken before `strip_bots`, so they still include what was stripped:

- **`total_commits_walked`** counts a bot-only commit even though its row is gone.
- **`squash_merges_expanded`** counts a squash merge whose PR authors were all bots.

The table's summary line appends `(N bots excluded)` when the count is above zero — [Table Output](../Usage/03%20Table%20Output.md#summary-line).

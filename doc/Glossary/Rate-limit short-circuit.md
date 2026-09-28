---
aliases:
  - rate limit
  - RateLimited
tags:
  - github-api
---
The rate-limit short-circuit is the shared flag that, once any PR lookup gets a GitHub rate-limit response (a 429, or a 403 with an exhausted quota or a `retry-after` header), makes every PR lookup that starts afterwards fail at once without a request.

Covered in: [Detecting a rate limit](../Attribution/04%20Fallback%20and%20the%20Accurate%20Flag.md#detecting-a-rate-limit), [The rate-limit short-circuit](../Attribution/04%20Fallback%20and%20the%20Accurate%20Flag.md#the-rate-limit-short-circuit)

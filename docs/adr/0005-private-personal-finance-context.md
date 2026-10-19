---
status: accepted
---

# Keep personal finance interpretations private and outside the CLI

The CLI supplies bank records, while an agent-led interview captures meanings the bank cannot infer, such as account purposes and expense-sharing arrangements.
Store those answers in a private personal finance context outside the repository and load them for later finance sessions.
The public skill describes the interview; the user's answers stay out of commits, PRs, backups, and shared locations.

Use the existing summary and classified-transaction commands to guide the interview instead of adding an interview to the binary.
These interpretations guide the agent's reasoning and do not rewrite the bank's classifications or the CLI's calculated output.

Historical evidence: [personal finance context skill](https://github.com/magnusrodseth/sparebank1-cli/commit/d14be5e75b90ab33c1b2d485ebe3f43896d904b3).
[ADR 0006](0006-isolated-banking-profiles.md) later scopes this context to a profile and preserves the legacy context in place.

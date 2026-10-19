---
status: accepted
---

# Use personal OAuth credentials and confirm transfers

The initial CLI uses the user's personal API client and interactive BankID login, then stores OAuth tokens for later reads and refreshes.
This follows the personal-access boundary recorded in the repository's API terms; each user supplies their own credentials.

Money movement displays a confirmation before execution, with an explicit CLI option to skip that prompt.
Debit transfers require both account references to resolve within the authenticated account list and reject unknown destinations.
List membership is the implemented restriction; it does not prove legal ownership of those accounts.

Historical evidence: [initial implementation](https://github.com/magnusrodseth/sparebank1-cli/commit/7f43ad85eb67e832797052b40597c7728aaca8a6), including `src/auth.rs`, `src/commands/transfer.rs`, and `TERMS.md`.
[ADR 0006](0006-isolated-banking-profiles.md) later makes authentication and transfer selection explicit per profile.

---
status: accepted
---

# Derive summaries from bank data and bank classification

Compute summaries from accounts and transactions returned by the API.
Use the bank's transaction categories, recurring flags, and subscription flags so the report works without hardcoded merchant rules.
Prefer classified transactions and fall back to plain transactions when the classified request fails.

Exclude transactions whose counterparty number matches an included account number or IBAN from the summary's cash-flow calculations.
The historical implementation calls these internal transfers and treats the account list as the reporting boundary.
The bank's account list establishes access, while ownership and expense-sharing interpretations require additional context.

Historical evidence: [summary and classification implementation](https://github.com/magnusrodseth/sparebank1-cli/commit/94d5683eba36b0302e075fbcfa40c6dadbfa644f), especially `src/commands/summary.rs` and the corrected classified-response model.

The released JSON uses `netWorth`, `assets`, and `liabilities`.
Its balance map separates currencies, but several other totals combine them.
[ADR 0007](0007-combined-account-views.md) proposes different names and currency-separated metrics for combined account summaries while retaining the single-profile JSON contract.

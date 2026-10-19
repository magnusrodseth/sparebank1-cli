---
status: accepted
---

# Select profiles explicitly for combined account views

Profiles represent banking logins, and account access does not establish ownership or household membership.
Combined views therefore describe a deliberate profile selection and attribute the returned information to its source profiles.
This extends [ADR 0006](0006-isolated-banking-profiles.md), which establishes isolated credentials and single-profile reads.

Repeat `--profile` to select several profiles. Keep `--all-profiles` as shorthand for selecting every configured profile.
One explicit profile retains its existing meaning, and commands that require one profile reject multiple selections.
Profile selection and account types are independent: `accounts --all` includes all types regardless of the selected profiles.

A combined account summary includes accessible accounts, counts shared accounts once, and excludes transfers between included accounts from income and spending.
Use ownership-neutral labels and keep balance totals separate by currency.
The combined JSON fields are `balancesByCurrency`, `positiveBalancesByCurrency`, and `negativeBalancesByCurrency`.

Keep all monetary metrics separate by currency in combined summaries.
This includes income, spending, net cash flow, monthly totals, categories, counterparties, and subscriptions.
Calculate each savings rate from income and spending in the same currency.

Preserve the released single-profile JSON fields for compatibility.
The distinct combined schema avoids extending ownership claims to accounts that the selected users can only access.

The combined JSON groups cash flow under `cashFlowByCurrency` and includes
account-to-profile attribution. Posted amounts are grouped by the queried
account's currency without currency conversion.

This contract is implemented in [#3](https://github.com/magnusrodseth/sparebank1-cli/pull/3).
The single-profile output contract from [ADR 0002](0002-derived-summaries-and-bank-classification.md) remains compatible.

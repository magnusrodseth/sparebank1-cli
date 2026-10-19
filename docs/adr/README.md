# Architecture decisions

Read [GLOSSARY.md](../../GLOSSARY.md) for domain terms.

ADRs 0001 through 0006 retrospectively record decisions implemented in merged commits through v1.2.0.
Each cites its historical evidence. The records do not reconstruct undocumented discussions or claim that alternatives were evaluated without evidence.
Numbering follows the order of the main-branch commits that establish each decision, rather than this documentation's creation date.
Routine fixes, release bumps, and cosmetic changes do not receive separate ADRs.

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](0001-personal-access-and-transfer-confirmation.md) | Personal OAuth access and transfer confirmation | Accepted, historical |
| [0002](0002-derived-summaries-and-bank-classification.md) | Derived summaries and bank classification | Accepted, historical |
| [0003](0003-secret-storage-defaults.md) | Native secret storage by default | Accepted, historical |
| [0004](0004-mask-table-output-only.md) | Table masking with unchanged JSON and CSV | Accepted, historical |
| [0005](0005-private-personal-finance-context.md) | Private personal finance context | Accepted, historical |
| [0006](0006-isolated-banking-profiles.md) | Isolated banking profiles and legacy compatibility | Accepted, historical |
| [0007](0007-combined-account-views.md) | Explicit combined account views | Proposed |

ADR 0007 records the current design discussion and comes after the profile decision it extends.
Its command syntax and output fields are proposals, not features available in v1.2.0.
The implementation is being reviewed separately in [#3](https://github.com/magnusrodseth/sparebank1-cli/pull/3).

Append future records using the next unused number.
When a decision changes, retain its historical record and link to the record that supersedes it.

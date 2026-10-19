---
status: accepted
---

# Mask table output while preserving machine-readable values

Provide `--mask` for sharing table screenshots without displaying supported sensitive values such as amounts, account numbers, and counterparty descriptions.
Keep dates, categories, and labels visible; masked amounts retain their sign and currency shape.

JSON and CSV retain their original values for automation.
This separates presentation masking from the machine-readable output contract, so selecting masked tables does not change exported financial data.
Masking therefore does not make JSON, CSV, or every visible table label safe to share.

Historical evidence: [masking implementation and explicit output scope](https://github.com/magnusrodseth/sparebank1-cli/commit/1842bea7b3467832b5644ac4eb0c2befe0ac8c27).

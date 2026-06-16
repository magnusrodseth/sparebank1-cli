---
name: sparebank1-transfers
description: Move money with the `sb1` CLI (SpareBank 1) — transfer between the user's own accounts, pay a credit card, or pay into a pension policy. Use when a user explicitly asks to transfer/move money, pay a credit card, or fund a pension via sb1. Handles the confirmation flow safely.
compatibility: Requires the `sb1` binary, installed and authenticated (see sparebank1-shared). Executes real, irreversible payments.
---

# sparebank1-transfers

Money-movement workflows for SpareBank 1 via `sb1`. Read
[sparebank1-shared](../sparebank1-shared/SKILL.md) first.

**⚠ Transfers are real and irreversible. Treat every transfer as high-stakes.**

## Safety rules (follow exactly)

1. **Never** pass `-y/--yes` unless the user has explicitly confirmed the exact
   amount and destination in this conversation. Prefer letting the CLI's own
   confirmation prompt show, and surface its summary to the user.
2. Confirm `from`, `to`, and `amount` against `sb1 accounts` before running.
3. `debit` transfers are **own-account only** — both `--from` and `--to` must be
   the user's own accounts (the CLI rejects external destinations by design).
4. If anything is ambiguous (which account, how much), stop and ask. Do not guess.

## Transfer between your own accounts

```bash
sb1 transfer debit --from Brukskonto --to Sparekonto --amount 250
sb1 transfer debit --from Brukskonto --to Sparekonto --amount 250 --message "Sparing"
```

The CLI prints a summary and asks `Proceed? [y/N]`. Amounts accept `250`,
`250.50`, or `250,50`. Optional `--due-date YYYY-MM-DD` schedules it.

## Pay a credit card

```bash
sb1 transfer creditcard --from Brukskonto --credit-card-id <id> --amount 500
```

`--credit-card-id` is the credit card account id (find it via `sb1 accounts --all`).
The payment posts as a payment, so the card balance may update the next business
day.

## Pay into a pension policy

```bash
sb1 transfer pension --from Brukskonto --policy-number <polisenummer> --amount 500
```

## Non-interactive use

Only when the user has clearly authorized the specific transfer, you may add
`-y` to skip the prompt. Always echo back exactly what you ran (from / to /
amount) afterwards, and report the returned `paymentId` and any warnings.

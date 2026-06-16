---
name: sparebank1-accounts
description: Read accounts, balances, and transactions with the `sb1` CLI (SpareBank 1). List accounts and totals, look up balances, browse and filter transactions by date/account, inspect a single transaction, and export to CSV. Use when a user wants to see their Norwegian bank balances, spending, recent transactions, or wants account/transaction data for analysis.
compatibility: Requires the `sb1` binary, installed and authenticated (see sparebank1-shared).
---

# sparebank1-accounts

Read-only workflows for SpareBank 1 accounts and transactions via `sb1`. Read
[sparebank1-shared](../sparebank1-shared/SKILL.md) first for auth and flags.

## Accounts

```bash
sb1 accounts                      # list (name, number, balance, currency, key)
sb1 accounts --all                # also include cards/BSU/ASK/pension/currency
sb1 --json accounts               # machine-readable
```

Individual account (by name, key, or number):

```bash
sb1 account Brukskonto            # summary
sb1 account Brukskonto --details  # extended details (JSON)
sb1 account Brukskonto --roles    # roles (JSON)
sb1 balance 1234.56.78903         # balance via account number (POST /accounts/balance)
```

## Transactions

```bash
# Last 30 days for one account
sb1 transactions -a Brukskonto --days 30

# Explicit range, multiple accounts
sb1 transactions -a Brukskonto -a Sparekonto --from 2026-01-01 --to 2026-03-31

# Useful flags
#   --limit N        cap rows
#   --source RECENT|HISTORIC|ALL
#   --classified     use the classified endpoint (adds categories)
#   --json           machine-readable
#   --csv -o file    write CSV locally
```

Omitting `-a/--account` queries **all** accounts. Dates are `YYYY-MM-DD`.

Single transaction details (id comes from a `transactions` listing):

```bash
sb1 transaction <id>              # details (JSON)
sb1 transaction <id> --classified
```

## CSV export (server-rendered)

```bash
sb1 export -a Brukskonto --from 2026-05-01 --to 2026-06-16 -o booked.csv
```

`export` returns the bank's native semicolon-delimited CSV (Norwegian headers:
Dato, Beskrivelse, Inn, Ut, …) for **booked** transactions. For programmatic
analysis prefer `transactions --json` or `transactions --csv` instead.

## Analysis pattern

To answer "how much did I spend on X last month":

1. `sb1 --json transactions -a <account> --from <start> --to <end>` (use `ALL`
   source if you need older rows).
2. Parse the JSON `transactions[]`: `amount` (negative = outgoing), `date`
   (ISO), `description`, `counterpartyName`.
3. Sum/group in your own logic. Never guess figures the API didn't return.

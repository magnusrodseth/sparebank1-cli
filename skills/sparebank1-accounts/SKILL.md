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
sb1 accounts --profile <profile-name>  # select another local profile
sb1 accounts --all-profiles       # combine all profiles; shared accounts once
```

Individual account (by name, key, or number):

```bash
sb1 account Brukskonto            # summary
sb1 account Brukskonto --all-profiles # combined read-only lookup
sb1 account Brukskonto --details  # extended details (JSON)
sb1 account Brukskonto --roles    # roles (JSON)
sb1 balance 1234.56.78903         # balance via account number (POST /accounts/balance)
sb1 balance 1234.56.78903 --all-profiles
```

## Transactions

```bash
# Last 30 days for one account
sb1 transactions -a Brukskonto --days 30
sb1 transactions --all-profiles --days 30 # combined; shared accounts queried once

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
sb1 transaction <id> --all-profiles  # look for the temporary API id in each profile
```

## CSV export (server-rendered)

```bash
sb1 export -a Brukskonto --from 2026-05-01 --to 2026-06-16 -o booked.csv
sb1 export -a Brukskonto --all-profiles -o booked.csv
```

`export` returns the bank's native semicolon-delimited CSV (Norwegian headers:
Dato, Beskrivelse, Inn, Ut, …) for **booked** transactions. For programmatic
analysis prefer `transactions --json` or `transactions --csv` instead.

## Financial overview (preferred for "how are my finances")

```bash
sb1 summary --months 6        # net worth, monthly cash flow, categories, subs
sb1 --json summary --months 6 # machine-readable
sb1 summary --months 6 --all-profiles # aggregate household-level view
```

`summary` is generalizable across any account setup: net worth per currency,
income vs spending (internal transfers between the user's own accounts are
excluded), monthly breakdown, spending by **bank-assigned category**, top
counterparties, and bank-flagged subscriptions. Prefer this over hand-rolled
analysis.

Read commands use the default profile unless `--profile <profile-name>` or
`--all-profiles` is supplied. Aggregate views deduplicate accounts by bank
account key, count shared accounts once in summaries, and label transaction and
export rows with the profile used to fetch them. The API transaction id is a
temporary lookup value; use `transaction <id> --all-profiles` to search each
configured profile. Aggregate JSON includes `complete` and `errors`; check them
before relying on results. For table and CSV output, partial failures are
reported on stderr. `--profile` and `--all-profiles` cannot be combined. Use
`accounts --all` for all account types in one profile; aggregate account lists
include all account types by default.

## Manual analysis pattern

For bespoke questions ("how much on X last month"):

1. `sb1 --json transactions -a <account> --from <start> --to <end> --classified`
   (add `--source ALL` for older rows). `--classified` adds `category`,
   `recurring`, and `subscription` per transaction.
2. Parse `transactions[]`: `amount` (negative = outgoing), `date` (ISO),
   `description`, `counterpartyName`, `category`.
3. Sum/group in your own logic. Never guess figures the API did not return.

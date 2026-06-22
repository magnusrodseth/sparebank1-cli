---
name: sparebank1-context
description: Build and maintain a personal finance context for the `sb1` CLI (SpareBank 1) by running a short onboarding interview over the user's own accounts and transactions, then saving the answers to a private, git-ignored file the agent reads at the start of future finance sessions. Captures the meaning the bank can't know — which inbound money is salary vs reimbursement, what each account is for, what's work-expensed, split with a partner, or a subscription to cancel. Use when the user wants to personalize sb1, set up or refresh their finance context, teach the agent about their money, or annotate uncategorized/ambiguous transactions.
compatibility: Requires the `sb1` binary, installed and authenticated (see sparebank1-shared). Writes one markdown file to a user-owned path outside this repo.
---

# sparebank1-context

Turn `sb1` from "shows transactions" into "reasons over *your* money". This skill
runs a brief interview over the user's real data, then stores the answers as a
**personal finance context** that future finance sessions load up front. Read
[sparebank1-shared](../sparebank1-shared/SKILL.md) first for auth and flags.

The CLI already surfaces the *data*. This skill captures the *personal meaning
layer* the bank cannot infer, and nothing more.

## The privacy contract (read this first)

This skill's *mechanism* is public (it ships in a public repo and installs for
anyone), but the *answers are private personal data*. Therefore:

- **Write the context file to a user-owned path outside this repo.** Default:
  `~/.config/sparebank1-cli/context.md` (alongside where the `file` secret store
  writes). Override with the `SB1_CONTEXT_FILE` environment variable.
- **Never** write answers, balances, account numbers, or counterparties into
  this repository, a commit, a PR, or any shared/cloud-synced location. Treat the
  file like the plaintext `file` secret store: keep it off backups and out of
  anything that syncs.
- Prefer account **names + last 4 digits** over full account numbers. Don't
  record figures the user didn't ask you to keep.
- The file is for *this user's* agent sessions. Don't paste its contents into
  chats, issues, or external services.

Resolve the path once at the start:

```bash
ctx="${SB1_CONTEXT_FILE:-$HOME/.config/sparebank1-cli/context.md}"
mkdir -p "$(dirname "$ctx")"
```

## When to run

- **First time:** no context file exists → run the full interview below.
- **Refresh:** the file exists → read it, show the user what's captured, and only
  ask about what's new or changed (new counterparties, new Uncategorized rows,
  subscriptions to revisit). Don't re-ask settled questions.

Confirm login before pulling data:

```bash
sb1 status   # if not logged in, ask the user to run `sb1 login` (see sparebank1-shared)
```

## Step 1 — gather the data (no code change needed)

Drive the interview from existing commands. Start with a short window:

```bash
sb1 --json summary --months 3
```

From the JSON, the rows worth asking about are:

- `categories[]` — especially the **`"Uncategorized"`** bucket (the bank
  couldn't classify these; you can, with the user's help).
- `topOutgoing[]` — the biggest counterparties by spend.
- `subscriptions[]` — what the bank already flagged as recurring.

To see the actual Uncategorized rows behind that bucket, drill in per account:

```bash
sb1 --json transactions -a <account> --from <start> --to <end> --classified
```

Each row has `category`, `recurring`, `subscription`, `counterpartyName`,
`amount` (negative = outgoing), and `date`. Filter to rows with a missing/`"Uncategorized"`
category or to the top counterparties — those are your interview targets.

## Step 2 — the interview (keep it short)

**Prioritize the `Uncategorized` bucket and the top counterparties.** Do not walk
every transaction. Group similar rows and ask once per group. Aim for a handful
of focused questions, not a questionnaire. Capture these dimensions:

| Dimension | The question to resolve |
|---|---|
| **Income classification** | For each inbound counterparty: salary, reimbursement, transfer from a partner, or a refund? (So "income" isn't overcounted.) |
| **Work-expensed / reimbursed** | Which outgoing rows does work pay back (travel, equipment)? These should net OUT of "real" spending. |
| **Account semantics** | What is each account *for* — daily, buffer, a savings goal, BSU, joint? |
| **Shared / split** | Anything split with a partner where only the user's share should count? |
| **Counterparty aliases** | Cryptic strings (e.g. `KLARNA*…`, acquirer codes) → a human label. |
| **Subscriptions** | For each flagged (or user-known) subscription: keep, cancel, or annual? |
| **Fixed vs discretionary** | Which spend is fixed (rent, loan, insurance) vs discretionary? |

Ask in plain language, tied to what you actually saw: "You sent ~kr 4 000/month
to `BERG EIENDOM` — is that rent (fixed)?" beats an abstract checklist. Skip
dimensions that don't apply to this user.

## Step 3 — write the context file

Save the answers to the resolved path in the format below. On a refresh, merge
into the existing file rather than overwriting blind, and bump the date. After
writing, tell the user the exact path and restate that it stays out of git and
backups.

```markdown
# SpareBank 1 — personal finance context

<!-- Private. Maintained by the sparebank1-context skill. Never commit to git;
     keep off cloud-synced folders and backups. Last updated: YYYY-MM-DD -->

## Accounts — what each is for
- <name> (····<last 4>): <daily | buffer | savings goal | BSU | joint | ...>

## Income — how to classify inbound money
- <counterparty / pattern>: <salary | reimbursement | partner transfer | refund | other>

## Work-expensed / reimbursed  (net these OUT of real spending)
- <counterparty / pattern>: <what it is; reimbursed in full / partially>

## Shared / split expenses  (only my share counts)
- <counterparty / pattern>: <split, e.g. 50/50 with partner>

## Counterparty aliases  (cryptic string → human label)
- <raw string>: <label>

## Subscriptions
- <name>: <keep | cancel | annual> — <note>

## Fixed vs discretionary
- Fixed: <rent, loan, insurance, ...>
- Discretionary: <eating out, shopping, ...>

## Notes / preferences
- <anything else to know when reasoning about this money>
```

Leave out any section the user had nothing for. Don't pad it with invented rows.

## How future finance sessions use this

At the **start** of any session that reasons about the user's SpareBank 1 money
(budgeting, "how much did I really spend", savings rate, "what can I cancel"),
read the context file first:

```bash
cat "${SB1_CONTEXT_FILE:-$HOME/.config/sparebank1-cli/context.md}" 2>/dev/null
```

Then apply it on top of the raw `sb1` output: net out work-reimbursed and
internal/partner inflows before calling something "income", count only the
user's share of split rows, resolve aliases to readable labels, and respect the
keep/cancel notes on subscriptions. If the file is missing, offer to run this
skill's interview. If it looks stale (new big counterparties show up that aren't
in it), offer a quick refresh.

## Future idea (not built)

A `sb1 summary --uncategorized` helper could surface exactly the rows worth
asking about. It isn't needed: `summary` plus `transactions --classified` already
provide everything, and the conversational interview belongs to the agent, not
the binary.

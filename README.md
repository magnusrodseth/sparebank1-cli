# SpareBank 1 CLI

<p align="center">
  <img src="docs/banner.png" alt="A person at a laptop and an AI agent managing money together, with a piggy bank and kroner coins" width="640">
</p>

**Your finances, agent-ready.** `sb1` is a CLI for the
[SpareBank 1 personal banking API](https://developer.sparebank1.no), built for
AI agents (and humans).

List accounts, browse and export transactions, and make transfers, all
authenticated with BankID over OAuth 2.0. Output is human-readable tables by
default and machine-readable JSON with `--json`. Ships [agent skills](skills/) so
AI agents can drive it safely.

Written in Rust. Personal and unofficial; not affiliated with SpareBank 1.
Please read [TERMS.md](TERMS.md): usage of the underlying API is governed by the
bank's terms (personal use, confidential credentials, enforced rate limits).

## What it's for

Treat your bank as data, not a dashboard. The bank's app is great for glancing
at your money; `sb1` is for everything the app can't do, scripting, automation,
and especially AI agents that reason over your finances.

### Ask an agent about your money

Natural-language Q&A over your accounts and transactions, no fixed dashboard
required. The agent chains the commands and reasons over the JSON:

- "Where did my money go last month?"
- "What subscriptions am I paying for?"
- "Did my paycheck arrive?"
- "Help me optimize my finances: where am I wasting money, what can I cut, and where is my spending inconsistent?"

```bash
sb1 --json summary
sb1 --json transactions --classified
```

### Automate the boring parts

Scheduled balance checks, monthly reports, paycheck-arrival checks, "is the
balance below X?". Pair `--json` with cron or any task runner:

```bash
sb1 --json accounts
sb1 --json transactions --days 30
```

### Own your financial data

One-command export into your own spreadsheets, budgeting app, or notes, so you
keep a longitudinal history instead of the bank's fixed, current-only view:

```bash
sb1 export -a Brukskonto -o booked.csv
```

For everyday balance-glancing on the go, the bank's own app is still the better
tool; reach for `sb1` when you want your money as something you can query,
script, and pipe.

## For AI agents

`sb1` is designed for AI agent consumption: every command speaks `--json`, errors
are explicit, and money-moving commands confirm first. The repo ships
[agent skills](https://skills.sh) under [`skills/`](skills/) that teach an agent
how to drive it safely:

- `sparebank1-shared`: runtime contract (auth, storage, output, command map)
- `sparebank1-accounts`: read accounts, balances, transactions, exports
- `sparebank1-transfers`: money movement with confirmation safeguards

Install them into your agent:

```bash
npx skills add magnusrodseth/sparebank1-cli
```

## Install

Prebuilt binary (macOS and Linux, no toolchain required). Installs to
`~/.local/bin`:

```bash
curl -fsSL https://raw.githubusercontent.com/magnusrodseth/sparebank1-cli/main/install.sh | bash
```

Pin a version with `SB1_VERSION=v1.1.0`, or change the location with
`SB1_INSTALL_DIR`. Make sure the install dir is on your `PATH`, then run
`sb1 --help`.

From source (requires a Rust toolchain):

```bash
cargo install --path .          # installs the `sb1` binary
# or run from the repo:
cargo build --release           # ./target/release/sb1
```

## Setup

1. Go to [developer.sparebank1.no](https://developer.sparebank1.no) and register
   a **personal client** application. Note the `client_id` and `client_secret`.
2. Set the app's redirect URI to exactly:

   ```
   http://localhost:12345/callback
   ```

3. Provide the credentials. Easiest is a `.env` file (git-ignored):

   ```bash
   cp .env.example .env          # then fill in CLIENT_ID / CLIENT_SECRET
   ```

4. Log in with BankID (opens your browser):

   ```bash
   sb1 login
   ```

   The token is stored in your chosen secret store; you can delete `.env`
   afterwards.

## Secret storage

Your OAuth token is effectively a standing key to your bank (the CLI can refresh
access without a fresh BankID login), so where it lives matters. The store is
chosen with the `SB1_STORE` environment variable, and **the default is your OS
keychain**, so the easy path is also the secure one. `sb1 login` and `sb1 status`
both print the active backend and all alternatives, so you always see your
options.

| `SB1_STORE` | Backend | Notes |
| --- | --- | --- |
| _unset_ / `keychain` | macOS Keychain / Linux kernel keyutils / Windows Credential Manager | **Default.** Secure at rest, gated by your OS. On macOS an unsigned binary re-prompts for your login password after each rebuild. |
| `op` / `1password` | 1Password via the `op` CLI | Secure, no per-rebuild prompt (the `op` session caches your unlock). Needs `op` installed + signed in. Vault via `SB1_OP_VAULT` (default `Private`); account via `SB1_OP_ACCOUNT` if you have more than one. |
| `file` | `~/.config/sparebank1-cli/*.json` (`0600`) | **Opt-in. Plaintext on disk.** The most reliable backend for headless automation (servers, cron, CI, Docker) since it never prompts. Keep this dir out of git and cloud backups. |

```bash
# 1Password, no machine-password prompt; uses your 1Password unlock (Touch ID)
export SB1_STORE=op
export SB1_OP_ACCOUNT=my.1password.eu   # only needed with multiple op accounts
export SB1_OP_VAULT=Private             # default

# Plaintext files — only for headless automation where no keychain/op is available
export SB1_STORE=file
```

> Note on keychain prompts: an unsigned binary that you rebuild gets a new
> signature each time, so the keychain re-prompts for your login password on
> every run ("Always Allow" can't stick). 1Password (`op`) avoids this because
> the `op` CLI caches your unlock for the session. For a prompt-free keychain,
> code-sign the binary with a stable self-signed certificate.

> ⚠ The `file` backend writes your token and credentials as **plaintext secrets**
> under `~/.config/sparebank1-cli/`. Anything running as your user can read them,
> and a synced home directory or cloud backup will capture them. Prefer `keychain`
> or `op`; reach for `file` only on a headless box where neither is available.

## Usage

```bash
sb1 status                       # auth status, token expiry, storage backend
sb1 hello                        # verify auth against the Hello World endpoint

# Accounts
sb1 accounts                     # list (add --all to include cards/BSU/ASK/pension/currency)
sb1 account Brukskonto           # one account (by name, key, or number)
sb1 account Brukskonto --details # extended details
sb1 account Brukskonto --roles   # roles
sb1 balance 1234.56.78903        # balance by account number

# Transactions (account is positional, or use -a/--account; omit for all accounts)
sb1 transactions Brukskonto --days 30
sb1 transactions Brukskonto Sparekonto --days 30  # multiple accounts
sb1 transactions -a Brukskonto --days 30          # -a/--account works too
sb1 transactions Brukskonto --from 2026-01-01 --to 2026-03-31
sb1 transactions Brukskonto --classified
sb1 transaction <id>             # details for one transaction
sb1 transactions Brukskonto --csv -o out.csv      # local CSV
sb1 export -a Brukskonto -o booked.csv            # server-side CSV export

# Transfers (always confirms first; -y to skip)
sb1 transfer debit --from Brukskonto --to Sparekonto --amount 250
sb1 transfer debit --from Brukskonto --to Sparekonto --amount 250 --message "Sparing"
sb1 transfer creditcard --from Brukskonto --credit-card-id 1034222 --amount 500
sb1 transfer pension --from Brukskonto --policy-number 1034222 --amount 500

# Machine-readable output
sb1 --json accounts

# Financial overview: net worth, monthly cash flow, categories, subscriptions
sb1 summary --months 6
sb1 --json summary

# Mask sensitive values for screenshots (amounts, account numbers, names)
sb1 summary --months 6 --mask
sb1 accounts --mask
```

`--classified` enriches transactions with the bank's own category, recurring,
and subscription flags. `summary` builds on the same classification, so its
categories work for any account setup (no hardcoded merchants); internal
transfers between your own accounts are excluded automatically.

Most commands take an account as a positional argument (`sb1 account Brukskonto`,
`sb1 transactions Brukskonto`). `transactions` also accepts the older
`-a`/`--account` flag and lets you pass several accounts (positionally or with
repeated `-a`); omit the account entirely to query all of them.

`--mask` is a global flag for sharing screenshots: it replaces sensitive values
(amounts, account numbers, owner names, transaction descriptions, counterparties,
savings rate) with `*****` in table output while keeping categories, dates, and
labels visible. Masked amounts keep their sign and `kr` shape (`-kr *****`) so the
output still looks real. It has no effect on `--json` or CSV output, which stay
unredacted for automation.

`debit` transfers are restricted to **your own accounts**: both `--from` and
`--to` must resolve to accounts in your account list. Amounts accept `250`,
`250.50`, or `250,50`.

The two CSV paths differ. `transactions --csv` produces a comma-delimited UTF-8
file from the client. `export` streams the bank's own server-side CSV, which is
**semicolon-delimited with a UTF-8 BOM and Norwegian headers** (`Dato`,
`Beskrivelse`, `Rentedato`, `Inn`, `Ut`, ...), the format Excel expects in a
Norwegian locale.

## Commands

| Command | API |
| --- | --- |
| `login` / `logout` / `refresh` / `status` | OAuth 2.0 (`/oauth/authorize`, `/oauth/token`) |
| `hello` | `GET /common/helloworld` |
| `accounts` | `GET /accounts` |
| `account <ref>` | `GET /accounts/{key}` (`--details`, `--roles`) |
| `balance <number>` | `POST /accounts/balance` |
| `transactions` | `GET /transactions` (`--classified` → `/transactions/classified`) |
| `transaction <id>` | `GET /transactions/{id}/details` |
| `export` | `GET /transactions/export` |
| `transfer debit` | `POST /transfer/debit` |
| `transfer creditcard` | `POST /transfer/creditcard/transferTo` |
| `transfer pension` | `POST /transfer/pension` |
| `summary` | derived from `/accounts` + `/transactions/classified` |

The OpenAPI specs this client was built against are saved under
[`docs/api/`](docs/api/).

## Development

```bash
cargo build
cargo test
cargo clippy
cargo fmt --check
```

## Releasing

- **crates.io:** `cargo publish` (after `cargo publish --dry-run`). Users then
  `cargo install sparebank1-cli`.
- **Prebuilt binaries:** tag `vX.Y.Z`; CI (`.github/workflows/release.yml`)
  builds macOS/Linux tarballs and attaches them to the GitHub Release.

See the repository's release workflow for details.

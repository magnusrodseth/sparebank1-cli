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

## Install

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

Where the OAuth token and client credentials are kept, chosen with the
`SB1_STORE` environment variable:

| `SB1_STORE` | Backend | Notes |
| --- | --- | --- |
| _unset_ / `file` | `~/.config/sparebank1-cli/*.json` (`0600`) | **Default.** No prompts. Keep this dir out of git and backups. |
| `keychain` | macOS Keychain / Secret Service | Most secure at rest. Prompts after each rebuild of an unsigned binary. |
| `op` / `1password` | 1Password via the `op` CLI | Needs `op` installed + signed in. Vault via `SB1_OP_VAULT` (default `Private`); account via `SB1_OP_ACCOUNT` if you have more than one. |

Example (per-machine preference):

```bash
# Keychain
export SB1_STORE=keychain

# 1Password, no machine-password prompt; uses your 1Password unlock (Touch ID)
export SB1_STORE=op
export SB1_OP_ACCOUNT=my.1password.eu   # only needed with multiple op accounts
export SB1_OP_VAULT=Private             # default
```

> Note on keychain prompts: an unsigned binary that you rebuild gets a new
> signature each time, so the keychain re-prompts for your login password on
> every run ("Always Allow" can't stick). 1Password (`op`) avoids this because
> the `op` CLI caches your unlock for the session. For a prompt-free keychain,
> code-sign the binary with a stable self-signed certificate.

> ⚠ With the `file` backend, the files under `~/.config/sparebank1-cli/` are
> **secrets**. Make sure that directory is never committed to version control or
> synced to cloud backups.

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

# Transactions
sb1 transactions -a Brukskonto --days 30
sb1 transactions -a Brukskonto --from 2026-01-01 --to 2026-03-31
sb1 transactions -a Brukskonto --classified
sb1 transaction <id>             # details for one transaction
sb1 transactions -a Brukskonto --csv -o out.csv   # local CSV
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
```

`--classified` enriches transactions with the bank's own category, recurring,
and subscription flags. `summary` builds on the same classification, so its
categories work for any account setup (no hardcoded merchants); internal
transfers between your own accounts are excluded automatically.

`debit` transfers are restricted to **your own accounts**: both `--from` and
`--to` must resolve to accounts in your account list. Amounts accept `250`,
`250.50`, or `250,50`.

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

## For AI agents

This repo ships [agent skills](https://skills.sh) under [`skills/`](skills/) that
teach an AI agent how to drive `sb1` safely:

- `sparebank1-shared`: runtime contract (auth, storage, output, command map)
- `sparebank1-accounts`: read accounts, balances, transactions, exports
- `sparebank1-transfers`: money movement with confirmation safeguards

Install them into your agent:

```bash
npx skills add magnusrodseth/sparebank1-cli
```

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

---
name: sparebank1-shared
description: Runtime contract for the `sb1` CLI (SpareBank 1 personal banking). Covers install, BankID login, secret-storage backends, output formats, global flags, the command map, and error handling. Use this as the foundation before any other sparebank1 skill, or whenever a user asks to check Norwegian bank accounts, balances, or transactions via sb1.
compatibility: Requires the `sb1` binary installed and a registered SpareBank 1 developer app. Network access to api.sparebank1.no. BankID is needed for the initial login.
---

# sparebank1-shared

Foundation skill for the `sb1` CLI, a client for the
[SpareBank 1 personal banking API](https://developer.sparebank1.no). Read this
before using any other `sparebank1` skill.

**This is real money and real bank data. Be conservative.** Never invent account
numbers or amounts; only act on what commands return or what the user states.

## Install & first-time setup

```bash
cargo install sparebank1-cli          # installs the `sb1` binary
```

One-time setup the user must do themselves (requires their BankID):

1. Register a **personal client** app at https://developer.sparebank1.no
2. Set its redirect URI to exactly `http://localhost:12345/callback`
3. Put `CLIENT_ID` / `CLIENT_SECRET` in a `.env` (git-ignored) or env vars
4. `sb1 login`, opens the browser for BankID, stores the token

Do not run `sb1 login` on the user's behalf without asking; it triggers an
interactive BankID flow.

## Authentication & storage

```bash
sb1 status     # logged in? token expiry? which storage backend?
sb1 refresh    # force a token refresh
sb1 logout     # remove the token (add --all to also drop client credentials)
```

Tokens auto-refresh before expiry; the user normally does not re-login between
sessions. Secrets live in one of three backends, chosen by `SB1_STORE`:

| `SB1_STORE` | Where |
|---|---|
| _unset_ / `file` | `~/.config/sparebank1-cli/*.json` (0600), default |
| `keychain` | macOS Keychain / Secret Service |
| `op` / `1password` | 1Password via the `op` CLI (`SB1_OP_VAULT`, `SB1_OP_ACCOUNT`) |

If a command reports "not logged in" or the client secret is rejected, tell the
user to run `sb1 login` (the secret has limited validity and may need rotating
in the developer portal). Do not retry in a loop.

## Output & flags

- Default output is human-readable tables (Norwegian amounts: `kr 1 234,56`).
- Add `--json` to **any** command for machine-readable output, always use this
  when parsing programmatically.
- Errors print to stderr with a non-zero exit code.

## Command map

| Command | Purpose | Skill |
|---|---|---|
| `login` / `logout` / `refresh` / `status` / `hello` | auth & health | this skill |
| `accounts` | list accounts | sparebank1-accounts |
| `account <ref> [--details\|--roles]` | one account | sparebank1-accounts |
| `balance <number>` | balance by account number | sparebank1-accounts |
| `transactions ...` | list/filter/export transactions | sparebank1-accounts |
| `transaction <id>` | one transaction's details | sparebank1-accounts |
| `export ...` | server-side CSV export | sparebank1-accounts |
| `transfer debit\|creditcard\|pension` | move money | sparebank1-transfers |

`<ref>` for an account accepts its **name** ("Brukskonto"), **key**, or
**number**, the CLI resolves it. Verify with `sb1 accounts` first when unsure.

## Etiquette (API terms)

The bank enforces rate limits and monitors usage. Do **not** add retry/polling
loops or try to bypass limits. On HTTP 429 the CLI reports a `Retry-After`; wait,
don't hammer. Usage is strictly personal; never expose credentials in output.

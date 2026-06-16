# AGENTS.md

Guidance for AI agents helping a user set up and use `sb1`, the SpareBank 1
personal banking CLI. If you are an agent and the user pointed you at this
repository, start here.

## What this is

`sb1` is a command-line tool for the
[SpareBank 1 personal banking API](https://developer.sparebank1.no). It lists
accounts, browses and exports transactions, and makes transfers, authenticated
with BankID over OAuth 2.0. Every command supports `--json` for machine-readable
output. It is personal and unofficial; usage is governed by the bank's terms
(see `TERMS.md`).

## Install the binary

Prefer the prebuilt binary. It needs no Rust toolchain and does not call the
GitHub API (so it is not affected by API rate limits):

```bash
curl -fsSL https://raw.githubusercontent.com/magnusrodseth/sparebank1-cli/main/install.sh | bash
```

- Installs to `~/.local/bin` (override with `SB1_INSTALL_DIR`).
- Pin a version with `SB1_VERSION=v1.1.0`.
- If the script reports that the install dir is not on `PATH`, add it before
  continuing, then verify with `sb1 --help`.
- macOS and Linux only. On other platforms, build from source:
  `cargo install --path .`.

## Set up access (the user must do this once)

`sb1` talks to the bank on the user's behalf, so it needs their own API client
and a BankID login. You cannot do these steps for them:

1. The user registers a **personal client** at
   [developer.sparebank1.no](https://developer.sparebank1.no) and notes the
   `client_id` and `client_secret`.
2. The redirect URI on that app must be exactly `http://localhost:12345/callback`.
3. Provide the credentials via a git-ignored `.env` (`cp .env.example .env`,
   then fill in `CLIENT_ID` / `CLIENT_SECRET`).
4. Run `sb1 login` — this opens a browser for the interactive BankID flow.
   After login, the token is saved to the secret store and `.env` can be
   deleted.

Secret storage backend is chosen with `SB1_STORE` (`keychain` default, `op` for
1Password, `file` for headless automation). See `README.md` → "Secret storage".

## Install the agent skills

The repo ships skills that teach an agent to drive `sb1` safely:

```bash
npx skills add magnusrodseth/sparebank1-cli
```

- `sparebank1-shared` — runtime contract (auth, storage, output, command map). Read this first.
- `sparebank1-accounts` — read accounts, balances, transactions, exports.
- `sparebank1-transfers` — money movement, with confirmation safeguards.

## Operating rules for agents

- Use `--json` for anything you need to parse; tables are for humans.
- **Money-moving commands** (`transfer ...`) confirm before executing. Never
  pass `-y` to skip confirmation unless the user explicitly told you to in this
  session.
- `sb1` only sees what SpareBank 1 holds. Accounts at other institutions
  (e.g. DNB funds, Lånekassen) are not reachable even if the bank app aggregates
  them.
- Check `sb1 status` if commands fail with auth errors; the token may need
  `sb1 refresh` or a fresh `sb1 login`.

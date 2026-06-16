# SpareBank 1 API terms: what this tool must honour

Use of the SpareBank 1 personal banking API is governed by the bank's
"Vilkår for bruk av SpareBank 1 sine APIer". This file records the clauses that
shape `sb1`'s behaviour. The same notes live in code in
[`src/terms.rs`](src/terms.rs).

| Clause | What it says | How `sb1` complies |
| --- | --- | --- |
| §3 | The API key/secret has **limited validity** | A rejected secret produces a clear "rotate it in the developer portal" error, not a cryptic 401 (`src/error.rs::InvalidClientCredentials`). |
| §4.1 | Access is **strictly personal**; the key/secret are confidential and must not be shared | Secrets go to your keychain / 1Password / a `0600` file, never committed, never logged. `.env` is git-ignored. |
| §4.3 | Use the API **only as documented** | Only documented endpoints/params are called (see `docs/api/*.json`). |
| §5.1 | The bank **monitors** API usage | An honest, identifiable `User-Agent` is sent (`src/client.rs`). |
| §5.2 / §7 | You are **personally responsible** for protecting the data and credentials | Secret files are `0600`; tokens are redacted from output; you are warned when secrets are written to disk. |
| §6 | The bank **enforces rate limits**; circumventing them ends your access | No aggressive retries, no tight polling loops; HTTP 429 is surfaced with `Retry-After` and the request is **not** retried automatically. |

**Do not** add logic that bypasses rate limits, shares credentials, or calls
undocumented endpoints. It violates the terms and can get your access revoked.

This tool is personal and unofficial; it is not affiliated with or endorsed by
SpareBank 1.

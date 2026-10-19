---
status: accepted
---

# Default to native secret storage with explicit alternatives

The initial file-store default wrote client credentials and OAuth tokens as plaintext.
Change the default to the platform's native secret store so an ordinary setup does not write plaintext secret files.
Retain explicit 1Password and file backends for users who need a different storage mechanism, including headless automation.

Unknown backend values also select native storage instead of silently enabling the file backend.
Login and status expose the active backend and available alternatives because storage choice affects prompts and local data protection.
Use platform-specific native keyring backends so Linux builds do not require libdbus.

Historical evidence: [secure-default change and its rationale](https://github.com/magnusrodseth/sparebank1-cli/commit/73b90263c581fc38eec9ac950adabdd5f9d2a49e) and the earlier [platform-specific keyring change](https://github.com/magnusrodseth/sparebank1-cli/commit/b00aa05b2fe4ed77b16d1f689d44d8e868b87236).

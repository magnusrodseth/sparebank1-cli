---
status: accepted
---

# Isolate banking credentials and finance context by profile

Give each named profile separate API client credentials and OAuth tokens.
Reads select an explicit profile or the default profile; transfers require an explicit profile when several profiles exist.
Refresh and logout also require an explicit profile when several are configured.
Use the selected profile for account resolution and transfer confirmation so credentials and account choices stay together.

Adopt an existing single-profile login as the legacy `default` profile without moving or copying its stored secrets.
Preserve the existing top-level status JSON fields for the selected or default profile and add per-profile status alongside them.
Serialize registry updates so concurrent profile creation or default changes do not overwrite each other.

Personal finance contexts follow their profiles.
Keep the legacy context in place and use distinct paths for new profile contexts so changing the default cannot reassign private notes.

Historical evidence: [isolated profiles and compatibility fixes](https://github.com/magnusrodseth/sparebank1-cli/commit/577484ea53ed99728757b48a63d3aa35081d4ed1), merged in [#1](https://github.com/magnusrodseth/sparebank1-cli/pull/1) and released in [v1.2.0](https://github.com/magnusrodseth/sparebank1-cli/releases/tag/v1.2.0).
That release reads one profile at a time and provides no combined banking view.
[ADR 0007](0007-combined-account-views.md) proposes an explicit read-only extension to this isolation boundary.

# Swift integration follow-up — 29 October 2026

This implements the Swift audit following the Rust update, under
`swift-design-principles.md` and `rust-domain-principles.md`.

## Shared contracts

The Rust/FFI conversation now carries its action projection alongside its content.
The projection includes observation provenance (current or saved), observation time,
the initiation action and the conversations eligible for reply. Broad standing/lane
permission and conversation-confined reply permission are interpreted in Rust.
Malformed reply identities fail conversion rather than becoming usable destinations.
The separate action-fetch API and replaced Swift-facing permission fields were removed.
No extra network observation is needed to draw the controls.

A/B/C render the same action choices, preserving their independent visual styles.
Saved observations explain uncertainty rather than asserting a current refusal or
permission. Time remains internal provenance; no routine timestamp, reload control
or spinner was introduced. Recovery custody remains outside this presentation change.

MemberModel preserves Rust's native and external refresh outcomes independently.
The affected transport's send controls use its own freshness; unresolved recipients
cannot be submitted. These are presentation gates, not authorization: Rust still
checks each actual submission. A failed native mailbox page does not erase a
successful external refresh. Saved mailbox views and conversation readers explain
the relevant uncertainty without calling every partial failure an offline session.
Unaddressed writing can still open and save while observations are unavailable.
Conversation refresh also checks that its destination is still selected before
publishing the returned content.

SQLite mailbox commits construct one retained-ID set and reuse prepared insertion
statements. Head, retained references, changed ciphertext and draft submission state
remain one transaction; duplicate writes and missing references are rejected. This
removes quadratic membership checking and per-record statement preparation. It does
not implement lazy mailbox hydration or establish production-scale startup latency.

## Focused validation scope

Existing scenarios were adapted; no new test cases were added. The consequential
conjectures are:

- Losing one transport observation must not disable submission presentation for the
  other transport, and an unresolved recipient must never become submittable.
- Cached reading must not imply current action permission; action provenance must
  belong to the same conversation projection as the content.
- Reusing SQLite statements must preserve every record and must not weaken rollback
  when a later workspace/personal write fails.

The affected Rust client suite passed (10 tests; the separate packaged search
experiment remained ignored). The strengthened existing native-page failure scenario
also passed, confirming an external-current result survives native-page failure.
Client/FFI lint checks with all targets/features and formatting checks passed.
Logs: `/tmp/cs-mail-swift-audit-rust-tests.log`,
`/tmp/cs-mail-swift-audit-partial-refresh.log`,
`/tmp/cs-mail-swift-audit-clippy.log` and `/tmp/cs-mail-swift-audit-build.log`.

The native harness exercises mixed transport presentation gates, more than 64 MiB
of independent mailbox records, every record after statement reuse, process
exclusivity, and injected submission rollback. Its isolated Keychain fixture never
substitutes for the original member account. Browser enrollment is explicitly skipped
unless separately configured. Native results and deployment inspection are recorded
in the current Apple runbook.

No PostgreSQL/domain-transition suites were rerun: this follow-up changes client
projections, presentation coordination and the existing SQLite adapter, without a
server schema or decision change. Live payment/bank providers, iOS, exhaustive
transport failures in the real UI, and production-size mailbox latency remain
uncovered. The prior full Rust quality-gate evidence is not a substitute for these
limits. Existing unrelated presentation edits remain intact.

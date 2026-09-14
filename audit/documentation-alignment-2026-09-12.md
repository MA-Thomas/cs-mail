**Documentation alignment audit — 12 September 2026**

The current implementation substantially follows the revised relationship-request economics, but it does not yet satisfy the complete September specification. The largest remaining concerns are shared-history admission, durable receipt ordering, authentication at the transactional admission boundary, financial signing scope, and lifecycle privacy. The README describes an older economic model and needs revision.

This review covers the current working tree, including its pre-existing uncommitted changes and the new finance crate/migration. It is not limited to the committed revision or the diff. No implementation changes were made for the audit. Reproduction experiments use a separate workspace under `/private/tmp/cs-mail-alignment-audit`.

**Authority and scope**

I read README.md and all seven current document sources: the protocol specification (draft 0.7), financial deployment profile (0.6), Rust architecture (0.8), whitepaper (0.6), express-lane memo (0.3), express-lane implementation plan (0.4), and desktop build plan (0.2). These are the editable `.tex` sources identified by the README. `intro_doc.tex` is explicitly superseded and was not used as a requirement. PDF rendering and source/PDF parity were not part of this code audit.

The protocol specification controls common behavior; the C-SQD Financial Program section is additionally normative for that profile. Architecture and product milestones describe targets, not assertions of completed implementation. Proposed rates, eligibility thresholds, cooldown durations, and live financial policy are not treated as approved production requirements.

The review traced production code across all 17 crates, the seven migrations, CI, and relevant tests. Priorities below indicate implementation risk: P1 should be resolved before declaring the semantic baseline complete; P2 is a material correctness, integration, privacy, or documentation issue. Static findings include the precise execution sequence needed to validate them.

**Findings**

**1. [P1] A prepared alias can bypass and shorten a newer principal-wide cooldown.**

[Admission authority](/Users/thomm15/Documents/cs-mail/crates/cs-mail-protocol/src/lib.rs:1798) checks `request.terms.eligibility_time`, but does not compare the current shared `attempt.earliest_next_admission` or attempt version. [Successful admission](/Users/thomm15/Documents/cs-mail/crates/cs-mail-protocol/src/lib.rs:1130) then replaces that shared eligibility time with `now + quoted_backoff`.

Confirmed by a failing conformance reproduction in the isolated workspace. Concrete sequence with the existing fixture policy: identity A admits at time 3, making another request eligible at 8. Alias B prepares and captures at 8. A is rejected at 9, moving the shared cooldown to 99. B admits at 10 using its older terms and moves eligibility back to 15. PostgreSQL's shared-history row lock serializes these operations but does not repair the missing check: it loads the newer history into this same kernel. This defeats the anti-abuse continuity promised by the [specification](/Users/thomm15/Documents/cs-mail/cs_mail_protocol_spec.tex:411) and the deployment rule that cooldowns remain subject to later principal-history bounds.

Remedy: validate preparation against current shared history at initial admission; preserve the strongest outstanding eligibility bound. Define cancellation/refund treatment for a captured preparation whose authority has become stale. Add a multi-session alias test covering this sequence, not only sequential identity rotation.

**2. [P1] Timely decisions have no durable receipt queue or expiry barrier.**

[Ingress](/Users/thomm15/Documents/cs-mail/crates/cs-mail-service/src/lib.rs:300) samples time and immediately calls execution. [Storage](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/lib.rs:461) acquires locks and creates the journal entry inside the same transaction that executes the transition. There is no independently committed authenticated-receipt record, pending-decision lookup, or watermark/holdback check. [Expiry](/Users/thomm15/Documents/cs-mail/crates/cs-mail-protocol/src/lib.rs:1496) examines request state and the supplied time only.

A decision that arrives before the deadline but is delayed before its execution transaction can lose to expiry. Retaining the earlier in-memory timestamp cannot rescue the financial outcome once expiry commits. A crash before that transaction also loses the receipt time. The repository cannot represent the required story of an already durably received timely decision awaiting processing. This is a missing conformance mechanism, not a claim that signed receipts are currently returned before commit.

The [specification](/Users/thomm15/Documents/cs-mail/cs_mail_protocol_spec.tex:205) and [queue-ordering requirement](/Users/thomm15/Documents/cs-mail/cs_mail_protocol_spec.tex:406) explicitly require that story. Persist authenticated receipt before asynchronous/contended execution and make expiry consult an authoritative receipt boundary. Test a pre-deadline receipt processed after an expiry worker starts, including restart.

**3. [P1] Free-message admission can commit against a revoked operational key.**

[Native free admission](/Users/thomm15/Documents/cs-mail/crates/cs-mail-service/src/lib.rs:437) reads the registry and verifies the signature before calling storage. [The storage transaction](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/lib.rs:789) checks relationship, content, and lane state, but neither reloads key authority nor guards the registry version. Recipient lane grant/control follow the same split validation pattern. Ordinary protocol execution already demonstrates the missing protection with [lock_registry_version](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/lib.rs:475).

If revocation commits between signature verification and admission, a stale verified request can still produce delivery. The same pattern can admit a grant/control from an authority revoked before its transaction. This conflicts with [compromised-device revocation](/Users/thomm15/Documents/cs-mail/cs_mail_protocol_spec.tex:502). Carry verified registry version/evidence into these transactions and check it under the same ordering boundary; test revocation between verification and commit.

**4. [P1] Financial commands and payment identifiers omit deployment scope.**

[SignedProgramCommand](/Users/thomm15/Documents/cs-mail/crates/cs-mail-finance/src/program.rs:714) signs a label, settlement unit, idempotency key, revision, and command. It has no deployment, operator/program instance, or protocol-version target. [Financial execution](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/finance.rs:84) cannot check a target that is absent. Two deployments sharing an administration key and compatible state accept the same signed instruction; their local idempotency tables do not prevent the cross-deployment replay.

[PaymentOperation::digest and request_payment_id](/Users/thomm15/Documents/cs-mail/crates/cs-mail-finance/src/payments.rs:26), and [member allocation IDs](/Users/thomm15/Documents/cs-mail/crates/cs-mail-finance/src/program.rs:657), similarly have no explicit deployment/provider account scope. Request relationship handles can incidentally provide separation, but allocation IDs depend only on unit, quarter, and member. A domain label identifies a message type; it does not identify a deployment. The [interoperability requirement](/Users/thomm15/Documents/cs-mail/cs_mail_protocol_spec.tex:734) requires both deployment and version replay isolation. Bind these targets explicitly and add cross-deployment tests using the same signing key and input IDs.

**5. [P2] Request admissions do not evaluate current admission policy.**

[submit_with_artifacts](/Users/thomm15/Documents/cs-mail/crates/cs-mail-service/src/lib.rs:296), used for `AdmitRequest` and `AdmitFollowup`, does not invoke `AdmissionPolicy`. [Content validation](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/lib.rs:1664) checks bindings and canonical declaration structure, but not supported critical schemas. The service applies that policy at upload and on the separate free-admission paths.

Upload under a policy supporting critical schema X, then restart with a policy that does not support X: an initial or follow-up request admission still succeeds from the stored object. This violates [critical-extension refusal before admission](/Users/thomm15/Documents/cs-mail/cs_mail_protocol_spec.tex:467). In addition, an upload refused after request capture does not itself cancel/refund the preparation; that is deferred to an explicit cancellation or timeout, despite the specified unsupported-extension unwind behavior.

Evaluate current supported declarations at the admission boundary, with a version/equivalent guard if policy can change. Preserve refusal-without-financial-effects for follow-ups and atomically unwind a preparing initial request when its admission is definitively refused.

**6. [P2] Deleting ciphertext leaves complete request/formation linkage indefinitely.**

[persist_manifest](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/lib.rs:2080) stores the complete manifest, including the full `next_state`, on every idempotent command. The live aggregate also retains all requests and quotes. Those copies include public parties, original payment tokens/references, message references, declarations, and financial history. [The only implemented purge](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/lib.rs:1137) deletes ciphertext. There is no corresponding compaction or purge for completed request history, quotes, receipts, idempotency manifests, capability admissions, or financial/member activity history.

Migration 5 adds nullable deletion fields to some tables, but normal writes never populate them and no worker consumes them. Ending content retention therefore does not erase the relationship formation linkage. The [record-lifecycle requirements](/Users/thomm15/Documents/cs-mail/cs_mail_protocol_spec.tex:632) explicitly separate persistent permission and necessary unpaid obligations from shorter-lived formation history. Define per-field retention, compact idempotency results to minimal replay evidence, and delete every unnecessary copy while preserving outstanding obligations.

**7. [P2] Content deletion ignores its persisted retention hold.**

[purge_expired_content](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/lib.rs:1140) selects by `content.expires_at` and undelivered outbox work. It does not consult `cs_retention_records.hold_until` or that record's `delete_after`. A content object with a future hold and no pending delivery is deleted as soon as its original expiry passes, and its retention row is marked deleted.

This is distinct from missing broader retention: a retention control already present in the schema has no effect. Make both eligibility selection and deletion honor the record's policy/hold under the transaction. Test expired content with a later hold, then hold expiry, and verify unrelated objects still delete. See the [independent retention/hold requirements](/Users/thomm15/Documents/cs-mail/cs_mail_protocol_spec.tex:654).

**8. [P2] Standing acceptance makes the legacy free-admission API reject delivery.**

[admit_legacy_bond_free](/Users/thomm15/Documents/cs-mail/crates/cs-mail-service/src/lib.rs:510) always supplies `Some(capability)` and legacy evidence. [Storage's accepted branch](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/lib.rs:835) rejects any request containing either field. Thus a DMARC-authenticated legacy sender that is accepted cannot use the only legacy service admission method, even with an otherwise valid lane. Acceptance can break previously working lane delivery.

The documented order is [blocked, accepted, capability, follow-up, new request](/Users/thomm15/Documents/cs-mail/cs_mail_protocol_spec.tex:489). Support an authenticated legacy accepted path without requiring lane authority, and test a lane-authorized legacy sender before and after acceptance. Ensure the chosen authority and ciphertext binding stay consistent.

**9. [P2] Native uploads have no server-side ciphertext size bound.**

[upload_content](/Users/thomm15/Documents/cs-mail/crates/cs-mail-service/src/lib.rs:533) bounds time and verifies certificate/declaration scope, but never checks ciphertext or encapsulated-key lengths. [Storage](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/lib.rs:1043) serializes those vectors directly into PostgreSQL. The 32 MiB check in [encrypt](/Users/thomm15/Documents/cs-mail/crates/cs-mail-content/src/lib.rs:155) is a cooperative client helper, which an external caller can bypass by constructing `EncryptedContentRecord` directly.

This contradicts the README's bounded encrypted-content upload claim and leaves memory/database resource consumption unbounded at the implemented service boundary. Validate ciphertext/envelope sizes and supported structural parameters server-side; add an over-limit upload test using a valid certificate. A future HTTP body limit should complement this check.

**10. [P2] Follow-up errors conflate message expiry, request expiry, and closure.**

[admit_followup](/Users/thomm15/Documents/cs-mail/crates/cs-mail-protocol/src/lib.rs:1767) returns `DecisionWindowClosed` for all three conditions. For example, an open request whose decision deadline is 53 receives a follow-up at 6 with message validity 5: the response says the decision window is closed even though another fresh follow-up may still be allowed. The protocol error enum lacks `RequestClosed` and `MessageValidityClosed` variants.

An isolated conformance reproduction confirms this error. The [stable-error table](/Users/thomm15/Documents/cs-mail/cs_mail_protocol_spec.tex:445) deliberately assigns different recovery behavior to these cases. Return distinct errors without changing the open request's economics. Test each condition independently and verify client recovery can distinguish renewing a message from waiting for another request.

**11. [P2] The README contradicts both the current specification and implementation.**

[README's model](/Users/thomm15/Documents/cs-mail/README.md:32) still describes per-attempt `C + S + L_k`, releases of persistence reserves, and payment of rejected collateral to the recipient. The implementation now charges requests, creates original-payment refund obligations, and sends forfeitures to the member program; the specification explicitly forbids recipient rejection entitlements. The README also calls the entire deployment profile nonnormative, omits the finance crate, dates the drafts to August, and says there are four migrations instead of seven.

Its PostgreSQL test command omits `-- --ignored`; all current storage integration tests are marked `#[ignore]`, so setting the database variable alone does not execute them. CI correctly supplies `--ignored`. Rewrite onboarding, status, vocabulary, crate inventory, and test instructions against the current drafts. Keep the explicit distinction between implemented simulator/reference code and future production work.

**Alignment map and planned work**

| Area | Assessment from code and tests |
| --- | --- |
| One request / one charge | Implemented in the kernel; a partial database index prevents duplicate current directed-pair aggregates and shared preparation state coordinates aliases. The later-cooldown race in finding 1 remains. |
| Request outcome accounting | Acceptance returns C+S, rejection records C and a forfeiture, expiry returns S, and preparation cancellation returns captured amounts. No spendable user/recipient ledger account remains. |
| Follow-ups | Implemented count/rate policy, one deadline, no new charge/history increment, atomic outbox, and exact command replay. Findings 5 and 10 remain. |
| External payments | Signed operation/amount evidence, stable request operation IDs, lookup-before-submit, late-capture refund, separate refund completion, and corporate-loss compensation exist. Cross-deployment scope and operational integration need work. |
| Member program | Separate member IDs, duplicate-identity rejection, intentional-day activity, calendar cutoff, explicit maturity clearance/holds, per-rate-cohort flooring, once-only assessment, equal shares, restricted remainder, and unpaid/reversed payable handling are implemented. No live financial policy is inferred from fixtures. |
| Durable atomicity | Protocol state, ledger, shared history, schedules, outbox, and forfeiture ingestion use the transaction boundary. This is separate from the missing durable receipt queue. |
| Crypto/wire | Canonical re-encoding, scoped command signatures, authenticated HPKE bindings, operational content-key certificates, and transparency/recovery primitives exist. Endpoint verification/key custody journeys and cross-domain access isolation are not completed product features. |
| Express lanes | Signed purpose/origin scope, hard/sliding lifetime, count/rate consumption, and transactional block revocation exist. Findings 3 and 8 remain. Browser handover/reply issuance UX and adoption migration are future integration work. |
| Privacy | HMAC-scoped references and safe telemetry types exist, but all durable domains use the same Postgres client/credential and are joined by aggregate keys. `REVOKE ... FROM PUBLIC` does not establish the separate service roles, encryption keys, or audited translation authority required by the docs. This is an architectural conformance gap, alongside findings 6–7. |
| Migration/versioning | New protocol/wire/store versions and refusal of old stored formats are implemented; migration 7 does not silently reset old balances. An actual conversion/obligation migration procedure is not supplied. |
| Desktop/service/CLI | The planned apps, portable client core, API contracts, OS key custody, durable simulator, and administrative product flows are absent. The plan explicitly makes them later milestones; their absence is not evidence that an already claimed milestone regressed. |
| Live rails / SMTP operations / federation | No production readiness claim is warranted. The docs explicitly defer these deployment gates; pure adapter primitives are not end-to-end operational evidence. |

The existing tests cover substantial intended behavior, but ordinary workspace success alone would not prove the full baseline. Missing integration evidence includes durable delayed decisions, concurrent aliases with newly imposed cooldowns, registry revocation between verification and admission, post-upload admission-policy changes, cross-deployment financial replay, enforced cross-domain credentials, and retention through backup restoration. The in-memory identity-rotation test copies history between isolated engines; it is not a simultaneous multi-alias database race test.

**Validation**

- `cargo +stable test --workspace`: **67 passed, 0 failed, 9 ignored**, including the compile-fail documentation test. The installed stable toolchain is Rust 1.91.0. [Full output](/Users/thomm15/Documents/cs-mail/audit/workspace-tests.log).
- `cargo +stable fmt --all -- --check`: **passed**.
- `cargo +stable clippy --workspace --all-targets --offline -- -D warnings`: **failed** on three `manual_is_multiple_of` diagnostics in the calendar leap-year expression at finance/program.rs:94. This is a Rust 1.91 check, not evidence that the pinned 1.88 Clippy fails. [Output](/Users/thomm15/Documents/cs-mail/audit/clippy.log).
- The documented pinned `cargo test --workspace` could not start: downloading Rust 1.88.0 timed out after retries. No claim of pinned-toolchain verification is made. [Setup output](/Users/thomm15/Documents/cs-mail/audit/pinned-toolchain.log).
- Two added conformance checks were run **only in the isolated copy**. Both failed as expected, confirming findings 1 and 10: the alias is admitted at 10 and reduces eligibility from 99 to 15; a stale follow-up reports `DecisionWindowClosed` even though its request remains decision-open. [Output](/Users/thomm15/Documents/cs-mail/audit/reproductions.log), [reproduction patch](/Users/thomm15/Documents/cs-mail/audit/reproductions.patch). These assert the documented behavior and expose current failures; they are not changes to the repository test suite.
- No database URL or local PostgreSQL/container runtime was available. The nine ignored integration tests were **not executed**; transaction/race findings other than the kernel reproduction are supported by static code tracing.

The reproduction command was:

```sh
cargo +stable test --manifest-path /private/tmp/cs-mail-alignment-audit/Cargo.toml \
  -p cs-mail-application --offline audit_ -- --nocapture
```

No production code or existing documentation was edited. The only workspace additions are this audit and its evidence files.

> The current breaking update and validation scope are documented in [Rust domain update](rust-domain-update.md). Earlier stage descriptions below are historical.

# Shared identity refactor validation

Validated locally on 2026-09-20 across the two working trees. Changes are local; no commits, pushes, deployment or live-provider configuration were performed.

## What changed

- Durable enrollment ownership is separate from expiring authentication attempts. Renewal preserves IDs; eligible, review and denied attempt outcomes are durable.
- Account persistence and services are independent of relationship aggregates. Key changes serialize with receipt creation without draining unrelated queued work.
- Memory and PostgreSQL repositories share fresh signed-decision activation and the same behavioral conformance scenario. Cached verified capabilities are no longer enrollment inputs.
- Authority snapshots validate account/persona/key ownership on construction and deserialization.
- The shared service uses asynchronous PostgreSQL with explicit host-owned connection lifetime. Internal errors retain their causes; transport returns sanitized codes.
- Confirmation delivery continues after individual failures, schedules transient retries and exposes intervention plus explicit administrative retry.
- Account domain code no longer builds wire intents or republishes the transport crate. Intent construction lives in the application layer.
- Public fixture constructors and synthetic government-ID onboarding were removed from the identity production library. Development support crates replace cross-crate/private source includes.
- Obsolete API-absence checks, demo golden-output assertions and source-layout tests were removed. Existing consequential authentication, financial and protocol regression checks remain.

Fresh development schemas are required. No compatibility path, forward migration, legacy runtime mode or data-preservation mechanism was added.

## Verification

The complete cs-mail workspace suite passed with PostgreSQL and loopback HTTP enabled. The identity workspace suite passed with OIDC verification enabled, and identity-model's all-feature suite passed. Its default-feature tests/examples also compile independently.

Strict Clippy passed for every cs-mail target and for identity-contract, identity-enrollment and identity-test-support. cs-mail formatting, formatting of changed identity sources, and both repositories' whitespace checks passed. Broader pre-existing identity-model lint issues were not represented as a clean workspace-wide Clippy result.

Behavioral evidence covers conflicting ownership, stale/wrong-context decisions, stable renewal IDs, rejection of old attempts, exact committed replay, subject uniqueness, durable review outcomes, concurrent retries, transaction rollback, confirmation starvation, lost responses, HTTP transport and receipt-time authority after revocation. PostgreSQL tests use isolated schemas in a temporary PostgreSQL 17.6 server.

For reproducibility, test totals (not a measure of design quality):

- cs-mail workspace including database/HTTP: 139 passed, 0 failed, 0 ignored.
- identity workspace with OIDC: 164 passed, 0 failed, 0 ignored.
- identity-model all features, overlapping the workspace tests: 149 passed, 0 failed, 0 ignored.

Logs are `/private/tmp/identity-refactor-cs-tests.log`, `/private/tmp/identity-refactor-identity-tests.log`, `/private/tmp/identity-refactor-features.log`, `/private/tmp/identity-refactor-clippy.log` and `/private/tmp/identity-refactor-identity-clippy.log`.

## Scope and deliberate choices

The host uses one asynchronous database connection per instance; transactions are short and coordinated across instances through database locks/constraints. Renewal waits for the existing challenge to expire, deliberately avoiding overlapping attempts. A transient confirmation waits 30 seconds before retry; rejected items require intervention. These choices are documented in the [design decisions](identity-refactor-design.md).

Environment-gated provider harnesses do not establish that real OIDC/bank providers were exercised. Phoros policy, account linking, verified recovery, bank replacement and shared security-event propagation remain separate features. No production hosting was configured. Before publishing cs-mail, publish the matching identity-model revision and set `IDENTITY_MODEL_REF` to its reviewed commit.

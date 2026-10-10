# cs-mail Apple client

SwiftUI presentation over `cs-mail-client::member`, exported through
`cs-mail-client-ffi` with UniFFI 0.31.0. Domain and identity decisions retain their
existing Rust owners. Minimum development targets: macOS 14 and iOS 17.

Implemented: browser sign-in and first-device enrollment; encrypted device-state
persistence; reopening the same device; inbox/request reading and accept/reject;
durable decision retry; explicit offline, denied-custody, review and expiry errors.
The interface requires fresh synchronization before enabling decisions.

## Build and run

```sh
scripts/apple-build.sh
open target/apple/CsMail.app
```

The script generates bindings, builds the native static library and Swift package,
and produces an ad hoc signed development app. Generated binding files are ignored
and regenerated from the Rust interface. The Mac app uses a single window and a
serial worker queue; blocking network work never runs on the UI actor.

For a controlled local backend, create two **empty, disposable** PostgreSQL
databases, then run:

```sh
CS_MAIL_DEMO_DATABASE_URL=postgresql://localhost/apple_mail \
CS_MAIL_DEMO_IDENTITY_DATABASE_URL=postgresql://localhost/apple_identity \
  scripts/apple-local.sh
```

Keep the launcher running. It prints its temporary configuration/log directory and
starts cs-maild on port 8480, the existing identity-model application on 8481, and
the fictional OIDC provider on 8482. Ctrl-C stops those processes. It creates no
hosted resources and sends no external email. Banking and service charges are
simulated. Use fictional applicant and bank details in the app, then select Alice
or Bob in the browser. Use the same details after an interrupted enrollment.
Each fictional login can enroll only once in a fresh deployment. The native
validation harness uses Bob; Alice remains available for manual enrollment.

Device enrollment does not buy annual service or publish request classes. Those
remain separate workflows; until their screens arrive, provision development
members through the existing signed member APIs/CLI. The app displays an empty
inbox until a sender has submitted a request to a properly configured recipient.

## Existing identity model and browser configuration

The client calls `/v1/enrollments`, `/complete`, `/renew` and the reconciliation
endpoint. It uses the existing enrollment intent/device proof and never decides
subject matching, login ownership, bank assurance or review outcomes. The local
`browser-identity` host uses the real identity application, PostgreSQL adapter and
`OidcJwksSessionVerifier`; only its loopback HTTP fixture hosting differs from the
production HTTPS host. The fixture emits signed RS256 JWTs, single-use codes and
S256 challenges. It does **not** establish a real person's identity.

`cs-maild` publishes this optional public configuration at `/v1/deployment`:

```json
{
  "browser_login": {
    "issuer": "http://127.0.0.1:8482",
    "client_id": "cs-mail-apple",
    "redirect_uri": "com.csqd.member:/oauth/callback"
  }
}
```

Register a public native client with code flow, S256 PKCE and this exact callback.
Issuer/client ID must match the identity service's OIDC configuration. A production
issuer and endpoints use HTTPS. Only loopback HTTP is allowed for local work.
Browser state, code verifier, tokens and raw applicant claims are transient. The
encrypted reservation stores a details fingerprint and the existing enrollment
intent, so restart resumes the same ownership claim. It never persists the ID token.

“Open account” reopens a device already enrolled here. Existing-account enrollment
on a replacement device remains a separate task using the identity model's
`RecoverDevice` contract; there is no new-account fallback and no profile importer.

## Local data and interrupted work

Keychain service/account: `com.csqd.member.dev` / `member-device-v1`, device-only,
unlocked access. SQLite: `Application Support/com.csqd.member.dev/member.sqlite`.
It contains one authenticated encrypted snapshot, using a separate Keychain cache
key. Cached message records remain HPKE ciphertext even inside that snapshot.
SQLite uses full synchronous commits; a process lock prevents concurrent owners.
No secret/debug formatting or HTTP response bodies appear in client errors.

Recipient-key registration and decisions persist the exact signed operation before
sending it. A lost response retains uncertainty; retry replays those bytes. Only a
validated provider receipt resolves a decision journal, followed by a canonical
refresh. The UI clears plaintext when locked/backgrounded and discards late results
from an older presentation session. Browser sign-in can be cancelled explicitly.

## Validation and current limits

The agreed claims concern secret/plaintext leakage and incorrect outcomes after
interruptions. Rust experiments challenge expired/substituted browser responses,
restart of enrollment, lost decision responses, failed local commits, ciphertext
integrity and denied key access during content retrieval. Existing service and
PostgreSQL suites cover canonical decisions and idempotence.

Run the native checks against a fresh local deployment:

```sh
CS_MAIL_APPLE_SERVER=http://127.0.0.1:8480 \
  apps/cs-mail-apple/.build/debug/CsMailAppleCheck
```

The harness checks native SQLite durability/exclusivity, actual isolated Keychain
creation/reopen/overwrite refusal, and Swift-to-Rust browser enrollment against the
existing identity service. Its fictional browser approval is performed over HTTP;
it does not automate Safari. Temporary native test credentials/files are removed.
This executable is used because the installed Command Line Tools lack XCTest.

Validated locally on 8 October 2026: macOS build and launch, enrollment form and
system authentication session startup, the controlled login page in Chrome,
native persistence/Keychain/FFI enrollment
checks, and the repository quality gate including PostgreSQL suites. Visual
inspection of the Safari authentication window was unavailable because Computer
Use lacked permission. Full visual sign-in, an interactive request decision, real
provider/bank assurance, signing/notarization and App Store distribution remain
unverified. This is the first development slice, not the full Milestone 4 exit.

An iOS Xcode target shares the same Swift package and Rust facade:

```sh
# Requires full Xcode and the relevant installed rustup target:
# aarch64-apple-ios-sim or aarch64-apple-ios
scripts/apple-ios-build.sh simulator
scripts/apple-ios-build.sh device
```

Full Xcode and iOS Rust targets are absent on the current machine. The project
plist parses, but iOS compilation, packaging and suspension/device behavior have
not been validated. The local loopback provider works on the Mac or simulator;
a physical phone needs a reachable HTTPS development deployment.

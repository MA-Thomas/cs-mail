# Apple member application

Decision agreed 8 October 2026. Build the member application with SwiftUI,
shipping macOS first and designing for iPhone and iPad from the outset. This
replaces the planned Tauri shell; there is no implemented Tauri application to
migrate or retain. The first native implementation now lives in
`apps/cs-mail-apple`; its runbook and current evidence are in that directory’s README.

The [build plan](../cs_mail_build_plan.tex) owns sequencing and milestone exits.
The [Rust-domain principles](rust-domain-principles.md) and existing product,
identity, correspondence and consent contracts remain binding.

## Ownership and dependencies

The application follows this boundary:

```text
SwiftUI views and Swift presentation state
                  |
       narrow generated Swift bindings
                  |
       portable Rust client use cases
                  |
       authenticated service APIs
                  |
       Rust application and domain owners
```

Server Rust remains authoritative for account, permission, request, settlement,
billing and distribution decisions. The client never writes canonical tables.
The Rust client owns portable signing, content encryption/decryption, protocol
handling, command recovery and synchronization workflows. Existing client code
is reused and extended as needed; this decision does not claim those workflows
are all complete.

Swift owns screens, navigation, presentation state, accessibility and Apple
integration. Platform adapters implement the Rust client's ports for key custody,
local persistence, transport, time and lifecycle services as needed. The exact
adapter implementation language follows the platform requirement; domain rules
do not move into Swift or get duplicated at the binding boundary.

## Rust-to-Swift interface

Introduce the narrow client FFI facade during Milestone 4, alongside its first
consumer. Expose client use cases and checked results, not arbitrary domain
mutation or database access. Preserve distinct identifiers, bounded values,
explicit ownership and stable error meanings across the interface. Validate
foreign inputs using the same checked Rust construction paths.

Calls that may wait run asynchronously without blocking the UI. Define callback
delivery, object lifetimes and cancellation explicitly. Cancelling a UI task is
not evidence that a submitted server command was cancelled: preserve operation
identity and reconcile an unknown outcome before showing success or retrying.
Swift presentation updates return to the main actor.

The first slice uses UniFFI 0.31.0, a Rust static library and a Swift package.
The development targets are macOS 14 and iOS 17. The macOS build is validated;
iOS simulator/device packaging and execution still require full Xcode. An
XCFramework for distribution remains later packaging work.

## Keys, local data and platform behavior

Apple Keychain backs the key-custody port. Secret key material must not enter
SQLite, logs or diagnostics. SQLite holds ciphertext, key references, cursors,
minimized relationship metadata and nonsecret preferences. Plaintext needed for
reading or composition is transient; the UI does not create a plaintext cache.
Keychain denial, locked or unavailable keys, expired authentication and unavailable
network service have explicit states, not silent fallbacks.

Account authentication, retained-copy entitlement and scoped consent remain
separate authorities under the existing consent and key-custody contract. A new
screen or a successful login does not create decryption or disclosure authority.

Push notifications and background execution are platform services. Treat a
notification as a prompt to synchronize, not canonical evidence of a decision.
Client correctness must survive suspension and resume without relying on a
continuously running process. Configure each platform's key access and background
behavior deliberately rather than assuming macOS behavior transfers to iOS.

## Shared Apple interface and later platforms

Create macOS and iOS targets from the first slice. Share presentation models and
appropriate SwiftUI components while adapting navigation, windows, menus,
keyboard/touch interaction and accessibility to each device. Check the main
journeys on iPhone early; iPad layout belongs to the same Apple design. macOS is
the first release target; an iOS release date is not committed by this decision.

The guest request page remains a web interface with its own components. Share
terminology, design intent and API contracts with it; do not require SwiftUI and
the web page to share rendered components. The membrane/portal interaction
concepts remain independent of the rendering toolkit.

Later Android and Windows applications reuse the portable Rust client and supply
their own interfaces, bindings and platform adapters. Kotlin/Compose and C#/WinUI
are candidate interfaces, not selected implementations. Their binding support and
platform behavior require separate validation. Phoros's interface choice is a
separate decision.

## First implementation slice and validation

Start with one local member journey: browser sign-in and first-device enrollment, reopen it,
synchronize a request, read its content, and accept or reject it through the
existing authenticated service. Establish Keychain custody and encrypted local
storage in this slice. Then expand to existing-account device recovery, composition, external
conversations and disclosure controls, request classes, lanes, blocks and
financial views. Milestone 4's full exit remains the build plan's responsibility.

The agreed experiments challenge the existing milestone
claims: secret leakage through persistence or diagnostics, and disagreement with
canonical server outcomes after quit/reopen or projection rebuild. Consider
Keychain denial, interrupted commands, expired authentication, offline use and
iOS suspension as concrete conditions for those experiments. The FFI packaging
check must include macOS, an iOS simulator and an iOS device build; device runtime
behavior requires its own execution evidence. The current runbook separates
completed local checks from unavailable platform and provider checks.

Development remains local against simulated payments and controlled email
transports. Public hosting, MX, HTTPS and actual provider callbacks stay deferred
until the macOS application is built and locally validated. No provisioning or
hosting payment is part of the client implementation stage.

## Enrollment boundary

The Apple client reuses `identity-contract::EnrollmentIntent`, the existing
cs-mail enrollment endpoints and the sibling identity-model application. Identity
resolution, OIDC signature verification, bank evidence, uniqueness and review
remain there. The client adds browser ceremony orchestration (authorization code,
PKCE, state and nonce), Keychain device-key generation, and recoverable registration
of recipient/content keys. It does not define another identity model.

The controlled local OIDC fixture supplies signed fictional login evidence to the
existing `OidcJwksSessionVerifier`. This validates wiring, not real-user assurance.
Returning to the same enrolled device uses its registered keys. Signing an existing
account into a replacement device must use the existing `RecoverDevice` contract;
that HTTP/UI ceremony is not implemented in this slice and must never fall back to
creating another account. No development-profile import path is retained.

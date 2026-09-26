# Request pricing: operator policy and recipient request classes

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

Status: agreed design, 25 September 2026; implemented (see
[Implementation](#implementation)). It replaces the
pricing-menu storage and the publication authority described in
[request classes and lane acceptance](request-classes-implementation.md); the lane
acceptance and settlement parts of that document are unchanged.

## Decisions

- CSQD is the only provider. There is one request pricing policy for all of
  cs-mail.
- The operator sets one processing component `C` for all of cs-mail.
- Each protocol identity (email address) publishes its own set of up to eight
  request classes. Addresses belonging to the same account have independent sets.
- Each class carries its own collateral `S`, which must lie within cs-mail-wide
  bounds `collateral_min <= S <= collateral_max`.
- When the operator changes `C` or the bounds, quotes already issued keep their
  terms. A published class whose collateral falls outside the current bounds
  cannot be quoted until the recipient republishes. Nothing is rewritten or
  clamped on the recipient's behalf.

## What the current implementation gets wrong

1. **Publication is bound to a relationship.** `SignedRequestClasses` signs over a
   `SigningScope` that names one relationship, and the PostgreSQL adapter
   authorizes it against that relationship's key registry. A recipient's
   publication is a per-address fact that should not depend on any sender.
2. **The operator menu is copied per recipient.** `cs_recipient_pricing.policy`
   stores `C` and the allowed collateral values in each recipient's row. The
   operator must configure every recipient, through a relationship-scoped
   `PostgresEngine`, before that recipient can publish.
3. **The menu has the wrong shape.** `RequestPricingPolicy` is a list of discrete
   `collateral_choices` plus a `default_collateral`, with hard-coded amounts in its
   `Default`. The product rule is a minimum and a maximum.
4. **The operator cannot change the policy.** `configure_request_pricing` rejects
   any policy that excludes a class some recipient already published.
5. **Deployment policy and request terms share a type.** `PolicySnapshot` carries a
   scalar `processing_charge`, `collateral`, `pricing_policy_version` and
   `selected_class`, which quote issuance overwrites with `apply_pricing`.

## Model

### Types

- `RequestPricingPolicy` (operator-owned): `version`, `unit`, `processing_charge`
  (`C`), `collateral_min`, `collateral_max`. The checked constructor requires a
  nonzero version, a nonzero `C`, `0 < collateral_min <= collateral_max`, and that
  `C + collateral_max` does not overflow. A published version is immutable.
- `RequestClass`: stable `RequestClassId`, recipient-authored description, and
  collateral `S`. Unchanged.
- `RecipientRequestClasses` (owned by one protocol identity): recipient address,
  publication version, and at most eight classes with unique IDs. Unchanged.
- `RecipientSigningScope`: deployment domain and provider, with no relationship.
  It is a distinct type from the relationship `SigningScope`, so a publication
  cannot be confused with a relationship command.
- `RequestPrice`: the result of quoting one class — policy version, `C`, `S` and
  the `SelectedRequestClass`. Quote issuance passes it to the kernel as its own
  value. `PolicySnapshot` no longer carries price fields.

### Rules

- **Publication.** A publication is accepted only if it is signed by an active
  key registered for `ActorRef::Recipient(address)` in the account that owns the
  address, and that account allows service. A key belonging to another address
  of the same account does not qualify. Every class must lie within the current
  policy's bounds, or the whole publication is rejected. Versions strictly
  increase; an identical replay succeeds without change. An empty publication
  means the address offers no paid request route.
- **Quoting.** One pure function takes the current policy, the recipient's
  publication and the chosen class ID. It returns a `RequestPrice`, or refuses
  with *unknown class* or with *class outside current policy* — distinct
  outcomes. Issued `RequestTerms` keep the policy version, `C`, `S` and the
  selected class, so the `C + S` charge and refund obligation never change later.
- **Operator change.** The operator publishes a new policy version through the
  authenticated administration API; the current version only advances. The
  result reports how many addresses now have classes outside the bounds.
- **Views.** A recipient sees each of their classes with whether it is currently
  quotable. A sender sees `C` and only the classes that can be quoted now.

### Ownership and storage

Operator policy and recipient publications are deployment-level state. They are
reached through the deployment-scoped storage handle (`PostgresDeployment`), never
through a relationship-scoped engine.

| Table | Content |
| --- | --- |
| `cs_request_pricing_policies` | Append-only policy versions with their publication time |
| `cs_request_pricing_current` | Singleton pointer to the current version |
| `cs_request_class_publications` | Append-only signed publications, keyed by address and version (evidence) |
| `cs_recipient_request_classes` | Current publication per address (projection of the log) |

Quote issuance, inside the relationship transaction, reads the current policy and
the recipient's current publication by address under share locks. Publication and
policy changes take the receipt-order lock and lock the rows they replace.
Migration 0019 drops `cs_recipient_pricing` (refusing if it holds rows) and creates
these tables; earlier migrations are unchanged.

### Removed without aliases

- `cs_recipient_pricing`, `PostgresEngine::configure_request_pricing` and the
  relationship-scoped `RequestClassesStore` implementation.
- `collateral_choices`, `default_collateral` and `RequestPricingPolicy::default`.
- Relationship-scoped signing and authorization of publications.
- The price fields of `PolicySnapshot` and `PolicySnapshot::apply_pricing`. Issued
  `RequestTerms` and their wire encoding are unchanged; the publication signing
  domain advances to `cs-mail/recipient-request-classes/v2`.

## Claims to challenge

All against PostgreSQL:

1. **A class outside the current bounds cannot be quoted, although its
   publication was valid when made.** Falsified if, after the operator narrows the
   bounds, a quote is issued for a class outside them. It matters because bounds
   that do not bind would let old classes escape policy indefinitely.
2. **A publication signed for one address cannot be applied to another, including
   another address of the same account.** Falsified if a publication signed with
   address A's key changes address B's classes. It matters because each address's
   classes are that address's own consent to paid approaches.
3. **A quote issued concurrently with a republication or a policy change carries
   the terms of exactly one publication and one policy version.** Falsified if an
   issued quote combines `C`, `S` or a description from different versions. It
   matters because the charge and refund obligation must match what was shown.

The class limit, unique IDs and bound ordering are enforced by checked
constructors and need no runtime tests.

## Sequencing

This is Milestone 1b of the [build plan](../cs_mail_build_plan.tex). It must land
before Milestone 2, whose request journeys depend on it.

## Implementation

Implemented 25 September 2026, together with the deployment-scoped storage handle.

- **Deployment handle.** `PostgresDeployment::connect` migrates and owns trusted
  configuration, accounts, billing, the financial program, deployment work queues
  (`DeploymentQueue`) and request pricing. `PostgresDeployment::relationship` opens a
  relationship `PostgresEngine`, whose queues are `RelationshipQueue`. Both implement
  `WorkClaims`. `PostgresEngine::connect` is removed.
- **Domain.** `cs_mail_protocol::pricing` holds `RequestPricingPolicy` (checked
  constructor and deserialization), `RequestPrice`, `SenderOffer`, `quote`,
  `sender_offer` and `class_status`. `QuotePricing::resolve` is the single rule that
  prices a requested class from the current policy and publication; both storage
  adapters call it. The kernel receives its result as `TransitionContext::pricing`
  (`NotRequested`, `Resolved` or `Refused(PricingRefusal)`) and surfaces a refusal
  only when a charge is required, as `PolicyInvalid`, `RequestClassUnknown` or
  `RequestClassOutsidePolicy`. The PostgreSQL adapter checks the payment arrangement
  only when the kernel requires a charge, as before the cutover.
- **Application.** `cs_mail_application::request_pricing::RequestPricingService`
  (`publish_policy`, `publish_classes`) over the `RequestPricingStore` port.
- **Signing.** `RecipientSigningScope` (deployment domain and provider).
- **Service.** `IngressService::set_request_classes`, `request_offer` and
  `request_class_status`.
- **Harness.** The in-memory kernel harness takes trusted pricing through
  `InMemoryStore::configure_request_pricing` and `configure_request_classes`, which
  apply the same bounds check. Authorized publication is exercised against PostgreSQL.

Decisions made during implementation:

- Publications take the receipt-order lock but do not require earlier received
  commands to be processed first. Prices are read when a command is processed and
  are never captured at receipt, and draining would refuse an address's publication
  whenever any command in the deployment was in flight. The concurrency test exposed
  this.
- Until the Milestone 1 administration API exists, `publish_policy` is a trusted
  startup API like the other `configure_*` calls, and policy versions record their
  publication time but not an authorizing administration command.

Open follow-ups:

- Issued terms record the class ID, description, `C`, `S` and policy version, but
  not the publication version. Recording it (for provenance) changes the terms wire
  format.
- Operator publication counts affected addresses by reading every current
  publication; this is linear in the number of addresses.

Validation: the three claims above are challenged by
`request_pricing::*` in `crates/cs-mail-storage-postgres/tests/postgres.rs`,
replacing the earlier `recipient_pricing_is_signed_and_changes_only_new_quotes`.
The concurrency test was checked to interleave: its quotes spanned several
publication and policy versions. Existing suites were adapted to the new APIs.

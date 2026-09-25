# Rust-domain design principles

> A copy of this document is kept at `identity-model/docs/rust-domain-principles.md`. Keep the two in step; only their relative links differ.

Shared architectural guidance for **cs-mail** and **identity-model**, agreed on
2026-09-20. This is the canonical reference for the ten principles discussed
during the refactoring review. These principles guide future work; they do not
assert that every existing component already satisfies them.

Make domain meaning explicit in types, express decisions as deterministic rules,
and enforce authority and consistency at the boundary where state changes become
durable. cs-mail contributes transaction and lifecycle modeling; the identity core
contributes evidence, provenance, temporal interpretation, and separation between
recorded facts and derived conclusions.

## 1. Model distinct meanings with distinct types

A subject, account, principal, persona, payment obligation, and payment attempt
are different concepts—even when they concern the same person or transaction.
Use scoped identifiers and dedicated value types so accidental substitution
becomes difficult.

The [product identity model](product-identity-model.md) distinguishes the shared
subject, one durable login identity per product, and its multiple authentication
methods. Phoros's future ceremony may adopt an existing cs-mail subject through
verified account control; it does not add a second login to either product.

## 2. Give each invariant an explicit owner

Relationships own communication permission; accounts own personas; billing owns
service contracts; finance owns obligations; identity owns subject evidence.
An aggregate should contain the state needed to enforce its rules. Cross-domain
coordination belongs in an application layer.

## 3. Represent lifecycles directly

Use enums and structured states to express meaningful alternatives and the data
each requires. Separate independent lifecycles: authentication can expire while
enrollment ownership persists; a payment attempt can fail while the obligation
remains owed.

## 4. Keep domain decisions deterministic and independent of infrastructure

Pass time, policy, evidence, and relevant state explicitly. Domain functions
compute decisions and consequences; they do not fetch credentials, call providers,
query databases, or obtain the current time themselves. This makes behavior
reproducible and adapters replaceable.

## 5. Separate evidence, interpretation, and authority

A signed assertion is evidence from a particular issuer, about particular claims,
in a particular context. Policy determines what it establishes. Authorization
determines whether it permits this action now. Historical assurance, successful
authentication, and permission to perform a sensitive action must remain
distinguishable.

## 6. Protect valid domain state at every entry point

Use private fields, checked constructors, validated transitions, and checked
deserialization. Treat incoming DTOs and restored database records as inputs
requiring validation. Constructing a valid Rust value establishes its internal
invariants; it does not establish current external authority or exclusive ownership.

## 7. Record facts with provenance; derive views for a stated purpose and time

Preserve the evidence, policy references, and causal relationships needed to
explain a conclusion. Materialized state is an interpretation of those records.
Corrections, revocations, and later evidence should have explicit semantics,
including how they affect historical versus current views.

## 8. Make the complete consequence of a decision explicit

A transition should identify the state changes, ledger effects, events, and
external-work intents that belong together. The persistence boundary commits that
set atomically and checks concurrency constraints. External work then proceeds
through recoverable, idempotent operations.

## 9. Treat uncertainty and retries as domain situations

“Outcome unknown,” “review required,” “expired,” and “denied” carry different
meanings. Preserve stable operation identities across retries, reject altered
retries, and reconcile uncertain results without inventing a second obligation
or ownership claim.

## 10. Use tests to challenge meaningful conjectures

Avoid adding tests unless there is a conjecture we are interested in falsifying,
or whose falsification would have a meaningful consequence. Before adding a test,
state the claim it challenges, the observation that would falsify it, and why that
result would matter. Prefer scenarios that seriously challenge the claim. Passing
tests mean that these attempts did not falsify it; they do not prove it universally.
Do not add tests merely to accompany code, increase coverage, or preserve superseded
behavior.

## The role of tests in development

Domain reasoning determines the architecture. Tests are targeted experiments that
challenge its consequential assumptions. Test-driven design is not the prescribed
development approach: business meaning, invariants, authority boundaries, and
operational requirements guide the Rust types and contracts.

Examples of meaningful conjectures for these projects include:

- Two concurrent enrollment attempts cannot establish conflicting ownership.
- A payment retry cannot create a second obligation.
- A disclosure permit that expires while waiting cannot authorize subsequent
  decryption.
- A failed transaction cannot leave ledger effects without their corresponding
  state changes.

Each claim has an identifiable counterexample and a material consequence if false.
A proposed test should create conditions under which that counterexample could
appear and observe the relevant outcome. Tests that merely repeat implementation
logic or assert private call sequences usually provide little challenge to a
meaningful claim.

Choose an experiment capable of challenging the conjecture. PostgreSQL locking and
atomicity claims require database integration checks; an in-memory adapter cannot
establish those properties. Shared behavioral scenarios across adapters are useful
when they challenge a consequential claim about their common contract. Keep
synthetic evidence and scenario builders in development support code.

Use Rust's newtypes, enums, private fields, and checked construction to enforce
structural guarantees. A guarantee already enforced by the type system generally
needs no redundant runtime test. Tests can instead challenge behavioral assumptions
that remain, such as authorization freshness, conservation, replay, and recovery.

A discovered bug can supply a concrete counterexample and justify a regression
test when the underlying claim still matters. Retain that test only while the claim
remains relevant; update or remove it when the contract changes. Tests must not
force replaced functionality or compatibility code back into the architecture.

Passing tests are limited evidence. Design review, compiler guarantees, database
constraints, and operational verification address additional aspects of correctness.

## Agreed application and persistence boundary

The application layer expresses use cases and orchestration in database-agnostic
Rust. Domain types and functions own business invariants and decisions. Rust
traits define the persistence contracts, with typed context and explicit effects;
PostgreSQL adapters implement their atomicity and concurrency requirements.

Security and correctness still require database enforcement: transactions, unique
constraints, locks, conditional writes, durable audit records, and lease-generation
fencing belong in storage. An application-owned decision can run as a local
callback under an adapter-owned transaction, using the complete protected context.
Sample trusted time after acquiring the required protection when freshness matters.
Keep external I/O outside database locks, and recheck authority after intervening
waits where the protected action requires current authorization.

## Agreed refactoring discipline

Remove replaced APIs, implementations, and functionality when making a cutover.
Do not retain compatibility wrappers, aliases, or parallel implementations for
superseded behavior. Keep one clear owner for each rule and contract.

The recent refactoring explicitly prohibited creating new tests. Principle 10
states the general validation standard; it does not override that instruction.
The implementation used and adapted existing suites without adding test cases.
Future work must follow its agreed testing scope and report coverage limitations.

## Implementation references

- [Domain update](rust-domain-update.md)
- [Application and persistence boundaries](application-persistence-boundaries.md)
- [Identity-model remaining work](../../identity-model/RESIDUAL_REFACTOR_DEBT_AND_NEXT_STEPS.md)

Cross-repository links assume sibling `cs-mail` and `identity-model` checkouts.

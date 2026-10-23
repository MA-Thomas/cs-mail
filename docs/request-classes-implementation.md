# Request classes and lane acceptance

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

> **Updated 25 September 2026.** Pricing and publication follow the
> [request pricing design](request-pricing-design.md), which replaced the operator
> menu and relationship-scoped publication. Lane acceptance and settlement are
> unchanged.

**Alias-bound implementation (21 October 2026).** Each bonded alias owns its
profile and up to eight request classes, independently of directory visibility or
address generation. Signed commands, current-version pointers and history keys bind
the selected alias; a signature for one alias cannot authorize an edit to another.
Public/private is no longer an alias kind. No purpose
taxonomy is hard-coded. A class has a stable `RequestClassId`, user-authored
description, and collateral within the operator's cs-mail-wide bounds; the
operator sets `C`.

`RequestPricingService::publish_classes` authenticates a `SignedRequestClasses`
publication against a key of the controlling protocol identity, with the selected
alias ownership checked under protected current authority.
`RecipientRequestClasses` checks the count and unique IDs, including on
deserialization. The PostgreSQL adapter keeps the signed publication and commits
the address's current classes. `IngressService::request_offer` exposes the
currently quotable classes to senders and `request_class_status` shows the
recipient which are quotable. Publishing an empty collection creates no implicit
class.

`IssueRequestTerms` names the receiving alias, frozen return route and `class_id`. Issued `RequestTerms` retain a checked
`SelectedRequestClass` and the fixed collateral. Class changes cannot rewrite
issued terms, partition principal-recipient history, or provide extra pending
request slots. There is no misclassification penalty beyond ordinary rejection.

## Permission and settlement

Standing acceptance uses the existing signed relationship command. Express-lane
acceptance uses the recipient-signed grant API, including a grant issued from
lane management. `ValidatedLaneGrant` validates grant contents; current signing
authority is established separately at receipt. Application-owned
`LaneAcceptance` coordinates the capability with protocol-owned settlement.

| Request at grant application | Financial consequence |
| --- | --- |
| Preparing submission | Cancel preparation; void capture or refund the full charge, including a capture that completes later |
| Submitted and still pending, including after the decision deadline | Accept and record the full `C + S` refund obligation |
| Already terminal | Preserve its existing settlement |
| No request | Establish lane access without a charge |

The lane, relationship status, request transition, refund obligation, schedules,
and events commit in one transaction. Provider refund execution happens later.
`RequestLifecycle::Accepted` records `AcceptancePermission`; the relationship's
`ExpressLane(LaneId)` state does not imply standing permission. Lane expiry or
revocation never changes the historical settlement.

A live grant removes the need for another request bond. A scheduled start still
restricts when messages can be admitted. Purpose, validity, allowance and block
checks apply to sends. Native correspondence records the admission basis and
commits lane consumption with encrypted delivery and its replay receipt. The
lane is relationship-level, usually not tied to a conversation; conversation
reuse policy remains separately enforced.

## Cutover and verification

Migration 0044 keys current and historical class publications by alias, removes
the single-public-alias constraint and records an explicit correspondence route.
Class signatures use domain v3; directory commands use v2; request commands and
terms use v7. Superseded readers and creation APIs are removed. Populated schema-43
records require the explicit preservation-checked owner upgrade; it archives
original signed evidence without inventing signatures. The active local owner
copy and validation evidence are recorded in the Apple runbook.

The existing pricing, lane and correspondence scenarios are extended to challenge
publication limits and tampering, immutable quote contents, preparation and late
settlement, rollback on failed lane persistence, replay, scope, and shared lane
allowance across conversations. No additional test cases were introduced.
The in-memory protocol harness remains a trusted kernel harness; PostgreSQL
exercises the authenticated publication, grant and correspondence boundaries.

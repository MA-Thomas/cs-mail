# Request classes and lane acceptance

Recipients publish up to eight of their own classes. No purpose taxonomy is
hard-coded. A class has a stable `RequestClassId`, user-authored description, and
collateral selected from the operator's menu; the operator still sets `C`.

`RequestClassesService` authenticates a `SignedRequestClasses` publication under
protected current authority. `RecipientRequestClasses` checks the count and
unique IDs, including on deserialization. The PostgreSQL adapter commits the
whole versioned publication. `IngressService::request_classes` exposes it for
sender selection. Publishing an empty collection creates no implicit class.

`IssueRequestTerms` includes `class_id`. Issued `RequestTerms` retain a checked
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

Migration 0018 adds checked class-publication storage and protocol record format
9. Authenticated command and quote signing domains advance to v6. Superseded
scalar-preference APIs are removed. This is a coordinated release requiring a
fresh database or a separately reviewed migration of existing records; it does
not invent descriptions for old preferences or reinterpret old signed requests.
Only the isolated test database was migrated during development.

The existing pricing, lane and correspondence scenarios are extended to challenge
publication limits and tampering, immutable quote contents, preparation and late
settlement, rollback on failed lane persistence, replay, scope, and shared lane
allowance across conversations. No additional test cases were introduced.
The in-memory protocol harness remains a trusted kernel harness; PostgreSQL
exercises the authenticated publication, grant and correspondence boundaries.

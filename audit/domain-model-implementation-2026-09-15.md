# Domain-model implementation

Implemented September 15, 2026, using the user's clarified product rules. This is a clean replacement of the deprecated financial model. The [current domain reference](../DOMAIN_MODEL.md) and the seven companion document sources now describe the corrected model.

## Result

- **One annual allocation, one bank payment.** `MemberPayable` has one payment execution, with sequential attempt history. Its reporting values are derived as `rebate = min(D, U)` and `excess = D - rebate`. Statements expose `rebate_amount` and `excess_amount`, independently of payment status. Positive allocations are paid in full; zero allocations create no processor operation.
- **Automatic distribution to the associated verified bank account.** Distribution preparation takes an allocation and canonical time. It resolves the beneficiary's account and bank association. The owner-supplied payout destination, minimum, refund reservation, and distribution-instruction APIs were removed. Pool finalization atomically queues distribution preparation under published payment terms.
- **Renewal-independent payment.** Distribution has no dependency on service collection or following-year renewal. Closing the account preserves the financial association needed to pay an outstanding allocation.
- **Fixed annual service contracts.** `ServiceOffer` publishes the annual period, advance collection date, price, unit, and policy version. `ServiceContract` preserves those facts across retries. Collection does not start coverage early or extend its end. `ServiceContractId` and `PaymentOperationId` are distinct Rust types.
- **One configured processing arrangement.** Bootstrap checks a single scope/unit/processor authority and bank-verification authority. Service and request charges use that arrangement. Conflicting configuration is rejected.
- **Explicit account authority.** Signed account actions use revocable operational keys and explicit account grants. They reference published service offers; they cannot set prices, processor keys, or payout routing. Bank verification uses separately authenticated evidence. Account registration enforces person/member uniqueness and alias ownership. Durable pool enrollment must match the verified person associated with the account.
- **Explicit dispatch authorization.** `authorize_utility_dispatch` and `authorize_request_dispatch` commit a funding-source grant against current restrictions before submission. Previously authorized attempts remain reconcilable. Restricted queued work waits for re-verification. `maximum_unresolved` names the actual capacity control.
- **Explicit cancellation.** `PaymentProcessor::cancel_capture` takes a validated capture-cancellation request. Cancellation reaches an already-pending processor capture; lookup cannot silently consume the cancellation request.
- **Checked persistence.** Service accounts/contracts, payment executions, payables, funding sources, request finances, and financial-program restoration validate their invariants. Restored payable balances must match the ledger. Selected-owner program views reject complete-program operations such as enrollment and annual finalization.
- **Published quote pricing.** New durable quotes require configured pricing, resolve preferences against that policy, and record its version in the signed terms. The implicit fallback to caller-supplied prices was removed.

## Main implementation locations

| Area | Source |
| --- | --- |
| Annual service contracts and account actions | [billing models](/Users/thomm15/Documents/cs-mail/crates/cs-mail-billing/src/lib.rs), [commands](/Users/thomm15/Documents/cs-mail/crates/cs-mail-billing/src/commands.rs) |
| One distribution and its reporting values | [payable](/Users/thomm15/Documents/cs-mail/crates/cs-mail-finance/src/payable.rs), [application coordination](/Users/thomm15/Documents/cs-mail/crates/cs-mail-application/src/billing.rs) |
| Payment attempts and bank evidence | [execution](/Users/thomm15/Documents/cs-mail/crates/cs-mail-finance/src/execution.rs), [bank verification](/Users/thomm15/Documents/cs-mail/crates/cs-mail-finance/src/bank.rs), [funding restrictions](/Users/thomm15/Documents/cs-mail/crates/cs-mail-finance/src/funding.rs) |
| Durable authority and execution | [billing transactions](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/billing.rs), [finance transactions](/Users/thomm15/Documents/cs-mail/crates/cs-mail-storage-postgres/src/finance.rs), [workers](/Users/thomm15/Documents/cs-mail/crates/cs-mail-worker/src/lib.rs) |

## Validation

**127 tests passed, including 27 PostgreSQL/worker tests.** Workspace Clippy with warnings denied, formatting, and `git diff --check` passed. Validation used the available `stable` Rust toolchain; the test log records the executed suite.

New product acceptance examples cover:

- `$80`, `$120`, and `$170` allocations with `U = $120`, plus zero and arithmetic boundary cases.
- One full payment and the correct reporting breakdown despite a lost response.
- Distribution after closure while following-year service remains pending.
- Advance collection and coverage boundaries; late collection retries preserving price and period.
- Wrong beneficiary, substituted account authority, duplicate person enrollment, and unbacked pool enrollment.
- Restriction of already-queued work before its first dispatch, followed by authoritative re-verification.
- Cancellation of an already-pending capture.
- Corrupt payment state/receipts, malformed payables, and incomplete owner snapshots.
- Automatic annual allocation/payment through PostgreSQL workers, with recovery through a reconnected engine after a lost processor response.

Deprecated split-payment, payout-threshold, and refund-reservation tests were removed or replaced. Unrelated request, ledger, admission, privacy, receipt-ordering, and worker-fencing tests were retained as regression checks. Their previous expectations were not used as the authority for the new financial model.

Evidence: [workspace tests](/Users/thomm15/Documents/cs-mail/audit/domain-refactor-evidence-2026-09-15/workspace-tests.log), [Clippy](/Users/thomm15/Documents/cs-mail/audit/domain-refactor-evidence-2026-09-15/clippy.log), [formatting](/Users/thomm15/Documents/cs-mail/audit/domain-refactor-evidence-2026-09-15/fmt.log), [diff check](/Users/thomm15/Documents/cs-mail/audit/domain-refactor-evidence-2026-09-15/diff-check.log).

## Clean-update boundary and policy scope

The active formats are wire 6, stored protocol 8, and stored financial program 6. The development schema definitions now describe the replacement model. This requires a freshly initialized schema; no conversion or compatibility decoder for deprecated records was added. Verification used isolated test schemas, without rewriting existing application data.

Calendar anchors, collection dates, distribution dates, annual utility amounts, and unresolved-attempt capacity are explicit published/configured inputs. No new universal date or lead-time default was invented. The reporting utility amount is explicitly published with the annual distribution terms, independently of a user's renewal purchase.

Bank-account replacement, broader account administration/recovery UX, and account-data erasure policy remain within the account-model discussion the user deferred. Re-verification currently preserves the same person and bank route. Reversed payments retain their obligation and evidence for reconciliation; they are not silently reissued. The bank-verification boundary and processor simulator do not constitute a live bank integration.

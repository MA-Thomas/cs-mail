# Applications

> **Design principles.** Before changing code or this design, read the [Rust-domain design principles](../docs/rust-domain-principles.md). They are binding for cs-mail and identity-model; new work must not regress them.

The binaries in this directory are the product layer described in the
[product build plan](../cs_mail_build_plan.tex). Each is an **adapter**: it
validates input, invokes application use cases, and renders outcomes. None of
them makes domain decisions.

| Binary | Role | Arrives |
|---|---|---|
| `cs-maild` | Service process: HTTP API, workers, sender request page hosting, outbound guest and notice email | Milestone 1 |
| `cs-mail-admin` | Operator administration client, through the service's authenticated API | Milestone 1 |
| `cs-mail-cli` | Developer member client, over the client core | Milestone 2 |
| `cs-mail-mx` | Receive-only SMTP receiver (MX) for the deployment domain | Milestone 3c |
| `cs-mail-desktop` | macOS member application (Tauri) | Milestone 4 |
| `cs-mail-web` | Sender request page (static assets) | Milestone 3b |

## Boundary rules

- **Dependencies point inward.** Applications may depend on `crates/`. No crate in
  `crates/` may depend on an application. `scripts/quality-gate.sh` enforces this.
- **No domain decisions in adapters.** Permission, settlement, eligibility, and
  billing rules belong to their owning domain crates, and orchestration to
  `cs-mail-application`. An adapter never translates a signed command into a
  different meaning.
- **Time and authority.** Adapters obtain trusted time only as the
  [application and persistence boundaries](../docs/application-persistence-boundaries.md)
  prescribe: after acquiring the required protection when freshness matters.
- **No direct canonical writes.** The admin CLI uses the authenticated API. No
  application writes canonical tables outside the storage adapters' transactions.
- **Secrets stay behind ports.** Key material is reached only through key-custody
  ports and never enters logs, configuration, or diagnostics.

Shared metadata (version, edition, minimum Rust version, `publish = false`), lints,
and dependency versions come from the workspace `Cargo.toml`.

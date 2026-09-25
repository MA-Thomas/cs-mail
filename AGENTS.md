# Agent instructions — cs-mail

Read these before making any code or design change:

1. **[Rust-domain design principles](docs/rust-domain-principles.md)**. They are binding for
   cs-mail and identity-model, and new work must not regress them. In particular:
   - give distinct meanings distinct types, and give each invariant one owner;
   - keep domain decisions deterministic, with time, policy and evidence passed in;
   - make the complete consequence of a transition explicit and commit it atomically;
   - on a cutover, remove replaced code: no compatibility aliases, fallback readers or
     parallel implementations;
   - add a test only to challenge a named conjecture whose falsification would matter.
     Agree the testing scope for each piece of work, and report what was not covered.
2. **[C-SQD domain model](DOMAIN_MODEL.md)**: the confirmed product rules and deferred decisions.
3. **[Product build plan](cs_mail_build_plan.tex)**: the current milestones and the claims
   each one challenges.

Working conventions:

- identity-model is a separate repository, checked out as a sibling at `../identity-model`.
  Commit identity-model changes before the cs-mail changes that depend on them, and keep
  `identity-source.sha256` in step (`sha256sum --check identity-source.sha256`).
- Development and testing are local; there is no hosted CI. Local quality gate:
  `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings`,
  `cargo test --workspace`, and the PostgreSQL suites with
  `CS_MAIL_TEST_DATABASE_URL=... cargo test --workspace -- --ignored`.
- Applications (`apps/`) are adapters. Domain decisions belong in domain and application
  crates.

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

- There is **one unified member application**, with all implemented native workflows
  available for Marcus to assess. Do not introduce a separate demo mode, preview app,
  assessment app, fake client adapter or alternate owner account. Local bank/payment
  simulation and isolated test fixtures belong behind the existing backend contracts;
  they are not another application mode.
  The canonical build entry is `scripts/apple-build.sh`; the current app is
  `target/apple/CsMail-continuity-20261012.app` (**cs-mail Development**).
  Read the current sections of [the Apple runbook](apps/cs-mail-apple/README.md)
  before operating the deployment; older dated sections are historical evidence.
- Marcus's requested development protocol address is `mtallis62831@c-sqdmail.com`,
  with aliases `marcus.thomas@c-sqdmail.com`,
  `mat7771@c-sqdmail.com` (NYC-Hospital) and `m11a33t@c-sqdmail.com`
  (Brooklyn-Lab-Co). This explicitly replaces the earlier `marcus@csqd-mail.com`
  address by the owner's October 16 instruction; the original account is retained.
  Retain these addresses across development-environment setup and cutovers. Preserving
  the address and preserving the existing account/Keychain identity are distinct
  requirements; do not treat a newly enrolled account as continuity of the old one.
  Maintain Marcus's original Keychain entry and its account association. Do not
  replace, delete, overwrite or substitute that credential during development or
  a cutover; use an explicit preservation-checked continuity upgrade.
- A successful build does not establish that Marcus is running the updated app.
  Finish installation at the canonical path, preserve the original signing requirement
  and account association, reopen the app when needed, and verify the requested
  behavior in that running application before reporting it available. If a step is
  blocked, state exactly what is built, installed and running, and why.
- Preserve the agreed interaction meanings: navigation/list items have generous
  clickable regions; main-space portals highlight their **perimeter** on hover,
  without shading the whole portal. Navigation previews must correspond to the
  actual hovered icon and dismiss after the pointer leaves both icon and popup.
  The composer remains a popup/pop-out from the main space. These are presentation
  behaviors; they do not establish communication permission or disclosure consent.
- Report milestone progress from current implementation and validation evidence,
  distinguishing local simulation from public/live-provider delivery. Do not carry
  historical deployment blockers forward without checking current state. A slow
  native-app inspection tool is not evidence of slow application or backend behavior.
- identity-model is a separate repository, checked out as a sibling at `../identity-model`.
  Commit identity-model changes before the cs-mail changes that depend on them, and keep
  `identity-source.sha256` in step (`sha256sum --check identity-source.sha256`).
- Development and testing are local; there is no hosted CI. Run only tests and checks
  relevant to the alterations made, including affected integration boundaries. For
  presentation-only changes, build the Apple app and inspect the changed screen;
  do not run unrelated Rust or PostgreSQL suites. Use the full quality gate when the
  scope of the changes warrants all its checks:
  `CS_MAIL_TEST_DATABASE_URL=... scripts/quality-gate.sh`. It reports each check
  separately. `--skip-postgres` skips the PostgreSQL suites explicitly; skipped suites
  are not evidence. Report the checks performed and material coverage limitations.
- Applications (`apps/`) are adapters. Domain decisions belong in domain and application
  crates, and no library may depend on an application (the gate checks this). See
  [apps/README.md](apps/README.md).
- Schema migrations: until the project owner declares migrations forward-only, a
  schema change may replace or renumber existing migrations and requires a fresh
  database. From that declaration onward, migrations are forward-only: never edit,
  remove or renumber a merged migration.

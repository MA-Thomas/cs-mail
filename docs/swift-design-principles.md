# Swift design principles

Agreed on 14 October 2026. These principles guide the unified Apple member
application alongside the [Rust-domain design principles](rust-domain-principles.md).
Swift owns presentation and interaction; domain rules and authority retain their
existing Rust owners.

## 1. Keep implementation concerns out of the interface

Show information and controls when they help the member understand their
situation, make a meaningful decision or complete an intended action. Handle
routine loading, refreshing and coordination behind the scenes. Do not make the
member manage the application's internal operation.

A technical fact being available is not a reason to display it. Every visible
element should serve a clear member need. Explain problems in plain language
when they affect the member, with an action only when the member can usefully
resolve them. Preserve consequential uncertainty without exposing debugging
details or implying that stale information establishes current permission.

For example, the express-lane list should not have a separate “Reload alias
lanes” button or a permanent “Status as of” timestamp merely because alias data
is fetched separately. Keep the list current automatically and use the existing
Refresh action when a manual refresh is useful. If current information cannot
be retrieved, show a clear notice where that limitation matters.

Before adding a label, status, timestamp, button or confirmation, identify what
the member will understand or accomplish because it is there. If its purpose is
only to describe implementation machinery, keep it behind the scenes.

## 2. Preserve the member’s place, work, and intent

Agreed on 15 October 2026.

Moving through the app should form a coherent journey. Every opened destination
needs an obvious way back to its origin, restoring relevant search, filters,
selection and position. An explicit action such as Inbox can start a fresh
context.

Navigation must not silently discard unfinished work. Preserve edits where
appropriate, and offer Save, Discard or Stay when leaving would lose them. Ask
only when something consequential is at stake.

Background work must respect the member’s latest action. Loading must not swallow
a selection, an earlier response must not pull them back to an abandoned
destination, and an invisible reader must not count as reading.

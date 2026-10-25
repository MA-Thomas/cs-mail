# Swift design principles

Agreed on 14 October 2026, with principles 3–5 and the continuity clarification
in principle 2 added on 23 October 2026. These principles guide the unified Apple member
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

### Loading feedback: explicit owner request required for spinners

Do not show spinners anywhere in the app unless Marcus explicitly requests one
for a particular circumstance. This applies to every interface, screen, sheet,
background operation and pagination surface. Framework defaults or generic
loading conventions do not authorize an exception. Preserve useful content and
interaction context during work; use quiet text only when the member needs it.
Keep cancellation, refusal and uncertain-outcome actions available. Async work
must not depend on a visible loading indicator to start or continue.

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

Keep a meaningful, stable surface visible while a destination prepares. Avoid
unnecessary intermediate screens and premature changes to framing or emphasis.
Show retained content only through the domain’s valid reading rules, and never
present the previous destination’s content as the newly selected destination.

## 3. Give interaction states distinct meanings

Hover, focus, selection, pressing and action prominence represent different
things. Define their consequences explicitly; one state must not silently trigger
another state’s behavior.

A click should perform its assigned action without depending on tracked hover
state. Keyboard and accessibility access must preserve the same intended
capabilities, with appropriate feedback.

## 4. Own the complete visual composition

Give each surface an explicit owner for its geometry and material treatment.
Fills, clipping, outlines, effects and hit regions must agree, including their
interaction with native rendering.

Visual emphasis should follow the member’s current task. Supporting navigation
should remain subordinate to the active content.

Validate the composed result in the running application. Numerical settings and
successful compilation do not establish the appearance: native effects,
surrounding surfaces, clipping and available space can change the result.

## 5. Express visual consistency through shared contracts

Define the scope of each visual convention explicitly. Controls and surfaces that
serve equivalent roles within that scope should derive their shared appearance
and behavior from a common implementation contract, expressed through types,
styles or components.

A change to that contract should reach every member of the scope. Differences
should represent deliberate roles or explicit exceptions, rather than duplicated
values that drift apart.

Share only what the scope requires. Distinct presentation interfaces may evolve
independently; within each interface, related controls should remain coordinated.

## Applied search interaction contract

All search fields in the macOS member app, across interfaces A and B, use the
shared `SearchFieldInteraction` presentation modifier. Clicking outside a focused
search field ends its editing focus and removes its caret/focus treatment without
clearing the query or its results. The outside click continues to its original
control or content; dismissal must not require a second click. Clicking inside
continues editing. Keyboard and accessibility focus remain available; existing
search/submit shortcuts keep their assigned behavior. Visual styling remains
owned by each interface. This contract covers mailbox and conversation search,
alias inbox filters, Directory lookup, express-lane search and financial records.
The current pointer-dismissal adapter is macOS-specific; iOS touch behavior needs
its own implementation and runtime validation.

Live mailbox search retains its last completed results while a newer query runs,
with the displayed query identifying their provenance. The first current response
replaces those results; stale responses cannot replace a newer generation. Routine
per-keystroke loading indicators must not insert layout space or blank the result
surface. Clearing search or changing receiving scope discards the old projection.

## Interface B main-space texture contract

By default, whenever the main content column shows terrazzo, it uses one-quarter of the
workspace’s default texture opacity in dark mode and 55% in light mode
(revised 24 October 2026). The main-space composition supplies this
role through the view environment so nested screen backgrounds inherit the same
rule. Explicit member opacity choices override these defaults through the shared appearance contract; do not assign texture strength in an individual screen.
The surrounding workspace retains its default strength; the sidebar retains its
explicit subdued role. Keep opaque reading surfaces opaque, and preserve the
existing contrast/transparency accessibility behavior.

The email body retains its solid white background in light mode. Terrazzo remains
visible in the surrounding main-space areas; do not make the writing area
transparent to expose texture.

The background outside the rounded main-space surface, including its padding
behind the cove lighting, uses the normal workspace texture strength. Only the
main-space interior inherits the reduced strength. This prevents a rectangular
texture transition outside the rounded surface; preserve the cove-light overlay.

Column B and the general main-space surface share the same top and bottom inset
through InterfaceBWorkspaceLayout. Keep their visible surface edges aligned;
horizontal spacing is independent of their shared vertical inset. The vertical
divider between Columns A and B starts at that same top inset, aligned with
Column B’s top edge.

## Shared Column B sizing

Every instantiated Column B in A and B uses WorkspaceColumnLayout and the shared
workspaceColumn modifier. Its minimum is measured from the single-line
Communications heading in the actual heading font, including scaled text,
content insets and trailing breathing room. Do not select a narrower minimum
for a different area. On macOS the native split item enforces that same minimum
and disables user collapse, so dragging cannot make the organizer disappear.
Home and Notes retain their intentional layouts without Column B.

## Column B navigation selection

A/B Column B navigation uses the shared ColumnBNavigationSelection contract:
a short teal leading marker and stronger label weight, with no persistent filled
selection rectangle. The columnBNavigation environment scopes this treatment to
the organizer; content choices and diagram controls retain their distinct roles.
Hover/press feedback remains interface-owned and separate from persistent
selection. Preserve full-width hit areas and selected accessibility traits.

Interface B’s Column B panel is a regular rectangle with straight sides and
24-point corner rounding matching the main space. Its texture, quiet outline, shared width constraints and navigation
selection contract remain independent of that geometry. Do not restore the bowed
silhouette. Main-space rounded boundaries and cove lighting retain their own role.

## Main-space writing contract

Agreed 23 October 2026. New native conversations, native replies and external
email drafts open in the main space in both interfaces. Reopening those drafts
uses the same workspace. This supersedes the popup-composer convention. Column B supports correspondence reading beside new, unaddressed and external drafts, and native source browsing with explicit quotation, reference and shared-copy actions. Opening or selecting a source does not insert or disclose it. External email does not offer native source export.
Incoming supported HTML preserves supplied presentation through the shared inert reader;
external resources require explicit message-scoped retrieval permission.

The workspace owns the active draft and source selection together. Navigation
waits for the current writing to save; a save failure keeps the draft visible
with Save, Discard or Stay choices. Closing returns to the recorded origin.
Saving does not send, grant reuse permission or establish disclosure consent.
Existing Column B sizing and interface-owned surface contracts still apply.

External email composition has no separate relay/plaintext consent checkbox or
consent explanation, as requested on 23 October 2026. Marcus explicitly confirmed
that Send sends ordinary email directly without that checkbox. That action
carries the existing conversation-scoped disclosure authorization for a new
conversation. Opening, editing or saving writing grants no authorization and sends
nothing; replies retain the existing conversation-authority checks.

The Write an email toolbar action opens a blank draft immediately. The member
can write and save before choosing a recipient through an editable To field.
Suggestions appear inline while the message remains visible; entering or changing
a recipient must not replace the editor with another screen. Save/close and discard
share the quiet text-only ComposerActionStyle; Send keeps the existing primary
style for each interface. A recipient
is required only for sending. Known contacts and directory/address resolution feed the
existing native or external editor. Rust resolves routes and retains the selected
native alias; Swift never chooses a transport from a domain-name comparison.
Changing an unsent new recipient preserves the draft identity and message text.
Changing to an external destination requires an explicit choice before removing
a native subject or source inclusions. Replies keep their conversation recipient.
Column B offers native source actions when applicable and otherwise provides correspondence reading beside the draft. It does not add explanatory panels solely to describe unavailable actions.

## Main-space alias creation contract

Agreed 24 October 2026. Selecting Aliases and Express Lanes in Column A opens
New alias in the main space in both interfaces. Column B’s New alias action opens
the same destination; creation does not use a popup. Existing aliases and the
explicit guide choice open their respective main-space destinations. Unfinished
alias inputs survive navigation within the unlocked session. Opening, editing or
leaving the form does not create an alias or grant receiving permission; the
existing explicit Create alias operation retains its Rust authority checks.

## Interface B live terrazzo color

Agreed 24 October 2026. The palette toolbar popover edits one device-local
terrazzo appearance shared by all B backdrops. Color and intensity changes apply
immediately without Apply. The color field selects hue/brightness; Intensity owns
saturation. Native sliders provide keyboard/accessibility alternatives. Use default
colors restores the original cached raster exactly and retains the last custom
choice. Cached neutral flecks are colored independently of the binder; changing
controls does not regenerate mineral geometry. Preserve the existing texture
strength roles, opaque writing surfaces and accessibility suppression. These
preferences carry no account, receiving or disclosure authority.

### Independent texture opacity

Agreed 24 October 2026. The same Interface B palette popover contains Main-space
texture and Surrounding texture opacity controls. The first sets absolute texture
opacity inside the main space; the second sets outer workspace opacity and Column
B at one-quarter of that value. Color saturation is labeled separately. Editing
opacity preserves the selected default/custom color. Use defaults restores the
original color rendering and removes both opacity overrides, retaining the
scheme-specific original strengths. Overrides persist locally and update all
surfaces in their scope immediately. Opaque writing surfaces remain opaque.

### Named appearance profiles

Agreed 24 October 2026. Interface B’s palette can save named snapshots containing
resolved light/dark appearance, color/default selection and optional main/surrounding
opacity overrides. Selecting a profile restores its snapshot; later live edits
show Modified without changing the saved snapshot. Save as new profile, Update
profile, Rename and Delete are explicit actions. Deletion leaves the current
appearance intact. A single device-local encoded document owns profile snapshots
and selection. First adoption preserves the current setup once; deleting all
profiles does not recreate them on reopening. Unreadable profile data is retained
and reported without overwriting it. Profiles carry no account authority.

### Profile controls reveal the current task

Agreed 24 October 2026. Profiles is a list with a marked active choice. Save as
new profile replaces the list with a name field and Save/Cancel; rename uses the
same focused section. Save returns only after successful storage, and Cancel
leaves saved profiles and live appearance untouched. Each row’s menu owns Rename
and Delete for that exact profile, without applying it. Modified and Update profile
appear only for an edited selected snapshot. Color/opacity controls remain below.

## Quiet Column A and Home contacts in Interface B

Agreed 24 October 2026. Column A icons and Home contact rows use the shared quiet
workspace-button contract rather than raised glass, layered bezels or filled
contact capsules. Navigation retains 48-point hit areas and marks the active area
with one thin perimeter and soft exterior cove lighting (revised 24 October 2026). Home contacts use full-width padded rows with
name, status, restrained trailing arrow and subtle separators. Hover highlights
only the perimeter; pressing changes perimeter/content feedback without selecting
on hover. Focus has a separate teal ring. Space/Return activate the same action
as clicking; Right Arrow opens the correct navigation preview and Escape dismisses
it. Existing preview pointer tracking, member facts, main-space perimeter lighting
and saved texture appearance retain their owners.

Column A selection uses the shared area-owned cove-light renderer around the
icon hit region, without a side marker, fill, raised surface or stacked bezel.
Hover on an unselected icon remains a quiet perimeter; keyboard focus retains a
separate crisp outer ring. Increased contrast or reduced transparency replaces
the selected glow with a legible solid perimeter. Home contact rows retain their
existing quiet interaction contract and do not acquire persistent selection.

Column A pointer activation clears icon keyboard focus rather than assigning it.
Only keyboard traversal, Space/Return activation and keyboard-preview actions
retain the separate focus ring. Clicking a selected icon must not replace its
steady cove-selection appearance with a keyboard ring.

Column A no longer renders the solid teal keyboard-focus perimeter, including
transient focus during mouse-down (24 October 2026 correction). Unselected icons
with keyboard focus use a small neutral underline; selection retains cove lighting.
Home contact focus rings remain scoped to their separate role. Native focus effects
remain disabled on the navigation buttons.


## Compact Interface B mailbox contract

Agreed 24 October 2026. Mailbox rows share one presentation owner for density,
correspondent/subject/date alignment, constrained-width stacking, unread emphasis,
flags, and quiet perimeter feedback. Generous full-row hit areas remain. Selection
checkboxes, opening correspondence and row menus have distinct controls; hovering
never selects or fills the row. Bulk controls appear with selection. Receiving
route information is secondary in All inboxes and omitted when the inbox already
provides that context. Search excerpts use the same row composition and existing
retained-search evidence. Drafts share the composition without changing storage
or send semantics. Header options hold occasional actions; routine refresh and
transport machinery do not compete with correspondence. Offline or consequential
uncertainty remains explicit. Returning from reading preserves mailbox scope,
search, sort, selection and anchors through the existing navigation owner.


## Conversation reading hierarchy

Agreed 24 October 2026. Native and external conversation find is hidden until explicitly invoked with Command-F. When invoked, it occupies
one fixed header position within the main reading surface, using
ConversationReaderHeader. Escape dismisses it and clears the local query.
Titles, sender metadata, warnings and permission controls must not push it down.
The reading surface uses the same portal geometry for both transports. Message
content scrolls below the header. Opening a mailbox row opens that exact
conversation; no contact-level subject dropdown replaces this selection.

An email's subject is its heading, and the sender address and date are quiet
metadata. Latest is not part of the reading interface. Manage contact is inside the existing
conversation-actions menu, rather than a prominent separate button. Contact
permission and blocking actions belong under Manage contact,
with revocation offered only for a current standing/lane permission. Reply is the
primary message action. Sending ordinary email supplies the explicit outgoing
conversation authorization; opening, searching, drafting or saving does not.
External-picture approval remains separate and message/resource-specific.


## Member navigation areas

Agreed 24 October 2026. The member application has Home, Communications,
Relationships, Aliases and Express Lanes, and Notes. Finances is removed from
Column A, its Column B organizer and main-space destinations in both interfaces.
Do not retain a hidden financial-record route or its presentation-only settings.
Request and settlement domain rules remain with their existing Rust owners.

## Everyday composing and imported designs

Agreed 25 October 2026. Both interfaces use one shared body editor for new native conversations, native replies, external email and unaddressed writing. Request and request-follow-up forms retain their existing text workflow. Recipient and subject rows remain above a compact, stable formatting toolbar; the opaque body owns its vertical scrolling, with attached files below it. Send retains each interface’s primary action style, and secondary actions retain ComposerActionStyle. Column B remains a correspondence/source workspace.

The toolbar provides emphasis, font, size, color, paragraph/list controls, links, inline raster pictures and attached files. Picture size and alternative text belong to the selected picture. Selection, keyboard focus, active formatting and hover retain distinct meanings. Formatting must use the current selected passage even if a selection notification is queued; opening tools preserves that passage. Native undo/redo, spelling and paste remain available, including paste without formatting.

Import HTML opens a file chooser and an inert preview in the same main space. Local image dependencies require a deliberately selected containing folder and cannot escape it through paths or symlinks. Preview does not retrieve remote pictures, styles or fonts. Replacing existing writing or converting an imported design to editable plain text requires an explicit choice explaining the content that will be removed. Import is not a visual design studio.

The draft workspace owns the complete editor and helper lifetime. New writing cannot reuse another draft’s web surface, selection, files or helper selection. Save and navigation capture the current editor content and drain coalesced durable saves; failed capture or save keeps writing visible. File contents are copied into encrypted drafts, never represented by durable references to their original filesystem paths. Rust owns accepted content, size limits, source permissions, message preparation and exact retry. Opening, importing, formatting and saving grant no sending or external-retrieval authority.

## Composer click targets and recipient suggestions

Corrected 25 October 2026. Composer icon labels share ComposerControlTarget:
44-point square targets including transparent label space. Secondary composer
actions use the same minimum height through ComposerActionStyle; recipient
choices and source actions expose full-row regions. Keep compact glyphs and quiet
visual treatment independent of the clickable area. Both A/B derive this geometry
from the shared contract rather than individual icon dimensions.

A recipient field and its suggestion list share one click-away boundary. Clicking
a suggestion must deliver its action before the list disappears; clicking elsewhere
still dismisses field focus without consuming the outside action. Selecting a native
recipient populates To and switches Column B to its existing source picker. Choosing
a source message exposes Quote selection and Reference selection, enabled by a valid
text/image selection. Selection alone does not insert content or send anything.

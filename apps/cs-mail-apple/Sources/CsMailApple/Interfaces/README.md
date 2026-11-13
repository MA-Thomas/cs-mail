# Independent member interfaces

A, B, C and D are complete presentation trees within the one `CsMailApple` module.
A retains the original presentation; B uses decorated
native glass, with stationary rims and restrained hover/press feedback. Edit only the chosen directory
when developing an alternative visualization. All use the one `MemberModel` owned
by the shared `MemberView` host. Do not construct a client, copy credentials or
introduce backend rules here.

Visual primitives have matching independent trees under
`Sources/CsMailPresentation/Interfaces/A`, `B`, `C` and `D`. Each screen tree
references its own visual types, so changes to one interface do not implicitly
change the others. Shared data types, model projections and platform services remain
outside these directories. Layout preferences remain shared. B and C use shared WorkspaceColumn sizing and
WorkspaceSplitSurround decoration. D reuses the size measurements in its own clear
split layout and native invisible resize handle. Each interface owns its rendering.
C uses a continuous satin white-gold surround, smooth neutral content and independent
named light/dark appearance profiles. B's appearance remains independent.

D began as an exact source copy of B on 1 November 2026 and now owns selectable
tonal color families with independent glass opacity. Its screen/component tree is independent of B.
Its opaque Appearance popover independently controls background opacity for the
main space, Column B, and Column A plus exterior surround. Each region owns its
background alpha; the root surround excludes the other regions, and Column B is
clipped to its rounded outline. The three
values are saved in `csMailInterfaceDGlassOpacities`, separately from historical
profiles. Zero is transparent backing; 100% is opaque burgundy. Writing canvases,
navigation previews and accessibility fallbacks stay opaque. The shared
MemberView hosts D's window lease, restoring the original window properties
synchronously before switching to A/B/C. It never enumerates application windows.
The shared body editor has an explicit display style with unchanged standard
defaults; only D selects burgundy. Sender-authored HTML keeps its own presentation.
D's superseded terrazzo renderer, controls, shader, assets and generator are removed.
The original copied `csMailInterfaceDAppearanceDocument` remains preserved as saved
appearance work but has no runtime reader. B/C preference documents are untouched.
Layout preferences, editor behavior and the real member session remain shared.

To add another interface, copy an interface's screens and visual primitives, give its concrete
visual types distinct names, then add a `MemberInterface` case and screen constructor
in `MemberView.swift`. The picker enumerates that registry automatically. Keep the
shared model injection and switch-availability preference in the new root view.

Switching is available at Home with no editor/dialog or busy operation. Returning
Home uses the existing unsaved-edit guards. Switching recreates transient navigation
at Home, retains saved work and the client session, and remembers the selected
interface for reopening. Only the active interface is mounted.

D's Appearance panel offers immutable Burgundy, Turquoise, Gold-olive, Forest green,
Gray-black and Follow the selected area profiles. Definitions and resolved tonal roles live
in presentation InterfaceDAppearance.swift. D emits its actual selected area to
the host; the resolved palette is injected throughout D and passed explicitly to
its window backing. Color-profile selection uses csMailInterfaceDColorProfile,
independently of csMailInterfaceDGlassOpacities. Hovered icons do not change the
following profile. Shared composer styling accepts explicit tonal values only
for D; other interfaces retain their standard canvas.

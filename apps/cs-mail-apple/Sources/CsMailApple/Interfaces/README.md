# Independent member interfaces

A and B are complete presentation trees within the one `CsMailApple` module.
They began identically. A retains the original presentation; B now uses decorated
native glass, with stationary rims and restrained hover/press feedback. Edit only the chosen directory
when developing an alternative visualization. Both use the one `MemberModel` owned
by the shared `MemberView` host. Do not construct a client, copy credentials or
introduce backend rules here.

Visual primitives have matching independent trees under
`Sources/CsMailPresentation/Interfaces/A` and `B`. B screen references use B visual
types, so changing A's colors, layout or controls does not implicitly change B.
Shared data types, model projections and platform services remain outside these
directories. Existing appearance/layout preferences are intentionally shared.

To add C, copy an interface's screens and visual primitives, give its concrete
visual types distinct names, then add a `MemberInterface` case and screen constructor
in `MemberView.swift`. The picker enumerates that registry automatically. Keep the
shared model injection and switch-availability preference in the new root view.

Switching is available at Home with no editor/dialog or busy operation. Returning
Home uses the existing unsaved-edit guards. Switching recreates transient navigation
at Home, retains saved work and the client session, and remembers the selected
interface for reopening. Only the active interface is mounted.

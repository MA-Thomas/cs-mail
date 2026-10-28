# Independent member interfaces

A, B and C are complete presentation trees within the one `CsMailApple` module.
A retains the original presentation; B uses decorated
native glass, with stationary rims and restrained hover/press feedback. Edit only the chosen directory
when developing an alternative visualization. All use the one `MemberModel` owned
by the shared `MemberView` host. Do not construct a client, copy credentials or
introduce backend rules here.

Visual primitives have matching independent trees under
`Sources/CsMailPresentation/Interfaces/A`, `B` and `C`. Each screen tree
references its own visual types, so changes to one interface do not implicitly
change the others. Shared data types, model projections and platform services remain
outside these directories. Layout preferences remain shared. B and C use the same
WorkspaceColumn sizing and WorkspaceSplitSurround decoration; macOS owns divider
cursor and drag interaction. Each interface supplies its own material rendering.
C uses a continuous satin white-gold surround, smooth neutral content and independent
named light/dark appearance profiles. B's appearance remains independent.

To add another interface, copy an interface's screens and visual primitives, give its concrete
visual types distinct names, then add a `MemberInterface` case and screen constructor
in `MemberView.swift`. The picker enumerates that registry automatically. Keep the
shared model injection and switch-availability preference in the new root view.

Switching is available at Home with no editor/dialog or busy operation. Returning
Home uses the existing unsaved-edit guards. Switching recreates transient navigation
at Home, retains saved work and the client session, and remembers the selected
interface for reopening. Only the active interface is mounted.

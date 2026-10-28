import SwiftUI
import CsMailRust
import CsMailPresentation

private struct InterfaceCColumnBBounds: PreferenceKey {
    static var defaultValue: Anchor<CGRect>? { nil }
    static func reduce(value: inout Anchor<CGRect>?, nextValue: () -> Anchor<CGRect>?) {
        value = nextValue() ?? value
    }
}

struct InterfaceCView: View {
    private enum Appearance: String {
        case system, light, dark
        var colorScheme: ColorScheme? {
            switch self {
            case .system: return nil
            case .light: return .light
            case .dark: return .dark
            }
        }
    }
    @AppStorage("csMailHomeArrangement") private var homeArrangement: HomeArrangement = .cards
    @AppStorage("csMailMailboxSectionOrder") private var mailboxSectionOrder: MailboxSectionOrder = .receivingFirst
    @AppStorage("csMailLaneSectionOrder") private var laneSectionOrder: LaneSectionOrder = .aliasesFirst
    @AppStorage("csMailNotesReadingWidth") private var notesReadingWidth: NotesReadingWidth = .wide
    @AppStorage("csMailRelationshipsOverview") private var relationshipOverview: RelationshipOverviewStyle = .map
    @AppStorage("csMailInterfaceCAppearance") private var appearance: Appearance = .system
    @StateObject private var appearanceProfiles = InterfaceCAppearanceProfiles()
    private var appearanceSnapshot: Binding<InterfaceCAppearanceSnapshot> {
        Binding(get: { InterfaceCAppearanceSnapshot(mode: (appearance.colorScheme ?? scheme) == .dark ? .dark : .light) },
                set: { snapshot in
                    appearance = snapshot.mode == .dark ? .dark : .light
                })
    }
    @State private var showingMaterials = false
    @ObservedObject private var model: MemberModel
    private let accountAccess: DevelopmentAccountAccess
    @State private var policyConfirmation: PolicyConfirmation?
    private struct PolicyConfirmation { let conversation: String; let revision: UInt64; let action: PolicyAction; let title: String; let explanation: String }
    @State private var readingReference = false
    private struct RequestTarget: Identifiable {
        let other: SenderReference
        let name: String
        let requestClass: RequestClassID
        let alias: String
        var id: String { other.value + ":" + alias + ":" + requestClass.value }
    }
    @State private var requestTarget: RequestTarget?
    @State private var reviewingRequest = false
    @State private var managingContact = false
    @State private var enrolling = false
    @State private var composition: WorkspaceComposition?
    @State private var compositionNavigation: (() -> Void)?
    @State private var showingDrafts = false
    @StateObject private var mailboxNavigation = MailboxNavigation()
    @State private var aliasQuery = ""
    @State private var laneQuery = ""
    @State private var laneReceivingAlias: String?
    @State private var laneDirection: EditorialDirection = .incoming
    private struct NavigationCheckpoint {
        let destination: MemberDestination
        let sidebar: SidebarTarget?
        let sender: SenderReference?
        let thread: String?
        let message: String?
        let passage: SearchMatch?
        let title: String
        let inbox: String?
        let mailbox: MailboxNavigation.Context
        let laneQuery: String
        let laneDirection: EditorialDirection
        let laneReceivingAlias: String?
        let address: ManagedAddressDestination
    }
    @State private var returnStack: [NavigationCheckpoint] = []
    @State private var pendingNavigation: (() -> Void)?
    @State private var navigationRevision = 0
    @State private var destination: MemberDestination = .home
    @State private var sidebarTarget: SidebarTarget?
    @State private var columnVisibility: NavigationSplitViewVisibility = .detailOnly
    @State private var server = "http://127.0.0.1:8480"
    @State private var name = ""
    @State private var birthDate = ""
    @State private var managedAddressDestination: ManagedAddressDestination = .overview
    @State private var address = ""
    @State private var phone = ""
    @State private var bankAccount = ""
    private struct DecisionConfirmation {
        let sender: SenderReference
        let request: RequestID
        let decision: Decision
    }
    @State private var confirmation: DecisionConfirmation?
    @Environment(\.scenePhase) private var phase
    @Environment(\.colorScheme) private var scheme
    init(model: MemberModel, accountAccess: DevelopmentAccountAccess, serviceURL: String) {
        _server = State(initialValue: serviceURL)
        self.accountAccess = accountAccess
        self.model = model
    }
    public var body: some View {
        Group {
            if let inbox = model.inbox {
                memberShell(inbox)
            } else {
                VStack(spacing: 22) {
                    Text("cs-mail").font(InterfaceCEditorialStyle.heading(23)).padding(.bottom, 24)
                    Text("Your conversations.\nYour permission.").font(InterfaceCEditorialStyle.heading(34)).multilineTextAlignment(.center)
                    Button("Open account") { Task { await model.unlock() } }.buttonStyle(InterfaceCEditorialButtonStyle(prominent: true)).disabled(accountAccess != .available)
                    Button("Sign in and enroll…") { enrolling = true }.buttonStyle(InterfaceCEditorialButtonStyle()).disabled(accountAccess != .available)
                    Button("Check enrollment status") { Task { await model.resumeEnrollment() } }.disabled(accountAccess != .available)
                    if accountAccess == .signingSetupRequired { Text("Account setup needs stable app signing.").font(.caption).multilineTextAlignment(.center) }
                    Text("Credentials stay in Keychain. The local cache is encrypted.\nDevelopment uses a test login provider and simulated bank.")
                        .font(.caption).multilineTextAlignment(.center).foregroundStyle(.secondary)
                    status
                }.padding(32).frame(maxWidth: .infinity, maxHeight: .infinity).disabled(model.busy)
            }
        }
        .task { appearanceProfiles.preserveInitial(appearanceSnapshot.wrappedValue) }
        .preferredColorScheme(appearance.colorScheme)
        .toolbar {
            ToolbarItem {
                Button(scheme == .dark ? "Use light appearance" : "Use dark appearance",
                       systemImage: scheme == .dark ? "sun.max" : "moon") {
                    appearance = scheme == .dark ? .light : .dark
                }
                .help("Switch between light and dark appearance")
            }
            ToolbarItem {
                Button("Appearance profiles", systemImage: "paintpalette") { showingMaterials.toggle() }
                    .help("Choose or save an appearance profile")
                    .popover(isPresented: $showingMaterials, arrowEdge: .top) {
                        ScrollView {
                            VStack(spacing: 0) {
                                InterfaceCAppearanceProfileControls(profiles: appearanceProfiles, current: appearanceSnapshot)
                            }
                        }.frame(width: 340, height: 360)
                    }
            }
        }
        .textSelection(.enabled)
        .foregroundStyle(InterfaceCEditorialStyle.ink(scheme), InterfaceCEditorialStyle.secondaryInk(scheme))
        .tint(InterfaceCEditorialStyle.ink(scheme))
        .background { InterfaceCWorkspaceBackdrop(surface: .foundation).ignoresSafeArea() }
        #if os(macOS)
        .toolbarBackground(.hidden, for: .windowToolbar)
        #endif
        .overlay(alignment: .top) {
            if model.authenticating {
                Button("Cancel sign-in") { model.lock() }
                    .padding(12).background(.regularMaterial, in: Capsule())
            }
        }
        .onChange(of: model.inbox == nil) { _, locked in
            if locked { mailboxNavigation.reset(); returnStack = []; pendingNavigation = nil; showingDrafts = false; readingReference = false; requestTarget = nil; composition = nil; compositionNavigation = nil; model.clearReference(); destination = .home }
        }
        .confirmationDialog(policyConfirmation?.title ?? "Change reuse policy?", isPresented: Binding(get: { policyConfirmation != nil }, set: { if !$0 { policyConfirmation = nil } }), titleVisibility: .visible) {
            if let target = policyConfirmation { Button("Confirm") { policyConfirmation = nil; Task { await model.changePolicy(conversationID: target.conversation, revision: target.revision, action: target.action) } } }
            Button("Cancel", role: .cancel) { policyConfirmation = nil }
        } message: { Text(policyConfirmation?.explanation ?? "") }
        .confirmationDialog("Save changes to your approach page?", isPresented: Binding(get: { pendingNavigation != nil }, set: { if !$0 { pendingNavigation = nil } }), titleVisibility: .visible) {
            Button("Save changes and continue") {
                let action = pendingNavigation; pendingNavigation = nil
                Task { if await model.approachDraft.save(using: model) { action?() } }
            }.disabled(model.busy || !model.approachDraft.valid || model.approachDraft.loadedRevision != model.addressSettings?.revision)
            Button("Discard changes and continue", role: .destructive) {
                let action = pendingNavigation; pendingNavigation = nil
                if let settings = model.addressSettings { model.approachDraft.load(settings) }
                action?()
            }
            Button("Stay", role: .cancel) { pendingNavigation = nil }
        } message: { Text("Your unpublished edits are kept while you stay on this page. Save publishes them; discarding leaves the saved page unchanged.") }
        .sheet(isPresented: $showingDrafts) { InterfaceCDraftsView(model: model) }
        .environment(\.workspaceCompose, WorkspaceComposeAction(open: openComposition))
        .confirmationDialog("Keep this draft before leaving?", isPresented: Binding(get: { compositionNavigation != nil }, set: { if !$0 { compositionNavigation = nil } }), titleVisibility: .visible) {
            Button("Save and continue") {
                let action = compositionNavigation; compositionNavigation = nil
                if let action { navigateSafely(action) }
            }
            Button("Discard and continue", role: .destructive) {
                let action = compositionNavigation; compositionNavigation = nil
                Task {
                    if let editor = composition?.draft, await editor.discard() {
                        composition = nil; model.clearSourceContact(); action?()
                    }
                }
            }
            Button("Stay", role: .cancel) { compositionNavigation = nil }
        } message: { Text("Your draft could not be saved. Stay to resolve the saving problem, or explicitly discard it.") }
        .sheet(isPresented: $readingReference, onDismiss: { model.clearReference() }) {
            VStack(alignment: .leading, spacing: 18) {
                HStack { Text("Referenced selection").font(.title2); Spacer(); Button("Done") { readingReference = false; model.clearReference() } }
                switch model.reference {
                case .readable(let text): ScrollView { InterfaceCEditorialReadingView(text).privacySensitive() }
                case .unavailable: Text("The source is deleted, expired or otherwise unavailable.")
                case .accessDenied: Text("You do not have access to the retained source. The reference grants no access.")
                case nil: if let error = model.memberError { Text(error).foregroundStyle(.orange) } else { Color.clear }
                }
            }.padding(24).frame(idealWidth: 560, idealHeight: 430)
        }
        .sheet(item: $requestTarget) { target in
            VStack {
                InterfaceCRequestComposerView(model: model, other: target.other, name: target.name, requestClass: target.requestClass, receivingAlias: target.alias, close: { requestTarget = nil })
            }
        }
        .sheet(isPresented: $reviewingRequest) {
            VStack {
                if let progress = model.inbox?.requestProgress {
                    InterfaceCRequestComposerView(model: model, other: SenderReference(progress.other), name: model.inbox?.conversations.first(where: { $0.sender == progress.other })?.title ?? "saved contact", requestClass: nil, close: { reviewingRequest = false })
                } else {
                    Text("No request workflow to review.").padding()
                }
            }
        }
        .sheet(isPresented: $managingContact) {
            if let sender = model.selectedSender {
                VStack { HStack { Text("Contact permission").font(.headline); Spacer(); Button("Done") { managingContact = false } }.padding(); InterfaceCContactControlsView(model: model, sender: sender) }
            }
        }
        .sheet(isPresented: $enrolling) {
            VStack(alignment: .leading, spacing: 18) {
                Text("Sign in and enroll this device").font(.title2.bold())
                Text("Local development: use fictional applicant details and a simulated bank account. Your browser handles sign-in.")
                    .foregroundStyle(.secondary)
                Button("Use Marcus’s fictional development evidence") {
                    name = "Marcus Development Fixture"; birthDate = "1985-06-17"
                    address = "101 Fictional Development Avenue"; phone = "+1 555 019 0101"
                    bankAccount = "csqd-development-marcus-simulated-bank"
                }
                Form {
                    TextField("Service URL", text: $server)
                    TextField("Full name", text: $name)
                    TextField("Date of birth (YYYY-MM-DD)", text: $birthDate)
                    TextField("Address", text: $address)
                    TextField("Phone", text: $phone)
                    TextField("Simulated bank account", text: $bankAccount)
                }.textFieldStyle(.roundedBorder)
                Text("If interrupted, use these same details to resume your reserved enrollment.").font(.caption).foregroundStyle(.secondary)
                HStack {
                    Button("Cancel") { enrolling = false }
                    Spacer()
                    Button("Continue to sign-in") {
                        let details = EnrollmentDetails(name: name, dateOfBirth: birthDate, address: address, phone: phone, bankAccount: bankAccount)
                        enrolling = false
                        name = ""; birthDate = ""; address = ""; phone = ""; bankAccount = ""
                        Task { await model.signIn(server: server, details: details) }
                    }.buttonStyle(.borderedProminent)
                        .disabled([server,name,birthDate,address,phone,bankAccount].contains { $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty })
                }
            }.padding(28).frame(idealWidth: 500)
        }
        .confirmationDialog(decisionTitle, isPresented: Binding(get: { confirmation != nil }, set: { if !$0 { confirmation = nil } }), titleVisibility: .visible) {
            if let target = confirmation {
                Button(decisionAction) {
                    confirmation = nil
                    Task { await model.decide(sender: target.sender, request: target.request, decision: target.decision) }
                }
            }
            Button("Cancel", role: .cancel) { confirmation = nil }
        } message: {
            Text(decisionExplanation)
        }
        .onChange(of: phase) { _, phase in
            if phase == .background && !model.authenticating { confirmation = nil; model.lock() }
            if phase == .active && model.inbox != nil && !model.busy {
                Task { await model.refresh() }
                if destination.area == .lanes || destination == .communications { Task { await model.loadManagedAddresses() } }
            }
        }
        .onDisappear { model.endReading() }
        .preference(key: InterfaceSwitchAvailability.self, value: canSwitchInterface)
        .environment(\.interfaceCMaterialArea, destination.area)
    }
    // Switching rebuilds presentation only, and is limited to a neutral workspace.
    private var canSwitchInterface: Bool {
        destination == .home && !model.busy && !model.directory.busy && !model.authenticating
            && !showingDrafts && !readingReference && requestTarget == nil
            && !reviewingRequest && !managingContact && !enrolling && composition == nil
            && policyConfirmation == nil && confirmation == nil && pendingNavigation == nil
    }
    private func memberShell(_ inbox: Inbox) -> some View {
        let shell = InterfaceCEditorialNavigationShell(selected: destination.area,
            columnBVisible: destination.area != .home && destination.area != .notes && columnVisibility != .detailOnly,
            navigate: navigateArea,
            preview: navigationPreview, content: { memberWorkspace(inbox) })
        return shell
            .onChange(of: destination.area) { _, area in
                updateColumnVisibility(area)
            }
            .task(id: destination) {
                guard destination.area == .lanes else { return }
                while !Task.isCancelled {
                    if phase == .active && !model.busy { await model.loadManagedAddresses() }
                    do { try await Task.sleep(for: .seconds(60)) } catch { return }
                }
            }
            .task {
                while !Task.isCancelled {
                    do { try await Task.sleep(for: .seconds(60)) } catch { return }
                    if phase == .active && !model.busy { await model.refresh() }
                }
            }
            .onChange(of: model.selectedSender) { _, sender in selectedSenderChanged(sender) }
            .toolbar { memberToolbar }
    }
    private func updateColumnVisibility(_ area: MemberArea) {
        columnVisibility = area == .home || area == .notes ? .detailOnly : .all
    }
    private func selectedSenderChanged(_ sender: SenderReference?) {
        confirmation = nil; managingContact = false; reviewingRequest = false
        if let sender, showsCorrespondence { Task { await model.select(sender) } }
    }
    private func checkpoint() -> NavigationCheckpoint {
        let title: String
        switch destination {
        case .communications: title = model.inboxTitle + " · " + mailboxNavigation.returnTitle
        case .directory: title = "Directory"
        case .address: title = "My approach page"
        case .addresses: title = "My aliases"
        case .newAlias: title = "New alias"
        case .aliasesGuide: title = "How aliases and express lanes work"
        case .senderPermissions: title = "Permissions for particular senders"
        case .externalEmails, .correspondence: title = model.conversation?.title ?? "Correspondence"
        default: title = destination.area.title
        }
        return NavigationCheckpoint(destination: destination, sidebar: sidebarTarget, sender: model.selectedSender,
            thread: model.openedThread, message: model.focusedMessage, passage: model.focusedSearchPassage, title: title, inbox: model.selectedInbox,
            mailbox: mailboxNavigation.snapshot(),
            laneQuery: laneQuery, laneDirection: laneDirection, laneReceivingAlias: laneReceivingAlias,
            address: managedAddressDestination)
    }
    private func navigateSafely(_ action: @escaping () -> Void) {
        guard composition?.finishing != true else { return }
        navigationRevision += 1
        let requested = navigationRevision
        let currentAction = {
            guard requested == navigationRevision else { return }
            action()
        }
        if destination == .address && model.approachDraft.isDirty { pendingNavigation = currentAction }
        else if let composition {
            composition.savingForNavigation = true
            Task {
                defer { composition.savingForNavigation = false }
                guard await composition.saveCurrent() else {
                    if requested == navigationRevision { compositionNavigation = currentAction }
                    return
                }
                guard requested == navigationRevision else { return }
                self.composition = nil
                model.clearSourceContact()
                currentAction()
            }
        }
        else { currentAction() }
    }
    private func pushNavigation(_ action: @escaping () -> Void) {
        navigateSafely {
            if returnStack.count == 32 { returnStack.removeFirst() }
            returnStack.append(checkpoint())
            model.endReading(); navigationRevision += 1
            action()
        }
    }
    private var backTitle: String {
        guard let source = returnStack.last else { return "Inbox" }
        return source.title == "Inbox" ? "Inbox" : "Back to " + source.title
    }
    private func goBack() {
        navigateSafely {
            guard let source = returnStack.popLast() else { resetInbox(); return }
            model.endReading(); navigationRevision += 1
            if model.selectedInbox != source.inbox { model.changeInbox(source.inbox) }
            mailboxNavigation.restore(source.mailbox)
            laneQuery = source.laneQuery; laneDirection = source.laneDirection; laneReceivingAlias = source.laneReceivingAlias
            managedAddressDestination = source.address
            sidebarTarget = source.sidebar; destination = source.destination
            model.openedThread = source.thread; model.focusedMessage = source.message; model.focusedSearchPassage = source.passage
            model.chooseSender(source.sender)
            if let sender = source.sender, showsCorrespondence { Task { await model.select(sender) } }
        }
    }
    private func openComposition(_ target: WorkspaceComposeTarget) {
        pushNavigation {
            model.endReading()
            showingDrafts = false
            destination = .communications
            composition = WorkspaceComposition(target)
        }
    }
    private func closeComposition() {
        composition = nil
        model.clearSourceContact()
        goBack()
    }
    private func compositionEditor(_ workspace: WorkspaceComposition) -> some View {
        WorkspaceCompositionSurface(workspace: workspace) { compositionEditorContent($0) }
    }
    private func compositionSources(_ workspace: WorkspaceComposition) -> some View {
        WorkspaceCompositionSurface(workspace: workspace) { compositionSourcesContent($0) }
    }
    @ViewBuilder private func compositionEditorContent(_ workspace: WorkspaceComposition) -> some View {
        VStack(spacing: 0) {
        switch workspace.target {
        case .native(let contact, let conversation, let saved, let alias):
            InterfaceCMessageComposerView(model: model, contact: contact, conversationID: conversation,
                dismiss: closeComposition, savedDraft: saved, workspace: workspace, draft: workspace.draft, receivingAlias: alias, requestRelationship: {
                    pushNavigation {
                        let requested = navigationRevision
                        Task {
                            await model.directory.open(AddressReference(alias ?? contact.sender))
                            if requested == navigationRevision { sidebarTarget = nil; destination = .directory }
                        }
                    }
                })
                .id(workspace.id)
        case .external(let thread, let saved):
            InterfaceCExternalEmailComposer(model: model, thread: thread, completed: { _ in closeComposition() },
                savedDraft: saved, workspace: workspace, draft: workspace.draft, close: closeComposition)
                .id(workspace.id)
        case .unaddressed(let saved):
            InterfaceCUnaddressedComposer(model: model, workspace: workspace, draft: workspace.draft, savedDraft: saved, close: closeComposition)
        }
        }
    }
    @ViewBuilder private func compositionSourcesContent(_ workspace: WorkspaceComposition) -> some View {
            switch workspace.target {
            case .native(let contact, let conversation, let saved, let alias):
                InterfaceCMessageComposerView(model: model, contact: contact, conversationID: conversation,
                    dismiss: closeComposition, savedDraft: saved, workspace: workspace, draft: workspace.draft,
                    sourcesOnly: true, receivingAlias: alias).id(workspace.id)
            case .unaddressed, .external: CompositionCorrespondenceHelper(model: model, workspace: workspace)
            }
    }
    private func resetInbox() {
        model.chooseSender(nil)
        model.openedThread = nil; model.focusedMessage = nil; model.focusedSearchPassage = nil
        model.changeInbox(nil)
        sidebarTarget = nil; destination = .communications
        mailboxNavigation.reset(); returnStack = []; navigationRevision += 1
    }
    private func returnToInbox() { navigateSafely { resetInbox() } }
    private func navigateArea(_ area: MemberArea) {
        navigateSafely {
            if area == .communications { resetInbox() }
            else {
                model.chooseSender(nil); model.openedThread = nil; model.focusedMessage = nil; model.focusedSearchPassage = nil
                returnStack = []; navigationRevision += 1
                destination = area.destination; sidebarTarget = nil; model.changeInbox(nil)
            }
        }
    }
    @ViewBuilder private func navigationPreview(_ area: MemberArea, _ dismiss: @escaping () -> Void,
                                   _ focusChanged: @escaping (Bool) -> Void) -> some View {
        switch area {
        case .home:
            InterfaceCWorkspaceLayoutControls(title: "Main overview", choices: HomeArrangement.allCases, label: { $0.title }, selection: $homeArrangement,
                explanation: "Keep the same permission sections, with cards or a single-column list.", focusChanged: focusChanged)
        case .communications:
            InterfaceCWorkspaceLayoutControls(title: "Column B section order", choices: MailboxSectionOrder.allCases, label: { $0.title }, selection: $mailboxSectionOrder,
                explanation: "Put receiving addresses or mailboxes at the top of the organizer.", focusChanged: focusChanged)
        case .relationships:
            InterfaceCRelationshipLayoutControls(overview: $relationshipOverview, focusChanged: focusChanged)
        case .lanes:
            InterfaceCWorkspaceLayoutControls(title: "Column B section order", choices: LaneSectionOrder.allCases, label: { $0.title }, selection: $laneSectionOrder,
                explanation: "Put your aliases or sender permissions first. Both remain available.", focusChanged: focusChanged)
        case .notes:
            InterfaceCWorkspaceLayoutControls(title: "Reading width", choices: NotesReadingWidth.allCases, label: { $0.title }, selection: $notesReadingWidth,
                explanation: "Adjust the current Notes page. Creating and saving notes is still planned.", focusChanged: focusChanged)
        }
    }

    private func selectMailbox(_ folder: MailboxFolder) {
        navigateSafely {
            model.endReading(); model.chooseSender(nil); model.openedThread = nil; model.focusedMessage = nil; model.focusedSearchPassage = nil
            sidebarTarget = nil; destination = .communications; returnStack.removeAll()
            mailboxNavigation.reset(); mailboxNavigation.folder = folder
        }
    }
    @ToolbarContentBuilder private var memberToolbar: some ToolbarContent {
        ToolbarItemGroup {
            if destination.area == .communications {
                Button("Inbox", systemImage: "tray", action: returnToInbox)
                    .labelStyle(.titleAndIcon).help("Return to Inbox")
                Button("Write an email", systemImage: "square.and.pencil") {
                    openComposition(.unaddressed(nil))
                }.disabled(model.busy || !model.connected || model.externalInbox?.pending != nil)
            }
            if destination != .notes {
                Button("Refresh", systemImage: "arrow.clockwise") { Task {
                    if destination == .directory { await model.directory.refresh() }
                    else if destination == .address { await model.loadAddressSettings() }
                    else if destination == .addresses { await model.loadManagedAddresses() }
                    else {
                        await model.refresh()
                        if destination.area == .lanes { await model.loadManagedAddresses() }
                    }
                } }.disabled(model.busy || model.directory.busy)
            }
            Button("Lock", systemImage: "lock") { navigateSafely { confirmation = nil; model.lock() } }
        }

    }
    @ViewBuilder private func memberWorkspace(_ inbox: Inbox) -> some View {
        if destination.area == .home || destination.area == .notes {
            workspaceDetail(inbox)
        } else {
            NavigationSplitView(columnVisibility: $columnVisibility) {
                navigationColumn(area: destination.area)
                    .workspaceColumn()
                    #if os(macOS)
                    .toolbar(removing: .sidebarToggle)
                    #endif
                    .anchorPreference(key: InterfaceCColumnBBounds.self, value: .bounds) { $0 }
                    // Keep the organizer clear of the toolbar and window bottom.
                    .padding(.vertical, InterfaceCWorkspaceLayout.verticalInset)
                    .background(alignment: .bottom) {
                        InterfaceCWorkspaceBackdrop().frame(height: InterfaceCWorkspaceLayout.verticalInset)
                    }
            } detail: {
                workspaceDetail(inbox)
            }
            .overlayPreferenceValue(InterfaceCColumnBBounds.self) { anchor in
                GeometryReader { geometry in
                    if let anchor {
                        WorkspaceSplitSurround(sidebarBounds: geometry[anchor], topInset: geometry.safeAreaInsets.top) {
                            InterfaceCWorkspaceBackdrop()
                        }
                    }
                }
            }
        }
    }
    /// Every recorded origin has a visible return action, including mailbox and unavailable-detail screens.
    private func workspaceDetail(_ inbox: Inbox) -> some View {
        VStack(spacing: 0) {
            if composition == nil && !returnStack.isEmpty && !detailHasReturn {
                HStack {
                    InterfaceCBackButton(backTitle, action: goBack)
                    Spacer()
                }.padding(.horizontal, 24).padding(.vertical, 12)
            }
            Group {
                if let composition { compositionEditor(composition) }
                else if showsCorrespondence { correspondence(inbox: inbox) }
                else { destinationContent.disabled(model.busy) }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            // The return action occupies its own layout space. Scrolling and
            // overscroll stay within the detail viewport rather than under it.
            .clipped()
        }
        .clipShape(RoundedRectangle(cornerRadius: hasContactSurface ? 0 : InterfaceCWorkspaceLayout.surfaceCornerRadius))
        .background {
            if !hasContactSurface {
                InterfaceCWorkspaceBackdrop().clipShape(RoundedRectangle(cornerRadius: InterfaceCWorkspaceLayout.surfaceCornerRadius))
            }
        }
        .overlay {
            if !hasContactSurface {
                InterfaceCMainSpacePerimeter(in: RoundedRectangle(cornerRadius: InterfaceCWorkspaceLayout.surfaceCornerRadius))
            }
        }
        .padding(.horizontal, hasContactSurface ? 0 : InterfaceCWorkspaceLayout.horizontalInset)
        .padding(.vertical, hasContactSurface ? 0 : InterfaceCWorkspaceLayout.verticalInset)
        .background { InterfaceCWorkspaceBackdrop(surface: .workspace) }
        .environment(\.interfaceCSurface, .mainSpace)
    }
    private var hasContactSurface: Bool { composition == nil && ((showsCorrespondence && model.selectedSender != nil) || destination == .externalEmails) }
    private var detailHasReturn: Bool {
        if destination == .externalEmails { return true }
        return showsCorrespondence && model.selectedSender != nil && model.conversation?.sender == model.selectedSender?.value
    }
    private var summaries: [EditorialConversationSummary] {
        model.presentationContacts.map { contact in
            let lane = model.navigationContacts?.first { $0.sender == contact.sender }?.incomingLane
            let subtitle = route(contact.permissionRoute) == .bounded
                ? lane.map { "Until " + SidebarProjection.shortDate($0.expiresAt) } ?? contact.permission
                : contact.permission
            return EditorialConversationSummary(id: SenderReference(contact.sender), title: contact.title, subtitle: subtitle,
                external: contact.external, awaitingDecision: contact.requests.contains(where: \.awaitingDecision),
                route: route(contact.permissionRoute))
        }
    }
    private var showsCorrespondence: Bool {
        switch destination {
        case .communications, .correspondence: return true
        default: return false
        }
    }
    private func openContact(_ sender: SenderReference, area: MemberArea) {
        pushNavigation { showContact(sender, area: area) }
    }
    private func showContact(_ sender: SenderReference, area: MemberArea) {
        sidebarTarget = .contact(sender.value)
        if model.externalInbox?.threads.contains(where: { $0.guest == sender.value }) == true {
            sidebarTarget = .external(sender.value)
            model.chooseSender(sender); destination = .externalEmails; return
        }
        destination = .correspondence(area == .home ? .communications : area)
        chooseSidebarSender(sender.value)
    }
    private var senderPermissionEntries: [SidebarEntry] {
        let entries = SidebarProjection.senderLanes(contacts: model.navigationContacts ?? [])
        if laneDirection == .incoming, let alias = laneReceivingAlias {
            return entries.filter { entry in
                if case .senderLane(let sender, _) = entry.target {
                    return model.senderLaneScopes.contains { $0.sender == sender && $0.aliases.contains(alias) }
                }
                return true
            }
        }
        return entries
    }
    @ViewBuilder private func navigationColumn(area: MemberArea) -> some View {
        Group {
            if let composition {
                compositionSources(composition)
            } else if area == .communications {
                InterfaceCAliasInboxSidebar(model: model, navigation: mailboxNavigation, sectionOrder: mailboxSectionOrder, manage: {
                    pushNavigation { model.chooseSender(nil); sidebarTarget = nil; destination = .addresses }
                }, select: { id in
                    navigateSafely {
                        model.endReading(); model.chooseSender(nil); model.openedThread = nil; model.focusedMessage = nil; model.focusedSearchPassage = nil
                        model.changeInbox(id); sidebarTarget = nil; destination = .communications
                        mailboxNavigation.reset(); returnStack.removeAll()
                    }
                }, selectMailbox: selectMailbox, drafts: { navigateSafely { showingDrafts = true } })
            } else {
                VStack(spacing: 0) {
                    if area == .relationships {
                        InterfaceCRelationshipOrganizer(model: model, destination: destination,
                            navigate: { target in
                                if target == .relationships { navigateArea(.relationships) }
                                else { pushNavigation { model.chooseSender(nil); sidebarTarget = nil; destination = target } }
                            }, reviewSaved: { reviewingRequest = true })
                    } else if area == .lanes {
                        let aliases: [SidebarEntry] = (model.addressWorkspace?.addresses ?? []).map { address in
                            let target = SidebarTarget.aliasLane(address.id)
                            return SidebarEntry(target: target, item: EditorialNavigationItem(id: target.id, title: address.description ?? address.address,
                                subtitle: (address.policy == .bonded ? "Bonded requests · " : "Contact through address · ") + AliasLanePresentation.summary(address.state)))
                        }
                        InterfaceCEditorialLaneOrganizer(showGuide: {
                            pushNavigation { model.chooseSender(nil); sidebarTarget = nil; destination = .aliasesGuide }
                        }, createAlias: {
                            pushNavigation { model.chooseSender(nil); sidebarTarget = nil; destination = .newAlias }
                        }, showPermissions: {
                            pushNavigation { model.chooseSender(nil); sidebarTarget = nil; destination = .senderPermissions }
                        }, guideSelected: destination == .aliasesGuide, creationSelected: destination == .newAlias,
                            permissionsSelected: destination == .senderPermissions, aliasesFirst: laneSectionOrder == .aliasesFirst,
                            search: $aliasQuery, aliasItems: aliases.map(\.item), aliasLoaded: model.addressesCurrent,
                            aliasError: model.addressManagementError, selection: sidebarTarget?.id,
                            select: { id in
                                if let entry = aliases.first(where: { $0.target.id == id }) { selectSidebar(entry.target, area: .lanes) }
                            })
                    }
                    if hasSidebarStatus {
                        InterfaceCMaterialDivider()
                        ScrollView { status.padding(18).frame(maxWidth: .infinity, alignment: .leading) }.frame(maxHeight: 160)
                    }
                }
            }
        }
        .environment(\.columnBNavigation, true)
        .foregroundStyle(InterfaceCEditorialStyle.ink(scheme), InterfaceCEditorialStyle.secondaryInk(scheme))
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .clipped()
        .background {
            // Cover the native sidebar backing outside the bowed panel.
            InterfaceCWorkspaceBackdrop().ignoresSafeArea()
            InterfaceCSidebarSurface()
        }
    }
    private func selectSidebar(_ target: SidebarTarget, area: MemberArea) {
        pushNavigation { applySidebar(target, area: area) }
    }
    private func applySidebar(_ target: SidebarTarget, area: MemberArea) {
        sidebarTarget = target
        switch target {
        case .external(let sender):
            model.chooseSender(SenderReference(sender)); destination = .externalEmails
        case .aliasLane(let id):
            model.chooseSender(nil); managedAddressDestination = .address(id); destination = .addresses
        case .standingPermission:
            model.chooseSender(nil); destination = .lanes
        case .senderLane(let sender, _):
            destination = .lanes
            chooseSidebarSender(sender)
        case .request(let sender, _, _):
            destination = .correspondence(.relationships)
            chooseSidebarSender(sender)
        case .contact(let sender):
            destination = .correspondence(area)
            chooseSidebarSender(sender)
        }
    }
    private func chooseSidebarSender(_ value: String) {
        let sender = SenderReference(value)
        if model.selectedSender == sender { Task { await model.select(sender) } }
        else { model.chooseSender(sender) }
    }
    private var standingPermissionEntries: [SidebarEntry] {
        (model.navigationContacts ?? []).compactMap { contact in
            let permission = laneDirection == .incoming ? contact.incomingPermission : contact.outgoingPermission
            guard permission == .standing else { return nil }
            let target = SidebarTarget.standingPermission(contact.sender, laneDirection)
            return SidebarEntry(target: target, item: EditorialNavigationItem(id: target.id, title: contact.title,
                subtitle: laneDirection == .incoming ? "You granted ongoing access" : "Granted you ongoing access",
                external: contact.external, direction: laneDirection))
        }
    }
    private var senderPermissions: some View {
        InterfaceCSenderPermissionsView(search: $laneQuery, direction: $laneDirection, receivingSelection: $laneReceivingAlias,
            items: senderPermissionEntries.map(\.item), standingItems: standingPermissionEntries.map(\.item),
            receivingChoices: model.aliasInboxes.map { EditorialNavigationItem(id: $0.address.id,
                title: $0.address.description ?? $0.address.address, subtitle: "") },
            loading: model.navigationContacts == nil && model.navigationError == nil, error: model.navigationError,
            select: { id in
                if let entry = (senderPermissionEntries + standingPermissionEntries).first(where: { $0.target.id == id }) {
                    selectSidebar(entry.target, area: .lanes)
                }
            })
    }
    @ViewBuilder private var destinationContent: some View {
        switch destination {
        case .communications, .correspondence: EmptyView()
        case .home:
            InterfaceCEditorialMembraneView(conversations: summaries, listLayout: homeArrangement == .list, selection: Binding(get: { nil }, set: { sender in
                if let sender { openContact(sender, area: .communications) }
            }))
        case .externalEmails:
            InterfaceCEditorialPortalView(person: "", route: .closed, returnTitle: backTitle, returnToSpace: goBack) {
                InterfaceCExternalEmailView(model: model)
            }
        case .directory:
            InterfaceCDirectoryView(model: model.directory, member: model,
                openCorrespondence: { sender in openContact(sender, area: .relationships) },
                reviewRequest: { approach, requestClass in
                    requestTarget = RequestTarget(other: SenderReference(approach.address), name: approach.displayName, requestClass: requestClass, alias: approach.alias)
                })
        case .addresses:
            InterfaceCManagedAddressView(model: model, destination: $managedAddressDestination, create: { pushNavigation { sidebarTarget = nil; destination = .newAlias } })
        case .newAlias:
            InterfaceCAliasCreationView(model: model, draft: model.aliasCreationDraft, showAlias: { id in
                if destination == .newAlias {
                    managedAddressDestination = .address(id); destination = .addresses
                }
            })
        case .address: InterfaceCAddressEditor(model: model, draft: model.approachDraft)
        case .aliasesGuide: AliasesGuideView(heading: InterfaceCEditorialStyle.heading(30), bodyFont: InterfaceCEditorialStyle.reading)
        case .senderPermissions: senderPermissions
        case .relationships:
            if relationshipOverview == .map {
                InterfaceCEditorialRelationshipMap(people: summaries) { sender in openContact(sender, area: .relationships) }
            } else {
                InterfaceCRelationshipOverviewList(people: summaries) { sender in openContact(sender, area: .relationships) }
            }
        case .lanes:
            if case .senderLane(let sender, let direction) = sidebarTarget,
               let contact = model.navigationContacts?.first(where: { $0.sender == sender }) {
                InterfaceCLaneDetailView(contact: contact, direction: direction,
                    receivingAddresses: direction == .incoming ? model.senderLaneScopes.first(where: { $0.sender == sender && $0.lane == contact.incomingLane?.id })?.aliases.compactMap { id in model.aliasInboxes.first(where: { $0.address.id == id })?.address } : nil,
                    manage: { managingContact = true }, openCorrespondence: { openContact(SenderReference(sender), area: .communications) })
            } else if case .standingPermission(let sender, let direction) = sidebarTarget,
                      let contact = model.navigationContacts?.first(where: { $0.sender == sender }) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        Text(contact.title).font(InterfaceCEditorialStyle.heading(30))
                        Text("Standing permission").font(.headline)
                        if (direction == .incoming ? contact.incomingPermission : contact.outgoingPermission) == .standing {
                            Text(direction == .incoming ? "You have granted this sender ongoing permission to contact you." : "This person has granted you ongoing permission to contact them.")
                            Text("Access remains subject to the grant’s receiving aliases, revocation, blocks and service coverage.")
                                .foregroundStyle(.secondary)
                            Text("Receiving-alias details are not available in this view.").font(.callout).foregroundStyle(.secondary)
                            if direction == .outgoing {
                                Text("Only the person granting this permission can change or revoke it.").font(.callout)
                            }
                            Button("Open correspondence") { openContact(SenderReference(sender), area: .communications) }
                                .buttonStyle(InterfaceCEditorialButtonStyle())
                        } else {
                            Text("This standing permission is no longer current.").foregroundStyle(.secondary)
                        }
                    }.padding(32).frame(maxWidth: 720, alignment: .leading).frame(maxWidth: .infinity, alignment: .leading)
                }
            } else { AliasesGuideView(heading: InterfaceCEditorialStyle.heading(30), bodyFont: InterfaceCEditorialStyle.reading) }
        case .notes:
            VStack(alignment: .leading, spacing: 18) {
                InterfaceCEditorialReadingView(blocks: [EditorialReadingBlock("Notes", style: .title),
                    EditorialReadingBlock("A private space for writing is planned. Notes cannot be created or saved yet.")])
            }.frame(maxWidth: notesReadingWidth.points, alignment: .leading).padding(40)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                .background { InterfaceCWorkspaceBackdrop() }
        }
    }
    private func correspondence(inbox: Inbox) -> some View {
        Group {
            if let conversation = model.conversation, conversation.sender == model.selectedSender?.value {
                InterfaceCEditorialPortalView(person: conversation.title,
                    route: route(conversation.permissionRoute),
                    returnTitle: backTitle, returnToSpace: goBack) {
                    InterfaceCContactCorrespondenceView(model: model, conversation: conversation, inbox: inbox,
                        selectedRequest: selectedRequest,
                        manageContact: { managingContact = true },
                        requestRelationship: {
                            pushNavigation {
                                let requested = navigationRevision
                                Task {
                                    await model.directory.open(AddressReference(conversation.sender))
                                    if requested == navigationRevision { sidebarTarget = nil; destination = .directory }
                                }
                            }
                        }, reviewPolicy: reviewPolicy,
                        resolve: { message, part in readingReference = true; Task { await model.resolveReference(message: message, part: part) } },
                        decide: { decision, request in confirm(decision, conversation: conversation, request: request) })
                        .id(conversation.sender)
                }
            } else if model.selectedSender == nil {
                InterfaceCMailboxesView(model: model, navigation: mailboxNavigation, permissions: {
                    guard let id = model.selectedInbox else { return }
                    pushNavigation {
                        model.chooseSender(nil); laneDirection = .incoming; laneReceivingAlias = id
                        if model.aliasInboxes.first(where: { $0.address.id == id })?.address.permission != .unpaired {
                            sidebarTarget = .aliasLane(id)
                        } else { sidebarTarget = nil }
                        destination = .lanes
                    }
                }) { item, message in
                    pushNavigation {
                        model.openedThread = item.key.conversation; model.focusedMessage = message.message; model.focusedSearchPassage = message.passage
                        showContact(SenderReference(item.other), area: .communications)
                    }
                }
            } else {
                InterfaceCEditorialEmptyView(title: model.selectedSender == nil ? "Choose a conversation" :
                    model.busy ? "Opening correspondence…" : "Conversation unavailable. Try Refresh.")
            }
        }
    }
    private var selectedRequest: SidebarRequestSelection? {
        if case .request(_, let request, let direction) = sidebarTarget {
            return SidebarRequestSelection(id: request, direction: direction)
        }
        return nil
    }
    private func reviewPolicy(_ id: String, _ policy: ConversationPolicy, _ action: PolicyAction) {
        let restricting = action == .restrict
        policyConfirmation = PolicyConfirmation(conversation: id, revision: policy.revision, action: action,
            title: restricting ? "Restrict future reuse?" : "Allow wider reuse?",
            explanation: restricting ? "Future reuse must stay within this pair. Already delivered copies remain. Any pending relaxation proposal is cancelled." : "Both participants must agree before wider reuse is enabled. Known source and ancestor restrictions still apply.")
    }
    private var decisionTitle: String {
        switch confirmation?.decision {
        case .accept: return "Accept this request?"
        case .expressLane: return "Grant a seven-day express lane?"
        default: return "Reject this request?"
        }
    }
    private var decisionAction: String {
        switch confirmation?.decision {
        case .accept: return "Accept and grant standing permission"
        case .expressLane: return "Grant seven-day express lane"
        default: return "Reject request"
        }
    }
    private var decisionExplanation: String {
        switch confirmation?.decision {
        case .accept: return "Grant standing permission for future contact from this person."
        case .expressLane: return "Allow personal, human-initiated contact for seven days, up to 20 messages per day."
        default: return "Decline this request. No standing permission is granted, and later approaches are not blocked."
        }
    }
    private func confirm(_ decision: Decision, conversation: Conversation, request: RequestID) {
        guard conversation.requests.contains(where: { $0.id == request.value && $0.awaitingDecision }) else { return }
        confirmation = DecisionConfirmation(sender: SenderReference(conversation.sender), request: request, decision: decision)
    }
    private func route(_ value: PermissionRoute) -> EditorialRoute {
        switch value {
        case .standing: return .standing
        case .bounded: return .bounded
        case .closed: return .closed
        }
    }
    private func pendingSetupExplanation(_ setup: PendingSetup) -> String {
        switch setup {
        case .deviceRegistration: return "Device setup is unfinished."
        case .contentKeyPublication: return "Secure messaging setup is unfinished."
        case .servicePurchase: return "Your service purchase has not been confirmed."
        case .aliasManagement: return "Your alias change has not been confirmed."
        case .directoryPublication: return "Your profile changes have not been confirmed."
        }
    }
    private var statusBlocks: [EditorialReadingBlock] {
        var blocks: [EditorialReadingBlock] = []
        if let error = model.memberError { blocks.append(EditorialReadingBlock(error, style: .warning)) }
        if let inbox = model.inbox {
            if inbox.pendingPolicy { blocks.append(EditorialReadingBlock(inbox.pendingDeletion == nil ? "Your sharing change has not been confirmed." : "Your copy removal has not been confirmed.", style: .caption)) }
            if inbox.pendingSend != nil { blocks.append(EditorialReadingBlock("Your message’s delivery has not been confirmed.", style: .caption)) }
            if inbox.pendingSender != nil { blocks.append(EditorialReadingBlock("Your permission change has not been confirmed.")) }
            if let setup = inbox.pendingSetup { blocks.append(EditorialReadingBlock(pendingSetupExplanation(setup))) }
        }
        return blocks
    }
    private var hasSidebarStatus: Bool {
        if destination == .address || destination == .addresses { return false }
        if !statusBlocks.isEmpty { return true }
        guard let inbox = model.inbox else { return false }
        return inbox.pendingRequest || inbox.conversations.contains { $0.controls.laneExpiryReminder != nil }
    }
    @ViewBuilder private var status: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !statusBlocks.isEmpty { InterfaceCEditorialReadingView(blocks: statusBlocks) }
            if let inbox = model.inbox {
                ForEach(inbox.conversations.filter { $0.controls.laneExpiryReminder != nil }, id: \.sender) { contact in
                    Button("Review expiring lane for \(contact.title)") {
                        openContact(SenderReference(contact.sender), area: .lanes)
                    }.font(.caption)
                }
                if inbox.pendingRequest { Button("Review pending request") { reviewingRequest = true }.disabled(model.busy) }
                if inbox.pendingPolicy { Button(inbox.pendingDeletion == nil ? "Check sharing change" : "Check copy removal") { Task { await model.retry() } }.disabled(model.busy) }
                if inbox.pendingSend != nil { Button("Check message status") { Task { await model.retry() } }.disabled(model.busy) }
                if inbox.pendingSender != nil {
                    Button("Check permission change") { Task { await model.retry() } }.disabled(model.busy)
                }
                if inbox.pendingSetup != nil {
                    Button("Check update") { Task { await model.retry() } }.disabled(model.busy)
                }
            }
        }
    }
}

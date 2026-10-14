import SwiftUI
import CsMailRust
import CsMailPresentation

public enum DevelopmentAccountAccess { case available, signingSetupRequired }

public struct MemberView: View {
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
    @AppStorage("csMailAppearance") private var appearance: Appearance = .system
    @StateObject private var model: MemberModel
    private let accountAccess: DevelopmentAccountAccess
    @State private var policyConfirmation: PolicyConfirmation?
    private struct PolicyConfirmation { let conversation: String; let revision: UInt64; let action: PolicyAction; let title: String; let explanation: String }
    @State private var readingReference = false
    private struct RequestTarget: Identifiable {
        let other: SenderReference
        let name: String
        let requestClass: RequestClassID
        var id: String { other.value + ":" + requestClass.value }
    }
    @State private var requestTarget: RequestTarget?
    @State private var reviewingRequest = false
    @State private var managingContact = false
    @State private var enrolling = false
    @State private var externalComposeRequested = false
    @State private var destination: MemberDestination = .home
    @State private var sidebarTarget: SidebarTarget?
    @State private var columnVisibility: NavigationSplitViewVisibility = .detailOnly
    @State private var server = "http://127.0.0.1:8480"
    @State private var name = ""
    @State private var birthDate = ""
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
    public init(directory: URL? = nil, accountAccess: DevelopmentAccountAccess = .available, serviceURL: String = "http://127.0.0.1:8480", credentialService: String = "com.csqd.member.development", loginBrowserBundleID: String? = nil) {
        _server = State(initialValue: serviceURL)
        self.accountAccess = accountAccess
        _model = StateObject(wrappedValue: MemberModel(directory: directory, credentialService: credentialService, loginBrowserBundleID: loginBrowserBundleID))
    }
    public var body: some View {
        Group {
            if let inbox = model.inbox {
                memberShell(inbox)
            } else {
                VStack(spacing: 22) {
                    Text("cs-mail").font(EditorialStyle.heading(23)).padding(.bottom, 24)
                    Text("Your conversations.\nYour permission.").font(EditorialStyle.heading(34)).multilineTextAlignment(.center)
                    Button("Open account") { Task { await model.unlock() } }.buttonStyle(EditorialButtonStyle(prominent: true)).disabled(accountAccess != .available)
                    Button("Sign in and enroll…") { enrolling = true }.buttonStyle(EditorialButtonStyle()).disabled(accountAccess != .available)
                    Button("Check enrollment status") { Task { await model.resumeEnrollment() } }.disabled(accountAccess != .available)
                    if accountAccess == .signingSetupRequired { Text("Account setup needs stable app signing.").font(.caption).multilineTextAlignment(.center) }
                    Text("Credentials stay in Keychain. The local cache is encrypted.\nDevelopment uses a test login provider and simulated bank.")
                        .font(.caption).multilineTextAlignment(.center).foregroundStyle(.secondary)
                    status
                }.padding(32).frame(maxWidth: .infinity, maxHeight: .infinity).disabled(model.busy)
            }
        }
        .preferredColorScheme(appearance.colorScheme)
        .toolbar {
            ToolbarItem {
                Button(scheme == .dark ? "Use light appearance" : "Use dark appearance",
                       systemImage: scheme == .dark ? "sun.max" : "moon") {
                    appearance = scheme == .dark ? .light : .dark
                }
                .help("Switch between light and dark appearance")
            }
        }
        .task { if accountAccess == .available { await model.unlock() } }
        .onOpenURL { model.receiveLogin($0) }
        .textSelection(.enabled)
        .foregroundStyle(EditorialStyle.ink(scheme))
        .tint(EditorialStyle.ink(scheme))
        .background(EditorialStyle.canvas(scheme))
        .overlay(alignment: .top) {
            if model.busy {
                HStack {
                    ProgressView()
                    if model.authenticating { Button("Cancel sign-in") { model.lock() } }
                }.padding(12).background(.regularMaterial, in: Capsule())
            }
        }
        .onChange(of: model.inbox == nil) { _, locked in
            if locked { readingReference = false; requestTarget = nil; model.clearReference(); destination = .home }
        }
        .confirmationDialog(policyConfirmation?.title ?? "Change reuse policy?", isPresented: Binding(get: { policyConfirmation != nil }, set: { if !$0 { policyConfirmation = nil } }), titleVisibility: .visible) {
            if let target = policyConfirmation { Button("Confirm") { policyConfirmation = nil; Task { await model.changePolicy(conversationID: target.conversation, revision: target.revision, action: target.action) } } }
            Button("Cancel", role: .cancel) { policyConfirmation = nil }
        } message: { Text(policyConfirmation?.explanation ?? "") }
        .sheet(isPresented: $readingReference, onDismiss: { model.clearReference() }) {
            VStack(alignment: .leading, spacing: 18) {
                HStack { Text("Referenced selection").font(.title2); Spacer(); Button("Done") { readingReference = false; model.clearReference() } }
                switch model.reference {
                case .readable(let text): ScrollView { EditorialReadingView(text).privacySensitive() }
                case .unavailable: Text("The source is deleted, expired or otherwise unavailable.")
                case .accessDenied: Text("You do not have access to the retained source. The reference grants no access.")
                case nil: if let error = model.memberError { Text(error).foregroundStyle(.orange) } else { ProgressView() }
                }
            }.padding(24).frame(idealWidth: 560, idealHeight: 430)
        }
        .sheet(item: $requestTarget) { target in
            VStack {
                HStack { Spacer(); Button("Done") { requestTarget = nil } }.padding()
                RequestComposerView(model: model, other: target.other, name: target.name, requestClass: target.requestClass)
            }
        }
        .sheet(isPresented: $reviewingRequest) {
            VStack {
                HStack { Spacer(); Button("Done") { reviewingRequest = false } }.padding()
                if let progress = model.inbox?.requestProgress {
                    RequestComposerView(model: model, other: SenderReference(progress.other), name: model.inbox?.conversations.first(where: { $0.sender == progress.other })?.title ?? "saved contact", requestClass: nil)
                } else {
                    Text("No request workflow to review.").padding()
                }
            }
        }
        .sheet(isPresented: $managingContact) {
            if let sender = model.selectedSender {
                VStack { HStack { Text("Contact permission").font(.headline); Spacer(); Button("Done") { managingContact = false } }.padding(); ContactControlsView(model: model, sender: sender) }
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
        }
        .onDisappear { model.lock() }
    }
    private func memberShell(_ inbox: Inbox) -> some View {
        let shell = EditorialNavigationShell(selected: destination.area, navigate: navigateArea,
            preview: navigationPreview, content: { memberWorkspace(inbox) })
        return shell
            .onChange(of: destination.area) { _, area in updateColumnVisibility(area) }
            .onChange(of: model.selectedSender) { _, sender in selectedSenderChanged(sender) }
            .toolbar { memberToolbar }
    }
    private func updateColumnVisibility(_ area: MemberArea) {
        columnVisibility = area == .home || area == .notes ? .detailOnly : .all
    }
    private func selectedSenderChanged(_ sender: SenderReference?) {
        confirmation = nil; managingContact = false; reviewingRequest = false
        if let sender, destination != .externalEmails {
            Task { await model.select(sender) }
        }
    }
    private func navigateArea(_ area: MemberArea) {
        if area == .communications, let sender = model.selectedSender {
            let external = model.externalInbox?.threads.contains { $0.guest == sender.value } == true
            sidebarTarget = external ? .external(sender.value) : .contact(sender.value)
            destination = external ? .externalEmails : .communications
        } else { destination = area.destination; sidebarTarget = nil }
    }
    private func navigationPreview(_ area: MemberArea, _ dismiss: @escaping () -> Void,
                                   _ focusChanged: @escaping (Bool) -> Void) -> some View {
        navigationColumn(area: area, preview: true, dismiss: dismiss, focusChanged: focusChanged)
    }
    @ToolbarContentBuilder private var memberToolbar: some ToolbarContent {
        ToolbarItemGroup {
            if destination.area == .communications {
                Button("Write an email", systemImage: "square.and.pencil") {
                    sidebarTarget = nil; model.chooseSender(nil)
                    destination = .externalEmails; externalComposeRequested = true
                }.disabled(model.busy || !model.connected || model.externalInbox?.pending != nil)
            }
            if destination != .notes {
                Button("Refresh", systemImage: "arrow.clockwise") { Task { await model.refresh() } }.disabled(model.busy)
            }
            Button("Lock", systemImage: "lock") { confirmation = nil; model.lock() }
        }

    }
    @ViewBuilder private func memberWorkspace(_ inbox: Inbox) -> some View {
        if destination.area == .home || destination.area == .notes {
            destinationContent.disabled(model.busy)
        } else {
            NavigationSplitView(columnVisibility: $columnVisibility) {
                navigationColumn(area: destination.area)
                    .navigationSplitViewColumnWidth(min: 230, ideal: 280, max: 360)
            } detail: {
                ZStack {
                    correspondence(inbox: inbox)
                        .opacity(showsCorrespondence ? 1 : 0)
                        .allowsHitTesting(showsCorrespondence)
                        .accessibilityHidden(!showsCorrespondence)
                    destinationContent.disabled(model.busy)
                }
            }
        }
    }
    private var summaries: [EditorialConversationSummary] {
        model.presentationContacts.map {
            EditorialConversationSummary(id: SenderReference($0.sender), title: $0.title, subtitle: $0.permission,
                external: $0.external, awaitingDecision: $0.requests.contains(where: \.awaitingDecision),
                route: route($0.permissionRoute))
        }
    }
    private var showsCorrespondence: Bool {
        switch destination {
        case .communications, .correspondence: return true
        default: return false
        }
    }
    private func openContact(_ sender: SenderReference, area: MemberArea) {
        guard !model.busy else { return }
        sidebarTarget = .contact(sender.value)
        if area != .economics, model.externalInbox?.threads.contains(where: { $0.guest == sender.value }) == true {
            sidebarTarget = .external(sender.value)
            model.chooseSender(sender); destination = .externalEmails; return
        }
        destination = area == .economics ? .economics : .correspondence(area == .home ? .communications : area)
        if model.selectedSender == sender { Task { await model.select(sender) } }
        else { model.chooseSender(sender) }
    }
    private func sidebarEntries(_ area: MemberArea) -> [SidebarEntry] {
        SidebarProjection.entries(area: area, contacts: model.navigationContacts ?? [],
            external: model.externalInbox?.threads ?? [], conversations: model.presentationContacts)
    }
    @ViewBuilder private func navigationColumn(area: MemberArea, preview: Bool = false, dismiss: @escaping () -> Void = {},
                                  focusChanged: @escaping (Bool) -> Void = { _ in }) -> some View {
        if area == .home || area == .notes {
            // These areas have no item list; keep their hover preview compact.
            Text(area.title).font(EditorialStyle.heading(23)).padding(22)
        } else {
            let entries = sidebarEntries(area)
            EditorialInboxView(items: entries.map(\.item), selection: sidebarTarget?.id ?? (destination == .externalEmails ? model.selectedSender.map { SidebarTarget.external($0.value).id } : nil), area: area,
                destination: destination, hasRequestWorkflow: model.inbox?.pendingRequest == true,
                showsStatus: !preview && hasSidebarStatus, loading: model.navigationContacts == nil && model.navigationError == nil,
                error: model.navigationError, navigate: { target in dismiss(); sidebarTarget = nil; destination = target },
                select: { id in
                    if let id, let entry = entries.first(where: { $0.target.id == id }) {
                        dismiss(); selectSidebar(entry.target, area: area)
                    } else if !preview { sidebarTarget = nil; destination = area.destination }
                }, reviewSavedRequest: { dismiss(); reviewingRequest = true }, previewFocusChanged: focusChanged) {
                    if !preview { status }
                }.id(area).disabled(model.busy)
        }
    }
    private func selectSidebar(_ target: SidebarTarget, area: MemberArea) {
        guard !model.busy else { return }
        sidebarTarget = target
        switch target {
        case .external:
            model.chooseSender(SenderReference(target.sender)); destination = .externalEmails
        case .financial: destination = .economics
        case .lane:
            destination = .lanes
            chooseSidebarSender(target.sender)
        case .request:
            destination = .correspondence(.relationships)
            chooseSidebarSender(target.sender)
        case .contact:
            destination = .correspondence(area)
            chooseSidebarSender(target.sender)
        }
    }
    private func chooseSidebarSender(_ value: String) {
        let sender = SenderReference(value)
        if model.selectedSender == sender { Task { await model.select(sender) } }
        else { model.chooseSender(sender) }
    }
    @ViewBuilder private var destinationContent: some View {
        switch destination {
        case .communications, .correspondence: EmptyView()
        case .home:
            EditorialMembraneView(conversations: summaries, selection: Binding(get: { nil }, set: { sender in
                if let sender { openContact(sender, area: .communications) }
            }))
        case .externalEmails:
            ExternalEmailView(model: model, composeNew: $externalComposeRequested)
        case .directory:
            DirectoryView(model: model.directory, member: model,
                openCorrespondence: { sender in openContact(sender, area: .relationships) },
                reviewRequest: { approach, requestClass in
                    requestTarget = RequestTarget(other: SenderReference(approach.address), name: approach.displayName, requestClass: requestClass)
                })
        case .address: AddressEditor(model: model)
        case .questions: EditorialQuestionsView()
        case .relationships:
            EditorialEmptyView(title: "Choose a request")
        case .lanes:
            if case .lane(let sender, let direction) = sidebarTarget,
               let contact = model.navigationContacts?.first(where: { $0.sender == sender }) {
                LaneDetailView(contact: contact, direction: direction,
                    manage: { managingContact = true }, openCorrespondence: { openContact(SenderReference(sender), area: .communications) })
            } else { EditorialEmptyView(title: "Choose an express lane") }
        case .economics:
            if case .financial(let sender, let request, let direction) = sidebarTarget,
               let contact = model.navigationContacts?.first(where: { $0.sender == sender }),
               let record = (direction == .incoming ? contact.requests : contact.sentRequests).first(where: { $0.record.id == request }) {
                RequestFinancialStatusView(title: contact.title, request: record.record, direction: direction)
            } else { EditorialEmptyView(title: "Choose a financial record") }
        case .notes:
            VStack(alignment: .leading, spacing: 18) {
                EditorialReadingView(blocks: [EditorialReadingBlock("Notes", style: .title),
                    EditorialReadingBlock("A private space for writing is planned. Notes cannot be created or saved yet.")])
            }.padding(40).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .background(EditorialStyle.canvas(scheme))
        }
    }
    private func correspondence(inbox: Inbox) -> some View {
        Group {
            if let conversation = model.conversation, conversation.sender == model.selectedSender?.value {
                EditorialPortalView(person: conversation.title, permission: "Contact from this person: " + conversation.permission,
                    route: route(conversation.permissionRoute), returnToSpace: { destination = .home; sidebarTarget = nil }) {
                    ContactCorrespondenceView(model: model, conversation: conversation, inbox: inbox,
                        selectedRequest: selectedRequest,
                        manageContact: { managingContact = true },
                        requestRelationship: {
                            Task {
                                await model.directory.open(AddressReference(conversation.sender))
                                sidebarTarget = nil; destination = .directory
                            }
                        }, reviewPolicy: reviewPolicy,
                        resolve: { message, part in readingReference = true; Task { await model.resolveReference(message: message, part: part) } },
                        decide: { decision, request in confirm(decision, conversation: conversation, request: request) })
                        .id(conversation.sender)
                }
            } else { EditorialEmptyView(title: model.selectedSender == nil ? "Choose a conversation" : "Opening correspondence…") }
        }.disabled(model.busy)
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
        case .deviceRegistration: return "Device registration is unfinished. Its saved operation can be resumed."
        case .contentKeyPublication: return "Content-key publication is unfinished. Its saved operation can be resumed."
        case .servicePurchase: return "A service purchase has an unresolved outcome. Resume its original operation."
        case .directoryPublication: return "A profile publication has an unresolved outcome. Resume its original operation."
        }
    }
    private var statusBlocks: [EditorialReadingBlock] {
        var blocks: [EditorialReadingBlock] = []
        if let error = model.memberError { blocks.append(EditorialReadingBlock(error, style: .warning)) }
        if let inbox = model.inbox {
            if inbox.pendingPolicy { blocks.append(EditorialReadingBlock("A conversation-policy change has an unresolved outcome.", style: .caption)) }
            if inbox.pendingSend != nil { blocks.append(EditorialReadingBlock("A message has an unresolved outcome.", style: .caption)) }
            if inbox.pendingSender != nil { blocks.append(EditorialReadingBlock("Permission operation unresolved. A retry sends the original operation.")) }
            if let setup = inbox.pendingSetup { blocks.append(EditorialReadingBlock(pendingSetupExplanation(setup))) }
        }
        return blocks
    }
    private var hasSidebarStatus: Bool {
        if !statusBlocks.isEmpty { return true }
        guard let inbox = model.inbox else { return false }
        return inbox.requestProgress != nil || inbox.conversations.contains { $0.controls.laneExpiryReminder != nil }
    }
    @ViewBuilder private var status: some View {
        VStack(alignment: .leading, spacing: 8) {
            if !statusBlocks.isEmpty { EditorialReadingView(blocks: statusBlocks) }
            if let inbox = model.inbox {
                ForEach(inbox.conversations.filter { $0.controls.laneExpiryReminder != nil }, id: \.sender) { contact in
                    Button("Review expiring lane for \(contact.title)") {
                        destination = .correspondence(.lanes)
                        model.chooseSender(SenderReference(contact.sender))
                        Task { await model.select(SenderReference(contact.sender)) }
                    }.font(.caption)
                }
                if inbox.requestProgress != nil { Button(inbox.pendingRequest ? "Review saved request workflow" : "View last request outcome") { reviewingRequest = true }.disabled(model.busy) }
                if inbox.pendingPolicy { Button("Retry original policy change") { Task { await model.retry() } }.disabled(model.busy) }
                if inbox.pendingSend != nil { Button("Retry original send") { Task { await model.retry() } }.disabled(model.busy) }
                if inbox.pendingSender != nil {
                    Button("Retry original permission operation") { Task { await model.retry() } }.disabled(model.busy)
                }
                if inbox.pendingSetup != nil {
                    Button("Resume saved operation") { Task { await model.retry() } }.disabled(model.busy)
                }
            }
        }
    }
}

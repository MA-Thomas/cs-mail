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
    @State private var destination: MemberDestination = .home
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
                EditorialNavigationShell(selected: destination.area, navigate: { destination = $0.destination }) { area, dismiss, focusChanged in
                    navigationColumn(area: area, preview: true, dismiss: dismiss, focusChanged: focusChanged)
                } content: {
                    NavigationSplitView {
                        navigationColumn(area: destination.area)
                            .navigationSplitViewColumnWidth(min: 230, ideal: 280, max: 360)
                    } detail: {
                        ZStack {
                            // Retain the reader and its popup composer across destination changes.
                            correspondence(inbox: inbox)
                                .opacity(showsCorrespondence ? 1 : 0)
                                .allowsHitTesting(showsCorrespondence)
                                .accessibilityHidden(!showsCorrespondence)
                            destinationContent.disabled(model.busy)
                        }
                    }
                }
                .onChange(of: model.selectedSender) { _, sender in
                    confirmation = nil; managingContact = false; reviewingRequest = false
                    if let sender { Task { await model.select(sender) } }
                }
                .toolbar {
                    ToolbarItemGroup {
                        if destination != .notes {
                            Button("Refresh", systemImage: "arrow.clockwise") { Task { await model.refresh() } }.disabled(model.busy)
                        }
                        Button("Lock", systemImage: "lock") { confirmation = nil; model.lock() }
                    }
                }
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
                case .readable(let text): ScrollView { Text(text).textSelection(.enabled).privacySensitive() }
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
    private var summaries: [EditorialConversationSummary] {
        model.presentationContacts.map {
            EditorialConversationSummary(id: SenderReference($0.sender), title: $0.title, subtitle: $0.permission,
                external: $0.external, awaitingDecision: $0.requests.contains(where: \.awaitingDecision),
                route: route($0.permissionRoute), hasReceivedRequests: !$0.requests.isEmpty, hasSentRequests: !$0.sentRequests.isEmpty)
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
        if area != .economics, model.externalInbox?.threads.contains(where: { $0.guest == sender.value }) == true {
            model.chooseSender(sender); destination = .externalEmails; return
        }
        destination = area == .economics ? .economics : .correspondence(area == .home ? .communications : area)
        if model.selectedSender == sender { Task { await model.select(sender) } }
        else { model.chooseSender(sender) }
    }
    private func navigationColumn(area: MemberArea, preview: Bool = false, dismiss: @escaping () -> Void = {},
                                  focusChanged: @escaping (Bool) -> Void = { _ in }) -> some View {
        EditorialInboxView(conversations: summaries, selection: model.selectedSender, area: area,
            destination: destination, hasRequestWorkflow: model.inbox?.requestProgress != nil, showsStatus: !preview,
            navigate: { target in dismiss(); destination = target },
            select: { sender, origin in dismiss(); openContact(sender, area: origin) },
            reviewSavedRequest: { dismiss(); reviewingRequest = true }, previewFocusChanged: focusChanged) {
                if !preview { status }
            }
            .disabled(model.busy)
    }
    @ViewBuilder private var destinationContent: some View {
        switch destination {
        case .communications, .correspondence: EmptyView()
        case .home:
            EditorialMembraneView(conversations: summaries, selection: Binding(get: { nil }, set: { sender in
                if let sender { openContact(sender, area: .communications) }
            }))
        case .externalEmails:
            ExternalEmailView(model: model)
        case .directory:
            DirectoryView(model: model.directory, member: model,
                openCorrespondence: { sender in openContact(sender, area: .relationships) },
                reviewRequest: { approach, requestClass in
                    requestTarget = RequestTarget(other: SenderReference(approach.address), name: approach.displayName, requestClass: requestClass)
                })
        case .address: AddressEditor(model: model)
        case .questions: EditorialQuestionsView()
        case .relationships:
            overview(title: "Relationship requests", explanation: "Review incoming and outgoing requests, discover people, or edit your published approach page.",
                contacts: summaries.filter { $0.hasReceivedRequests || $0.hasSentRequests }, area: .relationships)
        case .lanes:
            overview(title: "Express lanes", explanation: "These people have incoming express-lane permission to contact you. Scope and validity apply. Your permission to contact them is separate; review both directions in Manage contact.",
                contacts: summaries.filter { $0.route == .bounded }, area: .lanes)
        case .economics:
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    Text("Economics").font(EditorialStyle.heading(28))
                    Text("Request charges and refunds").font(.headline)
                    Text("Select a person in the list to review their recorded request finances. Saved views can be out of date.").font(.callout).foregroundStyle(.secondary)
                    RequestFinancialStatusView(model: model)
                    Divider()
                    Text("Annual service and member distributions").font(.headline)
                    Text("These interfaces are not available yet. No expected disbursement or next-payment forecast is shown.").font(.callout).foregroundStyle(.secondary)
                    Button("Read the Q&A") { destination = .questions }.buttonStyle(EditorialButtonStyle())
                }.padding(32).frame(maxWidth: 720, alignment: .leading).frame(maxWidth: .infinity)
            }.background(EditorialStyle.canvas(scheme))
        case .notes:
            VStack(alignment: .leading, spacing: 18) {
                Text("Notes").font(EditorialStyle.heading(28))
                Text("A private space for writing is planned. Notes cannot be created or saved yet.").foregroundStyle(.secondary)
            }.padding(40).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .background(EditorialStyle.canvas(scheme))
        }
    }
    private func overview(title: String, explanation: String, contacts: [EditorialConversationSummary], area: MemberArea) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                Text(title).font(EditorialStyle.heading(28))
                Text(explanation).font(.callout).foregroundStyle(.secondary)
                if contacts.isEmpty { Text("No matching correspondence in this view.").foregroundStyle(.secondary) }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 18)], spacing: 18) {
                    ForEach(contacts) { contact in
                        Button { openContact(contact.id, area: area) } label: {
                            VStack(alignment: .leading, spacing: 12) {
                                Text(contact.title).font(.headline)
                                Text(contact.awaitingDecision ? "Awaiting your decision" : contact.subtitle).font(.caption).foregroundStyle(.secondary)
                                Text("Open correspondence").font(.caption)
                            }.padding(22).frame(maxWidth: .infinity, minHeight: 130, alignment: .leading)
                                .overlay { EditorialBoundary(route: contact.route).stroke(EditorialStyle.ink(scheme).opacity(0.3)).allowsHitTesting(false) }
                        }.buttonStyle(EditorialPortalButtonStyle(route: contact.route))
                    }
                }
            }.padding(32).frame(maxWidth: 940, alignment: .leading).frame(maxWidth: .infinity)
        }.background(EditorialStyle.canvas(scheme))
    }
    private func correspondence(inbox: Inbox) -> some View {
        EditorialSpaceView(conversations: summaries,
            selection: Binding(get: { model.selectedSender }, set: { sender in
                if let sender { openContact(sender, area: destination.area) } else { model.chooseSender(nil) }
            })) { person, space in
            EditorialPortalView(person: person.title, permission: "Contact from this person: " + person.subtitle,
                route: person.route, portalID: person.id, space: space, returnToSpace: { model.chooseSender(nil) }) {
                if let conversation = model.conversation, conversation.sender == person.id.value {
                    ContactCorrespondenceView(model: model, conversation: conversation, inbox: inbox,
                        manageContact: { managingContact = true },
                        requestRelationship: {
                            Task {
                                await model.directory.open(AddressReference(conversation.sender))
                                destination = .directory
                            }
                        }, reviewPolicy: reviewPolicy,
                        resolve: { message, part in readingReference = true; Task { await model.resolveReference(message: message, part: part) } },
                        decide: { decision, request in confirm(decision, conversation: conversation, request: request) })
                        .id(conversation.sender)
                } else { EditorialEmptyView(title: "Opening correspondence…") }
            }
        }.disabled(model.busy)
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
    @ViewBuilder private var status: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let settings = model.addressSettings {
                Text(settings.displayName).font(.headline)
                if let mailbox = settings.mailbox { Text(mailbox).font(.caption).textSelection(.enabled) }
            }
            Text("Development account\nSimulated bank verification and payments").font(.caption).foregroundStyle(.secondary)
            if let error = model.memberError { Label(error, systemImage: "exclamationmark.circle").font(.callout).foregroundStyle(.orange) }
            if let inbox = model.inbox {
                ForEach(inbox.conversations.filter { $0.controls.laneExpiryReminder != nil }, id: \.sender) { contact in
                    Button("Review expiring lane for \(contact.title)") {
                        destination = .correspondence(.lanes)
                        model.chooseSender(SenderReference(contact.sender))
                        Task { await model.select(SenderReference(contact.sender)) }
                    }.font(.caption)
                }
                if let date = inbox.synchronizedAt {
                    Text("\(model.connected ? "Synchronized" : "Saved snapshot") \(Date(timeIntervalSince1970: Double(date) / 1000).formatted())")
                        .font(.caption).foregroundStyle(.secondary)
                }
                if inbox.requestProgress != nil { Button(inbox.pendingRequest ? "Review saved request workflow" : "View last request outcome") { reviewingRequest = true }.disabled(model.busy) }
                if inbox.pendingPolicy { Text("A conversation-policy change has an unresolved outcome.").font(.caption); Button("Retry original policy change") { Task { await model.retry() } }.disabled(model.busy) }
                if inbox.pendingSend != nil { Text("A message has an unresolved outcome.").font(.caption); Button("Retry original send") { Task { await model.retry() } }.disabled(model.busy) }
                if inbox.pendingSender != nil {
                    Text("Permission operation unresolved. A retry sends the original operation.").font(.callout)
                    Button("Retry original permission operation") { Task { await model.retry() } }.disabled(model.busy)
                }
                if let setup = inbox.pendingSetup {
                    Text(pendingSetupExplanation(setup)).font(.callout)
                    Button("Resume saved operation") { Task { await model.retry() } }.disabled(model.busy)
                }
                if case .refused = inbox.lastSend { Text("The last message was refused. Its original operation is resolved; it will not be delivered later.").font(.caption) }
                if let decision = inbox.lastDecision { Text(decision).font(.caption).foregroundStyle(.secondary) }
            }
            if let notice = model.notice { Text(notice).font(.caption).foregroundStyle(.secondary) }
        }
    }
}

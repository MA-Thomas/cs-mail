import SwiftUI
import CsMailRust

public struct MemberView: View {
    @StateObject private var model = MemberModel()
    @State private var enrolling = false
    @State private var server = "http://127.0.0.1:8480"
    @State private var name = ""
    @State private var birthDate = ""
    @State private var address = ""
    @State private var phone = ""
    @State private var bankAccount = ""
    @State private var confirmation: Decision?
    @Environment(\.scenePhase) private var phase
    public init() {}
    public var body: some View {
        Group {
            if let inbox = model.inbox {
                NavigationSplitView {
                    List(selection: $model.selectedSender) {
                        Section("Conversations") {
                            ForEach(inbox.conversations, id: \.sender) { item in
                                VStack(alignment: .leading, spacing: 5) {
                                    Text(item.title).font(.headline).lineLimit(1)
                                    Text(item.requests.contains(where: \.awaitingDecision) ? "Request awaiting your decision" : item.permission)
                                        .font(.caption).foregroundStyle(.secondary)
                                    if item.external { Label("External email", systemImage: "envelope").font(.caption2) }
                                }.padding(.vertical, 5).tag(item.sender)
                            }
                        }
                    }
                    .overlay { if inbox.conversations.isEmpty { ContentUnavailableView("Your inbox is clear", systemImage: "tray", description: Text("Incoming requests will appear here.")) } }
                    .navigationTitle("cs-mail")
                    .safeAreaInset(edge: .bottom) { status.padding().frame(maxWidth: .infinity, alignment: .leading).background(.bar) }
                } detail: {
                    if let conversation = model.conversation {
                        ScrollView {
                            VStack(alignment: .leading, spacing: 24) {
                                Text(conversation.title).font(.title).textSelection(.enabled)
                                Label(conversation.permission, systemImage: "person.crop.circle.badge.checkmark")
                                    .font(.headline)
                                if conversation.external {
                                    Text("External email is not end-to-end encrypted. Outbound email requires separate disclosure consent.")
                                        .font(.callout).foregroundStyle(.secondary)
                                }
                                ForEach(conversation.requests, id: \.id) { request in
                                    VStack(alignment: .leading, spacing: 10) {
                                        Text(request.className).font(.title3.bold())
                                        Text(request.status).font(.headline)
                                        if request.awaitingDecision, let deadline = request.deadline {
                                            Text("Decision deadline: \(Date(timeIntervalSince1970: Double(deadline) / 1000).formatted())")
                                                .font(.callout).foregroundStyle(.secondary)
                                        }
                                        if request.awaitingDecision {
                                            Text("Accept grants standing permission for future contact. Reject declines this request without blocking later approaches.")
                                            HStack {
                                                Button("Accept", systemImage: "checkmark") { confirmation = .accept }.buttonStyle(.borderedProminent)
                                                Button("Reject", systemImage: "xmark") { confirmation = .reject }.buttonStyle(.bordered)
                                            }.disabled(model.busy || !model.connected || inbox.pendingSender != nil)
                                        }
                                        Text(request.financialStatus).font(.caption).foregroundStyle(.secondary)
                                    }.padding().frame(maxWidth: .infinity, alignment: .leading).background(.quaternary, in: RoundedRectangle(cornerRadius: 12))
                                }
                                ForEach(conversation.messages, id: \.id) { message in
                                    Text(message.text).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                                        .padding().background(.background, in: RoundedRectangle(cornerRadius: 12))
                                }
                            }.padding(28).frame(maxWidth: 800, alignment: .leading).frame(maxWidth: .infinity)
                        }.navigationTitle("Conversation")
                    } else {
                        ContentUnavailableView("Choose a conversation", systemImage: "text.bubble", description: Text("Read a request before deciding who may contact you."))
                    }
                }
                .onChange(of: model.selectedSender) { _, sender in
                    if let sender { Task { await model.select(sender) } }
                }
                .toolbar {
                    ToolbarItemGroup {
                        Button("Refresh", systemImage: "arrow.clockwise") { Task { await model.refresh() } }.disabled(model.busy)
                        Button("Lock", systemImage: "lock") { model.lock() }
                    }
                }
            } else {
                VStack(spacing: 22) {
                    Image(systemName: "envelope.badge.shield.half.filled").font(.system(size: 52)).foregroundStyle(.tint)
                    Text("Your conversations. Your permission.").font(.title2.bold())
                    Text("Open your account on this device, or sign in to enroll a new account.")
                        .multilineTextAlignment(.center).foregroundStyle(.secondary).frame(maxWidth: 410)
                    Button("Open account") { Task { await model.unlock() } }.buttonStyle(.borderedProminent)
                    Button("Sign in and enroll…") { enrolling = true }.buttonStyle(.bordered)
                    Button("Check enrollment status") { Task { await model.resumeEnrollment() } }
                    Text("Credentials stay in Keychain. The local cache is encrypted.\nThis local preview uses a test login provider and simulated bank.")
                        .font(.caption).multilineTextAlignment(.center).foregroundStyle(.secondary)
                    status
                }.padding(32).frame(maxWidth: .infinity, maxHeight: .infinity).disabled(model.busy)
            }
        }
        .overlay(alignment: .top) {
            if model.busy {
                HStack {
                    ProgressView()
                    if model.authenticating { Button("Cancel sign-in") { model.lock() } }
                }.padding(12).background(.regularMaterial, in: Capsule())
            }
        }
        .sheet(isPresented: $enrolling) {
            VStack(alignment: .leading, spacing: 18) {
                Text("Sign in and enroll this device").font(.title2.bold())
                Text("Local development: use fictional applicant details and a simulated bank account. Your browser handles sign-in.")
                    .foregroundStyle(.secondary)
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
        .confirmationDialog(confirmation == .accept ? "Accept this request?" : "Reject this request?", isPresented: Binding(get: { confirmation != nil }, set: { if !$0 { confirmation = nil } }), titleVisibility: .visible) {
            if let decision = confirmation {
                Button(decision == .accept ? "Accept and grant standing permission" : "Reject request") {
                    confirmation = nil
                    Task { await model.decide(decision) }
                }
            }
            Button("Cancel", role: .cancel) { confirmation = nil }
        } message: {
            Text(confirmation == .accept ? "Future contact will be permitted without a request charge. The sender will be owed a full refund; payment confirmation is separate." : "No standing permission is granted. The processing component is retained and collateral goes to the member program.")
        }
        .onChange(of: phase) { _, phase in
            if phase == .background && !model.authenticating { model.lock() }
        }
    }
    @ViewBuilder private var status: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let error = model.lastError { Label(error, systemImage: "exclamationmark.circle").font(.callout).foregroundStyle(.orange) }
            if let inbox = model.inbox {
                if let date = inbox.synchronizedAt {
                    Text("\(model.connected ? "Synchronized" : "Saved snapshot") \(Date(timeIntervalSince1970: Double(date) / 1000).formatted())")
                        .font(.caption).foregroundStyle(.secondary)
                }
                if inbox.pendingSender != nil {
                    Text("Decision outcome unresolved. A retry sends the original operation.").font(.callout)
                    Button("Retry original decision") { Task { await model.retry() } }.disabled(model.busy)
                }
                if let decision = inbox.lastDecision { Text(decision).font(.caption).foregroundStyle(.secondary) }
            }
            if let notice = model.notice { Text(notice).font(.caption).foregroundStyle(.secondary) }
        }
    }
}

import SwiftUI
import UniformTypeIdentifiers
import ImageIO
import CsMailRust
import CsMailPresentation

private struct PublicImage: View {
    let bytes: Data?
    var body: some View {
        if let bytes, let source = CGImageSourceCreateWithData(bytes as CFData, nil),
           let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 160] as CFDictionary) {
            Image(decorative: image, scale: 1).resizable().scaledToFit().frame(width: 64, height: 64).clipShape(RoundedRectangle(cornerRadius: 12))
        }
    }
}

struct DirectoryView: View {
    @ObservedObject var model: DirectoryModel
    let demo: Bool
    @ObservedObject var member: MemberModel
    let openCorrespondence: (SenderReference) -> Void
    let reviewRequest: (Approach, RequestClassID) -> Void
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    if demo { Text("Demo directory · all profiles are fictional.").font(.caption).foregroundStyle(.secondary) }
                    if let approach = model.screen.approach {
                        Button("Back to Directory", systemImage: "chevron.left") {
                            model.returnToResults()
                        }
                        if let error = model.error { Text(error).foregroundStyle(.orange) }
                        ApproachDetail(approach: approach, demo: demo, member: member, openCorrespondence: openCorrespondence, reviewRequest: reviewRequest).id(approach.address)
                    } else {
                        Text("Find someone and see the approaches they welcome.").foregroundStyle(.secondary)
                        HStack {
                            TextField("Name, address or description", text: $model.query).onSubmit { search() }
                            Button("Search") { search() }
                        }
                        HStack {
                            TextField("Exact address or shared request link", text: $model.reference).onSubmit { lookup() }
                            Button("Open") { lookup() }.disabled(model.reference.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        }
                        Text("Unlisted addresses appear only through exact lookup or a shared link.").font(.caption).foregroundStyle(.secondary)
                        if let error = model.error { Text(error).foregroundStyle(.orange) }
                        if let page = model.screen.results.page {
                            if page.entries.isEmpty { Text("No listed addresses match this search.").foregroundStyle(.secondary) }
                            ForEach(page.entries, id: \.address) { entry in
                                Button { Task { await model.open(AddressReference(entry.address)) } } label: {
                                    HStack(alignment: .top, spacing: 16) {
                                        PublicImage(bytes: entry.image)
                                        VStack(alignment: .leading, spacing: 6) {
                                            Text(entry.displayName).font(.headline)
                                            Text(entry.mailbox ?? entry.address).font(.caption).foregroundStyle(.secondary)
                                            Text(entry.description).lineLimit(3)
                                            if !entry.covered { Text("New requests currently unavailable").font(.caption) }
                                        }
                                        Spacer()
                                        Image(systemName: "chevron.right")
                                    }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                                }.buttonStyle(.plain).background(.quaternary, in: RoundedRectangle(cornerRadius: 12))
                            }
                            if page.nextOffset != nil { Button("Next page") { Task { await model.nextPage() } } }
                        }
                    }
                }.padding(24)
            }
            .navigationTitle("Directory")
            .toolbar {
                ToolbarItem {
                    Button("Refresh directory", systemImage: "arrow.clockwise") {
                        Task { await model.refresh() }
                    }.disabled(model.busy)
                    .help(model.screen.approach == nil ? "Refresh directory results" : "Refresh this profile")
                }
            }
            .disabled(model.busy)
            .overlay { if model.busy { ProgressView() } }
        }.frame(maxWidth: .infinity, maxHeight: .infinity)
        .task { await model.loadIfNeeded() }
    }
    private func search() { Task { await model.search() } }
    private func lookup() { Task { await model.openReference() } }
}

private struct ApproachDetail: View {
    let approach: Approach
    let demo: Bool
    @ObservedObject var member: MemberModel
    let openCorrespondence: (SenderReference) -> Void
    let reviewRequest: (Approach, RequestClassID) -> Void
    @State private var selected: RequestClassID?
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack { PublicImage(bytes: approach.image); Text(approach.displayName).font(.title2) }
            Text(approach.description)
            Text("Address: \(approach.mailbox ?? approach.address)").font(.caption).textSelection(.enabled)
            if demo { Text("Fictional profile · example terms only. No quote or payment is created.").font(.caption).foregroundStyle(.secondary) }
            Button("Open correspondence") { openCorrespondence(SenderReference(approach.address)) }
            switch approach.requestEntry {
            case .ownAddress: Text("Your own approach page.").font(.caption)
            case .incomingPending: Text("This person’s request is awaiting your decision. Open correspondence to review it.").font(.callout)
            case .outgoingPending: Text("Your relationship request is pending. Open correspondence to review it.").font(.callout)
            case .alreadyPermitted: Text("You already have permission to start conversations. Open correspondence to compose.").font(.caption)
            case .unavailable: Text("A relationship request is currently unavailable.").font(.caption)
            case .unverified: Text("Refresh this profile to check the current request state.").font(.caption)
            case .reviewApproaches: EmptyView()
            }
            Text("Ways to approach me").font(.headline)
            if approach.classes.isEmpty { Text("Not currently accepting relationship requests.") }
            if !approach.covered { Text("New requests are currently unavailable. Published information remains visible.").foregroundStyle(.secondary) }
            ForEach(approach.classes, id: \.id) { requestClass in
                Button { selected = selected == RequestClassID(requestClass.id) ? nil : RequestClassID(requestClass.id) } label: {
                    HStack {
                        Image(systemName: selected == RequestClassID(requestClass.id) ? "checkmark.circle.fill" : "circle")
                        Text(requestClass.description)
                        Spacer()
                        Text("Collateral \(requestClass.collateralMinorUnits)")
                    }.padding(12)
                }.buttonStyle(.plain).disabled(!approach.covered || (!demo && requestClass.conditionalMinorUnits == nil))
            }
            if demo, selected != nil { Text("Demo class selected. Edited amounts have no quoted financial terms.").font(.caption).foregroundStyle(.secondary) }
            if approach.requestEntry == .reviewApproaches, let selected, let requestClass = approach.classes.first(where: { RequestClassID($0.id) == selected }),
               let processing = approach.processingMinorUnits, let total = requestClass.conditionalMinorUnits {
                VStack(alignment: .leading, spacing: 8) {
                    Text("\(requestClass.description)").font(.headline)
                    Text("Conditional charge: \(total) minor units; processing \(processing), collateral \(requestClass.collateralMinorUnits).")
                    Text("Acceptance owes a full refund. Rejection retains processing and sends collateral into the member pool. Expiry returns collateral; cancellation before submission returns the full charge.").font(.callout)
                    Text("Selecting a class grants no permission. No charge has been authorized.").font(.caption).foregroundStyle(.secondary)
                    Button("Start request · review signed quote") { reviewRequest(approach, selected) }.disabled(demo || member.busy || member.inbox?.pendingRequest == true)
                    if demo { Text("Demo explanation only. No request or charge will be created.").font(.caption) }
                }.padding().background(.quaternary, in: RoundedRectangle(cornerRadius: 12))
            }
        }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
            .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 16))
    }
}

private struct ClassEditorRow: Identifiable {
    var id: RequestClassID
    var description: String
    var collateral: String
}
struct AddressEditor: View {
    @ObservedObject var model: MemberModel
    @State private var loadedRevision: UInt64?
    @State private var classVersion: UInt64 = 0
    @State private var name = ""
    @State private var mailbox = ""
    @State private var description = ""
    @State private var image: Data?
    @State private var listed = true
    @State private var classes: [ClassEditorRow] = []
    @State private var importingImage = false
    @State private var imageError: String?
    var body: some View {
        NavigationStack {
            Form {
                if model.isDemo { Text("Demo editor · changes stay in this session and reset on quit. Nothing is published.").font(.caption) }
                if let settings = model.addressSettings {
                    Section("Address") {
                        if !model.isDemo {
                            TextField("Development address · @csqd-mail.com", text: $mailbox)
                                .disabled(settings.mailbox != nil)
                            Text("Claiming an address is permanent in this development environment. It does not enable external email delivery.").font(.caption)
                        }
                        Text(settings.mailbox ?? settings.address).font(.caption.monospaced()).textSelection(.enabled)
                        ShareLink("Share request link", item: settings.link)
                        Text("This publication belongs to this address. Other addresses have independent classes and visibility.").font(.caption).fixedSize(horizontal: false, vertical: true)
                    }
                    if !model.isDemo {
                        Section("Simulated annual service") {
                            Text("Development offer 1: 12,000 minor units for the fixed 2026 service period. No real payment is taken.").font(.caption)
                            if settings.covered { Text("Simulated service coverage is active.").font(.caption) }
                            else { Button("Activate simulated service") { Task { await model.purchaseDevelopmentService() } } }
                        }
                    }
                    Section("Public profile") {
                        TextField("Display name", text: $name)
                        TextField("Description", text: $description, axis: .vertical).lineLimit(3...8)
                        PublicImage(bytes: image)
                        Button("Choose profile image…") { importingImage = true }
                        if image != nil { Button("Remove image") { image = nil } }
                        if let imageError { Text(imageError).foregroundStyle(.orange) }
                        Text("These details are your public statements, separate from enrollment evidence.").font(.caption).fixedSize(horizontal: false, vertical: true)
                    }
                    Section("Directory visibility") {
                        Toggle("Include this address in directory searches", isOn: $listed)
                        Text(listed ? "Publishing will include this profile in name, address and description searches." : "Anyone with the exact address or shared link can still view your publication. It will be absent from searches and browsing.").font(.caption).fixedSize(horizontal: false, vertical: true)
                    }
                    Section("Ways to approach me · up to eight") {
                        if classes.isEmpty { Text("No paid request route will be offered. You can publish a profile without classes.").font(.caption) }
                        ForEach($classes) { $row in
                            VStack(alignment: .leading) {
                                TextField("Approach description", text: $row.description)
                                TextField("Collateral · minor units", text: $row.collateral)
                                Button("Remove class", role: .destructive) { classes.removeAll { $0.id == row.id } }
                            }
                        }
                        if classes.count < 8 {
                            Button("Add request class") { classes.append(ClassEditorRow(id: RequestClassID(String(UInt64.random(in: 1...UInt64.max))), description: "", collateral: settings.collateralMin.map(String.init) ?? "")) }
                        }
                        if let min = settings.collateralMin, let max = settings.collateralMax,
                           let processing = settings.processingMinorUnits {
                            Text("Collateral must be \(min)–\(max) minor units. CSQD adds \(processing) processing units to each conditional charge. Classes do not grant contact permission.").font(.caption).fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    Section {
                        Button(model.isDemo ? "Save demo changes" : (settings.revision == 0 ? "Publish approach page" : "Save publication")) { save() }
                            .disabled(loadedRevision == nil || name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || classes.contains { $0.description.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || UInt64($0.collateral) == nil })
                        Button("Reload current publication") { Task { await model.loadAddressSettings(); loadDraft() } }
                        Button("Retry interrupted publication") { Task { await model.retry(); await model.loadAddressSettings(); loadDraft() } }
                        if let error = model.addressError { Text(error).foregroundStyle(.orange) }
                        if let notice = model.notice { Text(notice).foregroundStyle(.secondary) }
                    }
                } else {
                    Text("Loading address configuration…")
                    if let error = model.addressError { Text(error).foregroundStyle(.orange) }
                    Button("Reload") { Task { await model.loadAddressSettings(); loadDraft() } }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("My approach page")
            .disabled(model.busy)
            .overlay { if model.busy { ProgressView() } }
        }.frame(maxWidth: .infinity, maxHeight: .infinity)
        .task { await model.loadAddressSettings(); loadDraft() }
        .fileImporter(isPresented: $importingImage, allowedContentTypes: [.image]) { result in
            do {
                let url = try result.get(); let access = url.startAccessingSecurityScopedResource()
                defer { if access { url.stopAccessingSecurityScopedResource() } }
                guard (try url.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? Int.max) <= 8 * 1024 * 1024 else { throw ClientError.InvalidData }
                let bytes = try Data(contentsOf: url)
                guard let source = CGImageSourceCreateWithData(bytes as CFData, nil),
                      let thumbnail = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 512, kCGImageSourceCreateThumbnailWithTransform: true] as CFDictionary) else { throw ClientError.InvalidData }
                let output = NSMutableData()
                guard let destination = CGImageDestinationCreateWithData(output, UTType.png.identifier as CFString, 1, nil) else { throw ClientError.InvalidData }
                CGImageDestinationAddImage(destination, thumbnail, nil)
                guard CGImageDestinationFinalize(destination), output.length <= 262_144 else { throw ClientError.InvalidData }
                image = output as Data; imageError = nil
            } catch { imageError = "This image could not be prepared. Choose a smaller image." }
        }
    }
    private func loadDraft() {
        guard let settings = model.addressSettings else { return }
        loadedRevision = settings.revision; classVersion = settings.classVersion
        mailbox = settings.mailbox ?? ""
        name = settings.displayName; description = settings.description; image = settings.image; listed = settings.listed
        classes = settings.classes.map { ClassEditorRow(id: RequestClassID($0.id), description: $0.description, collateral: String($0.collateralMinorUnits)) }
    }
    private func save() {
        guard let expected = loadedRevision, classVersion < UInt64.max else { return }
        let drafts = classes.compactMap { row -> RequestClassDraft? in
            guard let amount = UInt64(row.collateral) else { return nil }
            return RequestClassDraft(id: row.id.value, description: row.description, collateralMinorUnits: amount)
        }
        guard drafts.count == classes.count else { return }
        Task {
            await model.publishAddress(expected: expected, classVersion: classVersion + 1, name: name, description: description, image: image, listed: listed, classes: drafts, mailbox: mailbox.isEmpty ? nil : mailbox)
            if model.addressError == nil { loadDraft() }
        }
    }
}

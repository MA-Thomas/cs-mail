import SwiftUI
import UniformTypeIdentifiers
import ImageIO
import CsMailRust
import CsMailPresentation

private struct InterfaceBPublicImage: View {
    let bytes: Data?
    var body: some View {
        if let bytes, let source = CGImageSourceCreateWithData(bytes as CFData, nil),
           let image = CGImageSourceCreateThumbnailAtIndex(source, 0, [kCGImageSourceCreateThumbnailFromImageAlways: true, kCGImageSourceThumbnailMaxPixelSize: 160] as CFDictionary) {
            Image(decorative: image, scale: 1).resizable().scaledToFit().frame(width: 64, height: 64).clipShape(RoundedRectangle(cornerRadius: 12))
        }
    }
}

struct InterfaceBDirectoryView: View {
    @FocusState private var searchFocused: Bool
    @ObservedObject var model: DirectoryModel
    @ObservedObject var member: MemberModel
    let openCorrespondence: (SenderReference) -> Void
    let reviewRequest: (Approach, RequestClassID) -> Void
    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    if let approach = model.screen.approach {
                        InterfaceBBackButton("Back to Directory") {
                            model.returnToResults()
                        }
                        if let error = model.error { Text(error).foregroundStyle(.orange) }
                        InterfaceBApproachDetail(approach: approach, member: member, openCorrespondence: openCorrespondence, reviewRequest: reviewRequest).id(approach.alias)
                    } else {
                        Text("Find someone and see the approaches they welcome.").foregroundStyle(.secondary)
                        HStack {
                            TextField("Name, address or shared link", text: $model.query).modifier(SearchFieldInteraction(focus: $searchFocused)).onSubmit { search() }
                            Button("Search") { search() }
                        }
                        if let error = model.error { Text(error).foregroundStyle(.orange) }
                        if let page = model.screen.results.page {
                            if page.entries.isEmpty { Text("No listed addresses match this search.").foregroundStyle(.secondary) }
                            ForEach(page.entries, id: \.alias) { entry in
                                let copyLabel = [entry.displayName, entry.mailbox ?? entry.address, entry.description].joined(separator: "\n")
                                Button { Task { await model.openAlias(AliasReference(entry.alias)) } } label: {
                                    HStack(alignment: .top, spacing: 16) {
                                        InterfaceBPublicImage(bytes: entry.image)
                                        VStack(alignment: .leading, spacing: 6) {
                                            Text(entry.displayName).font(.headline)
                                            Text(entry.mailbox ?? entry.address).font(.caption).foregroundStyle(.secondary)
                                            Text(entry.description).lineLimit(3)
                                            if !entry.covered { Text("New requests currently unavailable").font(.caption) }
                                        }
                                        Spacer()
                                        Image(systemName: "chevron.right")
                                    }.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                                }.buttonStyle(InterfaceBEditorialRegionButtonStyle()).background(.quaternary, in: RoundedRectangle(cornerRadius: 12))
                                    .editorialCopyableLabel(copyLabel)
                            }
                            if page.nextOffset != nil { Button("Next page") { Task { await model.nextPage() } } }
                        }
                    }
                }.padding(24)
            }
            .navigationTitle("Directory")
            .disabled(model.busy)
        }.frame(maxWidth: .infinity, maxHeight: .infinity)
        .task { await model.loadIfNeeded() }
    }
    private func search() { Task { await model.submit() } }
}

private struct InterfaceBApproachDetail: View {
    let approach: Approach
    @ObservedObject var member: MemberModel
    let openCorrespondence: (SenderReference) -> Void
    let reviewRequest: (Approach, RequestClassID) -> Void
    @State private var selected: RequestClassID?
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            InterfaceBPublicImage(bytes: approach.image)
            InterfaceBEditorialReadingView(blocks: [
                EditorialReadingBlock(approach.displayName, style: .title),
                EditorialReadingBlock(approach.description),
                EditorialReadingBlock("Address: \(approach.mailbox ?? approach.address)", style: .caption)
            ])
            Button("Open correspondence") { openCorrespondence(SenderReference(approach.address)) }
            switch approach.requestEntry {
            case .ownAddress: Text("Your own approach page.").font(.caption)
            case .incomingPending: InterfaceBEditorialReadingView("This person’s request is awaiting your decision. Open correspondence to review it.", style: .body)
            case .outgoingPending: InterfaceBEditorialReadingView("Your relationship request is pending. Open correspondence to review it.", style: .body)
            case .alreadyPermitted: InterfaceBEditorialReadingView("You already have permission to start conversations. Open correspondence to compose.", style: .caption)
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
                        Text("\(requestClass.collateralMinorUnits) units collateral")
                    }.padding(12)
                }.buttonStyle(InterfaceBEditorialRegionButtonStyle(selected: selected == RequestClassID(requestClass.id))).disabled(!approach.covered || requestClass.conditionalMinorUnits == nil)
                    .editorialCopyableLabel(requestClass.description + "\nCollateral \(requestClass.collateralMinorUnits)")
            }
            if approach.requestEntry == .reviewApproaches, let selected, let requestClass = approach.classes.first(where: { RequestClassID($0.id) == selected }),
               let processing = approach.processingMinorUnits, let total = requestClass.conditionalMinorUnits {
                VStack(alignment: .leading, spacing: 8) {
                    InterfaceBEditorialReadingView(blocks: [
                        EditorialReadingBlock(requestClass.description, style: .heading),
                        EditorialReadingBlock("Conditional charge: \(total) simulated units. Processing: \(processing); collateral: \(requestClass.collateralMinorUnits)."),
                        EditorialReadingBlock("Acceptance owes a full refund. Rejection retains processing and sends collateral into the member pool. Expiry returns collateral; cancellation before submission returns the full charge."),
                        EditorialReadingBlock("No charge until you review and authorize the request.", style: .caption)
                    ])
                    Button("Review request") { reviewRequest(approach, selected) }.disabled(member.busy || member.inbox?.pendingRequest == true)
                }.padding().background(.quaternary, in: RoundedRectangle(cornerRadius: 12))
            }
        }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
            .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 16))
    }
}

struct InterfaceBAddressEditor: View {
    @ObservedObject var model: MemberModel
    @ObservedObject var draft: ApproachEditorDraft
    @State private var importingImage = false
    @State private var imageError: String?
    private var pendingChanges: Bool {
        model.addressSettings?.pending == true || model.inbox?.pendingSetup == .directoryPublication
    }
    var body: some View {
        NavigationStack {
            Form {
                if let settings = model.addressSettings, settings.alias == model.editingAlias {
                    Section("Address") {
                        TextField("Address", text: $draft.mailbox)
                            .disabled(settings.mailbox != nil)
                        if settings.mailbox == nil { Text("This address will remain reserved to your account.").font(.caption) }
                        ShareLink("Share request link", item: settings.link)
                    }
                    Section("Simulated annual service") {
                        InterfaceBEditorialReadingView("12,000 units for 2026. Simulated payments; no real money is taken.", style: .caption)
                        if settings.covered { Text("Simulated service coverage is active.").font(.caption) }
                        else { Button("Activate simulated service") { Task { await model.purchaseDevelopmentService() } } }
                    }
                    Section("Public profile") {
                        TextField("Display name", text: $draft.name)
                        TextField("Description", text: $draft.description, axis: .vertical).lineLimit(3...8)
                        InterfaceBPublicImage(bytes: draft.image)
                        Button("Choose profile image…") { importingImage = true }
                        if draft.image != nil { Button("Remove image") { draft.image = nil } }
                        if let imageError { Text(imageError).foregroundStyle(.orange) }
                    }
                    Section("Directory visibility") {
                        Picker("Directory visibility", selection: Binding<Bool?>(get: { draft.visibilityChosen ? draft.listed : nil }, set: { value in if let value { draft.listed = value; draft.visibilityChosen = true } })) {
                            Text("Choose visibility").tag(Optional<Bool>.none)
                            Text("Listed in directory searches").tag(Optional(true))
                            Text("Unlisted · exact address or link only").tag(Optional(false))
                        }
                        Text(draft.listed ? "Publishing will include this profile in name, address and description searches." : "Anyone with the exact address or shared link can still view your publication. It will be absent from searches and browsing.").font(.caption).fixedSize(horizontal: false, vertical: true)
                    }
                    Section("Ways to approach me") {
                        if draft.classes.isEmpty { InterfaceBEditorialReadingView("You are not offering paid requests.", style: .caption) }
                        ForEach($draft.classes) { $row in
                            VStack(alignment: .leading) {
                                TextField("Approach description", text: $row.description)
                                TextField("Collateral · simulated units", text: $row.collateral)
                                if let published = settings.classes.first(where: { $0.id == row.id.value }), !published.quotable {
                                    Text("This published approach cannot currently receive requests. Update its collateral and save to offer it again.").font(.caption).foregroundStyle(.orange)
                                }
                                if let problem = draft.collateralProblem(row) { Text(problem).font(.caption).foregroundStyle(.orange) }
                                Button("Remove approach", role: .destructive) { draft.classes.removeAll { $0.id == row.id } }
                            }
                        }
                        if draft.classes.count < 8 {
                            Button("Add approach") { draft.classes.append(ClassEditorRow(id: RequestClassID(String(UInt64.random(in: 1...UInt64.max))), description: "", collateral: settings.collateralMin.map(String.init) ?? "")) }
                        }
                        if let min = settings.collateralMin, let max = settings.collateralMax,
                           let processing = settings.processingMinorUnits {
                            Text("Collateral: \(min)–\(max) simulated units. Processing fee: \(processing) units per request.").font(.caption).fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    Section {
                        if draft.isDirty { Text("Unsaved changes").font(.caption).foregroundStyle(.orange) }
                        Button(settings.revision == 0 ? "Publish approach page" : "Save changes") { save() }
                            .disabled(pendingChanges || !draft.valid || draft.loadedRevision != settings.revision)
                        if let loadedRevision = draft.loadedRevision, loadedRevision != settings.revision {
                            Text("Your page has changed since you began editing. Your edits are still here.").foregroundStyle(.orange)
                            Button("Discard edits and load saved page") { loadDraft() }
                        }
                        if pendingChanges {
                            Text("Your last change has not been confirmed.").foregroundStyle(.orange)
                            Button("Check saved changes") { Task { await model.retry(); await model.loadAddressSettings(); if !draft.isDirty { loadDraft() } } }
                        }
                        if let error = model.addressError { Text(error).foregroundStyle(.orange) }
                    }
                } else {
                    if model.addressError == nil { Text("Loading your page…").font(.caption).foregroundStyle(.secondary) }
                    if let error = model.addressError { Text(error).foregroundStyle(.orange) }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("My approach page")
            .disabled(model.busy)
        }.frame(maxWidth: .infinity, maxHeight: .infinity)
        .task(id: model.editingAlias) { if model.editingAlias == nil { await model.loadManagedAddresses() }; await model.loadAddressSettings(); if draft.loadedRevision == nil { loadDraft() } }
        .onChange(of: model.addressSettings?.revision) { _, _ in
            if draft.loadedRevision == nil { loadDraft() }
        }
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
                draft.image = output as Data; imageError = nil
            } catch { imageError = "This image could not be prepared. Choose a smaller image." }
        }
    }
    private func loadDraft() { if let settings = model.addressSettings { draft.load(settings) } }
    private func save() { Task { _ = await draft.save(using: model) } }
}

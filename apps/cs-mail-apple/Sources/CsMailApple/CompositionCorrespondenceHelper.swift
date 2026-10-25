import SwiftUI
import CsMailRust
import CsMailPresentation

/// Reading beside a draft conveys no quotation, export, or disclosure authority.
struct CompositionCorrespondenceHelper: View {
    @ObservedObject var model: MemberModel
    @ObservedObject var workspace: WorkspaceComposition
    @State private var query = ""
    @State private var message: Message?
    @State private var externalMessage: ExternalMessage?
    @FocusState private var searching: Bool
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Correspondence").font(.headline)
            if let message {
                Button("Back to messages") {self.message = nil}.buttonStyle(ComposerActionStyle())
                Text(workspace.sourceTitle).font(.caption).foregroundStyle(.secondary)
                ScrollView {
                    InterfaceNeutralDocument(content: message.content)
                }
            } else if let externalMessage {
                Button("Back to messages") {self.externalMessage = nil}.buttonStyle(ComposerActionStyle())
                ScrollView {InterfaceNeutralDocument(content: externalMessage.content)}
            } else if let sender = workspace.sourceContactID {
                Button("Back to people") {workspace.sourceContactID = nil; model.clearSourceContact()}.buttonStyle(ComposerActionStyle())
                if let contact = model.sourceConversation, contact.sender == sender {
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 0) {
                            ForEach(contact.messages, id: \.id) {item in
                                Button {message = item; workspace.sourceTitle = contact.title} label: {
                                    Text(item.text).lineLimit(4).frame(maxWidth: .infinity, minHeight: 48, alignment: .leading).padding(.vertical,8).contentShape(Rectangle())
                                }.buttonStyle(.plain)
                                Divider()
                            }
                            ForEach(model.externalInbox?.threads.filter {$0.guest == sender} ?? [], id: \.id) {thread in
                                ForEach(thread.messages, id: \.id) {item in
                                    Button {externalMessage = item} label: {Text(item.text).lineLimit(4).frame(maxWidth: .infinity, minHeight: 48, alignment: .leading).padding(.vertical,8).contentShape(Rectangle())}.buttonStyle(.plain)
                                    Divider()
                                }
                            }
                        }
                    }
                }
            } else {
                TextField("Find a person", text: $query).textFieldStyle(.plain).modifier(SearchFieldInteraction(focus: $searching))
                Divider()
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(model.presentationContacts.filter {query.isEmpty || $0.title.localizedCaseInsensitiveContains(query)}, id: \.sender) {person in
                            Button {workspace.sourceContactID = person.sender; Task {await model.openSourceContact(person.sender)}} label: {
                                Text(person.title).frame(maxWidth: .infinity,minHeight:44,alignment:.leading).contentShape(Rectangle())
                            }.buttonStyle(.plain)
                            Divider()
                        }
                    }
                }
            }
        }.padding(20)
    }
}
private struct InterfaceNeutralDocument: View {
    let content: ReadingContent
    var body: some View {
        switch content {
        case .readable(let parts):
            VStack(alignment: .leading, spacing: 16) {
                ForEach(Array(parts.enumerated()), id: \.offset) {_, part in
                    switch part {
                    case .email(let email): EmailReader(email: email, allowsPictureRetrieval: false, loadPictures: {_ in nil})
                    case .text(let text): Text(verbatim: text).textSelection(.enabled)
                    case .image(let bytes): SourceHelperPicture(bytes: bytes)
                    case .quotation(_,let blocks), .sharedCopy(_,let blocks):
                        ForEach(Array(blocks.enumerated()), id: \.offset) {_,block in
                            switch block {case .text(let text): Text(verbatim:text).textSelection(.enabled); case .image(let bytes): SourceHelperPicture(bytes:bytes)}
                        }
                    case .reference: Text("Reference").font(.caption).foregroundStyle(.secondary)
                    }
                }
            }
        default: Text("This message is unavailable.").foregroundStyle(.secondary)
        }
    }
}
private struct SourceHelperPicture: View {
    let bytes: Data
    var body: some View {
        #if os(macOS)
        if let image = NSImage(data: bytes) {Image(nsImage:image).resizable().scaledToFit()}
        #else
        if let image = UIImage(data: bytes) {Image(uiImage:image).resizable().scaledToFit()}
        #endif
    }
}

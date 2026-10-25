import SwiftUI
import CsMailRust

/// Presentation of Rust-owned revisions. Local calls bypass the network worker queue.

struct DraftSaveStatus: View {
    @ObservedObject var draft: DraftEditor
    var body: some View {
        if let error = draft.error {
            HStack { Text(error).font(.caption).foregroundStyle(.orange); Button("Retry saving") { Task { await draft.retrySaving() } } }
        }
        else if let record = draft.record {
            ForEach(Array(record.issues.enumerated()), id: \.offset) { _, issue in
                switch issue {
                case .subjectTooLong(let maximum): Text("Keep the subject within \(maximum) bytes. Your draft is still saved.").font(.caption).foregroundStyle(.orange)
                case .emptyMessage: EmptyView()
                case .invalidMailbox: if !draft.mailbox.isEmpty { Text("Enter a valid email address before sending.").font(.caption).foregroundStyle(.orange) }
                case .textTooLong(let maximum): Text("This message exceeds the \(maximum)-byte sending limit. Your draft is still saved.").font(.caption).foregroundStyle(.orange)
                }
            }
        }
    }
}

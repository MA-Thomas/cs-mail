import Foundation
import CsMailRust

/// Transient visual-review fixtures, never account state, authority, or payment evidence.
/// This adapter has no device store, credential vault, transport, or client.
@MainActor final class DemoSession {
    private(set) var settings = AddressSettings(covered: true, address: "alice@demo.example", mailbox: nil, link: "https://demo.example/r/alice",
        revision: 1, classVersion: 1, displayName: "Alice", description: "Thoughtful introductions, research and creative collaboration.",
        image: nil, listed: true, classes: [
            DirectoryClass(id: "1", description: "An introduction", collateralMinorUnits: 1500, conditionalMinorUnits: 1550, quotable: true),
            DirectoryClass(id: "2", description: "Research collaboration", collateralMinorUnits: 2000, conditionalMinorUnits: 2050, quotable: true),
            DirectoryClass(id: "3", description: "A creative project", collateralMinorUnits: 500, conditionalMinorUnits: 550, quotable: true)
        ], pending: false, processingMinorUnits: 50, collateralMin: 100, collateralMax: 2500)
    private let people: [Conversation] = [
        DemoSession.person("maya", "Maya Chen", "No standing permission", .closed, pending: true,
            text: "Hello,\n\nI came across your work on mindful communication and wanted to introduce myself. I’m exploring how small changes to our digital environments can give us more room to think.\n\nI’d value a conversation about making attention a choice. There’s no need for a quick reply.\n\nWith appreciation,\nMaya"),
        DemoSession.person("studio", "hello@fieldstudio.example", "No standing permission", .closed, pending: true, external: true,
            text: "Hello,\n\nWe’re putting together a series about spaces for thoughtful work. Your perspective on communication would be a lovely addition.\n\nWould you be open to a conversation?\n\nWarmly,\nField Studio"),
        DemoSession.person("daniel", "Daniel Reed", "Standing permission", .standing,
            text: "I’ve been thinking about our conversation yesterday. There’s something valuable in making room for a reply.\n\nLet’s pick this up when you have time.\n\nDaniel"),
        DemoSession.person("elena", "Elena Park", "Express lane — scope and validity apply", .bounded,
            text: "Thank you for making a little room for this project. I’ve sent the revised outline so we can return to it when we next speak.\n\nElena"),
        DemoSession.person("jules", "Jules Morgan", "Rejected", .closed,
            text: "This fictional approach was declined. This is a visual example, not a settlement record."),
        DemoSession.person("noah", "Noah Ellis", "Permission revoked", .closed,
            text: "This fictional historical message remains visible after the route for future contact was closed.")
    ]
    private static func person(_ id: String, _ name: String, _ permission: String, _ route: PermissionRoute,
                               pending: Bool = false, external: Bool = false, text: String) -> Conversation {
        Conversation(sentRequests: [], controls: RelationshipControls(state: route == .standing ? .standing : (route == .bounded ? .lane : .closed), revocable: route != .closed, followupMax: 0, followupPerInterval: 0, followupIntervalMs: 0, lane: nil, laneExpiryReminder: nil), threads: [], reply: .closed, requestEntry: pending ? .incomingPending(request: id + "-request") : .reviewApproaches, sender: id, title: name, permission: permission, permissionRoute: route, external: external,
            requests: pending ? [Request(id: id + "-request", capture: .finalized, className: external ? "A collaboration" : "An introduction",
                status: "Awaiting your decision", deadline: nil, awaitingDecision: true,
                financialStatus: "Fictional example · no charge or refund recorded", hasPoolForfeiture: false)] : [],
            messages: [Message(conversation: nil, outgoing: false, id: id + "-message", text: text, references: [])])
    }
    var inbox: Inbox {
        Inbox(lastSend: nil, pendingRequest: false, requestProgress: nil, pendingPolicy: false, pendingSend: nil, account: "demo", persona: settings.address, synchronizedAt: nil,
            conversations: people, pendingSender: nil, pendingSetup: nil, lastDecision: nil)
    }
    func conversation(_ sender: String) -> Conversation? { people.first { $0.sender == sender } }
    private var ownApproach: Approach {
        Approach(requestEntry: .ownAddress, address: settings.address, mailbox: nil, displayName: settings.displayName, description: settings.description,
            image: settings.image, covered: true, processingMinorUnits: settings.processingMinorUnits, classes: settings.classes)
    }
    private var directory: [Approach] {
        [ownApproach,
         Approach(requestEntry: .reviewApproaches, address: "maya@demo.example", mailbox: nil, displayName: "Maya Chen", description: "Research on attention and digital environments.", image: nil, covered: true, processingMinorUnits: 50, classes: [DirectoryClass(id: "introduction", description: "A thoughtful introduction", collateralMinorUnits: 500, conditionalMinorUnits: 550, quotable: true)]),
         Approach(requestEntry: .reviewApproaches, address: "daniel@demo.example", mailbox: nil, displayName: "Daniel Reed", description: "Writing, ideas and thoughtful introductions.", image: nil, covered: true, processingMinorUnits: 50, classes: []),
         Approach(requestEntry: .reviewApproaches, address: "elena@demo.example", mailbox: nil, displayName: "Elena Park", description: "Creative projects and collaboration.", image: nil, covered: true, processingMinorUnits: 50, classes: [DirectoryClass(id: "project", description: "Discuss a project", collateralMinorUnits: 500, conditionalMinorUnits: 550, quotable: true)]),
         Approach(requestEntry: .reviewApproaches, address: "noah@demo.example", mailbox: nil, displayName: "Noah Ellis", description: "A sample profile with requests currently unavailable.", image: nil, covered: false, processingMinorUnits: 50, classes: [])]
    }
    func search(_ text: String, offset: UInt32) -> DirectoryPage {
        let query = text.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        let entries = directory.filter { entry in
            (entry.address != settings.address || settings.listed) &&
                (query.isEmpty || [entry.displayName, entry.address, entry.description].contains { $0.lowercased().contains(query) })
        }.sorted { $0.displayName < $1.displayName }
        return DirectoryPage(entries: Array(entries.dropFirst(Int(offset)).prefix(20)), nextOffset: nil)
    }
    func lookup(_ reference: String) -> Approach? {
        let reference = reference.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return directory.first { $0.address == reference || "https://demo.example/r/" + $0.address.components(separatedBy: "@")[0] == reference }
    }
    func edit(name: String, description: String, image: Data?, listed: Bool, classes: [RequestClassDraft]) {
        // Keep only unchanged illustrative terms. Edited amounts never manufacture a quote.
        let previous = settings.classes
        settings = AddressSettings(covered: true, address: settings.address, mailbox: nil, link: settings.link, revision: settings.revision + 1,
            classVersion: settings.classVersion + 1, displayName: name, description: description, image: image, listed: listed,
            classes: classes.map { draft in
                let example = previous.first { $0.id == draft.id && $0.collateralMinorUnits == draft.collateralMinorUnits }
                return DirectoryClass(id: draft.id, description: draft.description, collateralMinorUnits: draft.collateralMinorUnits,
                    conditionalMinorUnits: example?.conditionalMinorUnits, quotable: false)
            }, pending: false, processingMinorUnits: 50, collateralMin: 100, collateralMax: 2500)
    }
}

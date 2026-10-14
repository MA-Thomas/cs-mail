import SwiftUI

private enum NavigationHitRegion: Hashable, Sendable { case icon(MemberArea), popup }
private enum NavigationCoordinates: Hashable, Sendable { case shell }
private struct NavigationHitRegions: PreferenceKey {
    static let defaultValue: [NavigationHitRegion: CGRect] = [:]
    static func reduce(value: inout [NavigationHitRegion: CGRect], nextValue: () -> [NavigationHitRegion: CGRect]) {
        value.merge(nextValue(), uniquingKeysWith: { _, next in next })
    }
}

/// Column A previews reuse Column B. Hover and focus are presentation state only.
public struct EditorialNavigationShell<Content: View, Preview: View>: View {
    let selected: MemberArea
    let navigate: (MemberArea) -> Void
    let content: Content
    let preview: (MemberArea, @escaping () -> Void, @escaping (Bool) -> Void) -> Preview
    private enum PreviewSession: Equatable {
        case pointer(MemberArea), keyboard(MemberArea)
        var area: MemberArea {
            switch self {
            case .pointer(let area), .keyboard(let area): return area
            }
        }
    }
    @State private var previewSession: PreviewSession?
    @State private var hoveredIcon: MemberArea?
    @State private var popupHovered = false
    @State private var popupFocused = false
    @State private var hitRegions: [NavigationHitRegion: CGRect] = [:]
    @State private var pointerLocation: CGPoint?
    @FocusState private var focusedIcon: MemberArea?
    @FocusState private var closeFocused: Bool
    @Environment(\.colorScheme) private var scheme
    private struct Activity: Equatable {
        let icon: MemberArea?
        let inside: Bool
        let session: PreviewSession?
    }
    public init(selected: MemberArea, navigate: @escaping (MemberArea) -> Void,
                @ViewBuilder preview: @escaping (MemberArea, @escaping () -> Void, @escaping (Bool) -> Void) -> Preview,
                @ViewBuilder content: () -> Content) {
        self.selected = selected; self.navigate = navigate; self.preview = preview; self.content = content()
    }
    public var body: some View {
        GeometryReader { geometry in
            let compact = geometry.size.width < 760
            let layout = compact ? AnyLayout(VStackLayout(spacing: 0)) : AnyLayout(HStackLayout(spacing: 0))
            layout {
                rail(horizontal: compact)
                Rectangle().fill(EditorialStyle.ink(scheme).opacity(0.15))
                    .frame(width: compact ? nil : 1, height: compact ? 1 : nil)
                content
            }
            .overlay(alignment: .topLeading) {
                if let area = previewSession?.area {
                    VStack(spacing: 0) {
                        HStack {
                            Text("Preview · \(area.title)").font(.caption)
                            Spacer()
                            Button("Close", systemImage: "xmark", action: dismissPreview)
                                .labelStyle(.iconOnly).focusable().focused($closeFocused)
                                .keyboardShortcut(.cancelAction)
                        }.padding(14)
                        Divider()
                        preview(area, dismissPreview) { popupFocused = $0 }
                    }
                    .frame(width: min(320, geometry.size.width - (compact ? 24 : 104)), height: min(area == .home || area == .notes ? 120 : 560, geometry.size.height - (compact ? 96 : 32)))
                    .background(EditorialStyle.canvas(scheme), in: RoundedRectangle(cornerRadius: 14))
                    .clipShape(RoundedRectangle(cornerRadius: 14))
                    .overlay { RoundedRectangle(cornerRadius: 14).strokeBorder(EditorialStyle.ink(scheme).opacity(0.2)).allowsHitTesting(false) }
                    .shadow(color: .black.opacity(0.15), radius: 18, y: 6)
                    .contentShape(Rectangle())
                    .background { hitRegion(.popup) }
                    .accessibilityElement(children: .contain)
                    .accessibilityLabel("\(area.title) navigation preview")
                    .padding(.leading, compact ? 12 : 94).padding(.top, compact ? 82 : 16)
                }
            }
            .onChange(of: focusedIcon) { _, area in
                if case .keyboard = previewSession, let area { previewSession = .keyboard(area) }
            }
            .task(id: Activity(icon: hoveredIcon, inside: keepsPreviewOpen, session: previewSession)) {
                if let area = hoveredIcon {
                    do { try await Task.sleep(for: .milliseconds(previewSession == nil ? 250 : 0)) }
                    catch { return }
                    guard !Task.isCancelled else { return }
                    if previewSession?.area != area { popupHovered = false; popupFocused = false }
                    previewSession = .pointer(area)
                } else if previewSession != nil && !keepsPreviewOpen {
                    do { try await Task.sleep(for: .milliseconds(220)) }
                    catch { return }
                    guard !Task.isCancelled else { return }
                    dismissPreview()
                }
            }
        }
        .coordinateSpace(name: NavigationCoordinates.shell)
        .onPreferenceChange(NavigationHitRegions.self) { regions in
            hitRegions = regions
            updatePointer(pointerLocation)
        }
        .onContinuousHover { phase in
            switch phase {
            case .active(let location): updatePointer(location)
            case .ended: updatePointer(nil)
            }
        }
        #if os(macOS)
        .onExitCommand(perform: dismissPreview)
        #endif
    }
    @ViewBuilder private func rail(horizontal: Bool) -> some View {
        if horizontal {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 4) { ForEach(MemberArea.allCases) { item($0) } }.padding(6)
            }.frame(height: 76)
        } else {
            VStack(spacing: 6) {
                Text("cs").font(EditorialStyle.heading(24)).padding(.vertical, 18).accessibilityLabel("cs-mail")
                ForEach(MemberArea.allCases) { item($0) }
                Spacer(minLength: 0)
            }.padding(.horizontal, 6).frame(width: 86)
        }
    }
    private func item(_ area: MemberArea) -> some View {
        Button {
            dismissPreview()
            navigate(area)
        } label: {
            VStack(spacing: 7) {
                Image(systemName: area.icon).font(.system(size: 20)).accessibilityHidden(true)
                Text(area == .communications ? "Comms" : area.title).font(.system(size: 10)).multilineTextAlignment(.center).lineLimit(2)
            }.frame(width: 74, height: 64)
        }
        .buttonStyle(EditorialRegionButtonStyle(selected: selected == area))
        .focusable()
        .focused($focusedIcon, equals: area)
        .onKeyPress(.rightArrow) { openKeyboardPreview(area); return .handled }
        .background { hitRegion(.icon(area)) }
        .accessibilityLabel(area.title)
        .accessibilityAddTraits(selected == area ? .isSelected : [])
        .accessibilityAction(named: "Preview navigation") { openKeyboardPreview(area) }
        .help("Open \(area.title). Right Arrow previews navigation; Escape closes the preview.")
    }
    private var keepsPreviewOpen: Bool {
        switch previewSession {
        case .pointer: return popupHovered
        case .keyboard(let area): return popupHovered || popupFocused || closeFocused || focusedIcon == area
        case nil: return false
        }
    }
    private func openKeyboardPreview(_ area: MemberArea) {
        updatePointer(nil)
        previewSession = .keyboard(area)
        focusedIcon = area
    }
    private func dismissPreview() {
        previewSession = nil; updatePointer(nil); popupFocused = false
        closeFocused = false; focusedIcon = nil
    }
    private func hitRegion(_ target: NavigationHitRegion) -> some View {
        GeometryReader { geometry in
            Color.clear.preference(key: NavigationHitRegions.self,
                value: [target: geometry.frame(in: .named(NavigationCoordinates.shell))])
        }.allowsHitTesting(false)
    }
    /// One pointer observation chooses from actual visible bounds in one coordinate space.
    private func updatePointer(_ location: CGPoint?) {
        pointerLocation = location
        hoveredIcon = location.flatMap { point in
            MemberArea.allCases.first { hitRegions[.icon($0)]?.contains(point) == true }
        }
        popupHovered = location.map { hitRegions[.popup]?.contains($0) == true } ?? false
    }
}

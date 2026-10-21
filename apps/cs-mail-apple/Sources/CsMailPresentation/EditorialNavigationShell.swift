import SwiftUI

private enum NavigationHitRegion: Hashable, Sendable { case icon(MemberArea), popup }
private enum NavigationCoordinates: Hashable, Sendable { case shell }
private struct NavigationHitRegions: PreferenceKey {
    static let defaultValue: [NavigationHitRegion: CGRect] = [:]
    static func reduce(value: inout [NavigationHitRegion: CGRect], nextValue: () -> [NavigationHitRegion: CGRect]) {
        value.merge(nextValue(), uniquingKeysWith: { _, next in next })
    }
}

/// Column A offers workspace layout controls; column B owns the current organizer.
public struct EditorialNavigationShell<Content: View, Preview: View>: View {
    let selected: MemberArea
    let columnBVisible: Bool
    let navigate: (MemberArea) -> Void
    let content: Content
    let preview: (MemberArea, @escaping () -> Void, @escaping (Bool) -> Void) -> Preview
    private let railWidth: CGFloat = 64.5
    private let railInset: CGFloat = 4
    // Keep the reduced gap while clearing the macOS window controls above the rail.
    private let columnGap: CGFloat = 10.75
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
    @Environment(\.colorScheme) private var scheme
    private struct Activity: Equatable {
        let icon: MemberArea?
        let inside: Bool
        let session: PreviewSession?
    }
    public init(selected: MemberArea, columnBVisible: Bool, navigate: @escaping (MemberArea) -> Void,
                @ViewBuilder preview: @escaping (MemberArea, @escaping () -> Void, @escaping (Bool) -> Void) -> Preview,
                @ViewBuilder content: () -> Content) {
        self.selected = selected; self.columnBVisible = columnBVisible
        self.navigate = navigate; self.preview = preview; self.content = content()
    }
    public var body: some View {
        GeometryReader { geometry in
            let compact = geometry.size.width < 760
            let layout = compact ? AnyLayout(VStackLayout(spacing: 0)) : AnyLayout(HStackLayout(spacing: 0))
            layout {
                rail(horizontal: compact)
                    .background(EditorialStyle.canvas(scheme))
                Rectangle().fill(EditorialStyle.ink(scheme).opacity(0.15))
                    .frame(width: compact ? nil : 1, height: compact ? 1 : nil)
                    .padding(.bottom, compact ? 0 : geometry.safeAreaInsets.top)
                if !compact {
                    Color.clear.frame(width: columnGap).allowsHitTesting(false).accessibilityHidden(true)
                }
                content.simultaneousGesture(TapGesture().onEnded { dismissPreview() })
            }
            .overlay(alignment: .topLeading) {
                if let area = previewSession?.area {
                    VStack(spacing: 0) {
                        HStack {
                            Text("\(area.title) layout").font(.caption)
                            Spacer()
                        }.padding(.horizontal, 14).padding(.vertical, 16)
                        Divider()
                        preview(area, dismissPreview) { popupFocused = $0 }
                    }
                    .frame(width: min(320, geometry.size.width - (compact ? 24 : railWidth + columnGap + 18)), height: min(300, geometry.size.height - (compact ? 96 : 32)))
                    .foregroundStyle(EditorialStyle.ink(previewScheme))
                    .tint(EditorialStyle.ink(previewScheme))
                    .environment(\.colorScheme, previewScheme)
                    .background { EditorialGlassSurface(previewArea: area).environment(\.colorScheme, scheme) }
                    .clipShape(RoundedRectangle(cornerRadius: 14))
                    .overlay {
                        if columnBVisible {
                            RoundedRectangle(cornerRadius: 14).strokeBorder(EditorialStyle.ink(scheme).opacity(0.2))
                                .allowsHitTesting(false)
                        } else {
                            GeometryReader { geometry in
                                EditorialCoveLight(area: area, bounds: CGRect(origin: .zero, size: geometry.size), cornerRadius: 14)
                            }
                            .allowsHitTesting(false).accessibilityHidden(true)
                        }
                    }
                    .shadow(color: .black.opacity(columnBVisible ? 0.15 : 0), radius: 18, y: 6)
                    .contentShape(Rectangle())
                    .background { hitRegion(.popup) }
                    .accessibilityElement(children: .contain)
                    .accessibilityLabel("\(area.title) navigation preview")
                    .padding(.leading, compact ? 12 : railWidth + columnGap + 8).padding(.top, compact ? 82 : 16)
                }
            }
            .onChange(of: focusedIcon) { _, area in
                if case .keyboard = previewSession, let area { previewSession = .keyboard(area) }
            }
            .task(id: Activity(icon: hoveredIcon, inside: keepsPreviewOpen, session: previewSession)) {
                if let area = hoveredIcon {
                    do { try await Task.sleep(for: .milliseconds(previewSession == nil ? 750 : 0)) }
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
    private var previewScheme: ColorScheme { scheme == .dark ? .light : .dark }
    @ViewBuilder private func rail(horizontal: Bool) -> some View {
        if horizontal {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 4) { ForEach(MemberArea.allCases) { item($0) } }.padding(6)
            }.frame(height: 76)
        } else {
            VStack(spacing: 6) {
                ForEach(MemberArea.allCases) { item($0, width: railWidth - 2 * railInset) }
                Spacer(minLength: 0)
            }.padding(.horizontal, railInset).frame(width: railWidth)
        }
    }
    private func item(_ area: MemberArea, width: CGFloat = 74) -> some View {
        Button {
            if selected != area { navigate(area) }
            previewSession = hoveredIcon == area ? .pointer(area) : nil
            focusedIcon = area
        } label: {
            VStack(spacing: 7) {
                HStack(spacing: 0) {
                    Image(systemName: area.icon)
                    if area == .lanes { Image(systemName: area.icon) }
                }.font(.system(size: 20)).accessibilityHidden(true)
                Text(area == .communications ? "Comms" : area.title).font(.system(size: 10))
                    .multilineTextAlignment(.center).lineLimit(area == .relationships ? 1 : 2).minimumScaleFactor(0.8)
            }.frame(width: width, height: 64)
        }
        .buttonStyle(EditorialRegionButtonStyle(selected: selected == area))
        .focusable()
        .focused($focusedIcon, equals: area)
        .onKeyPress(.rightArrow) { openKeyboardPreview(area); return .handled }
        .background { hitRegion(.icon(area)) }
        .accessibilityLabel(area.title)
        .accessibilityAddTraits(selected == area ? .isSelected : [])
        .accessibilityAction(named: "Preview navigation") { openKeyboardPreview(area) }
        .help("Open \(area.title). Hover or press Right Arrow for layout controls; Escape closes them.")
    }
    private var keepsPreviewOpen: Bool {
        switch previewSession {
        case .pointer: return popupHovered
        case .keyboard(let area): return popupHovered || popupFocused || focusedIcon == area
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
        focusedIcon = nil
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

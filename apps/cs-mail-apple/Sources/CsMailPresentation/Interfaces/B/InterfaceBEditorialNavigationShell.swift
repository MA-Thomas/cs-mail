import SwiftUI

private enum InterfaceBNavigationHitRegion: Hashable, Sendable { case icon(MemberArea), popup }
private enum InterfaceBNavigationCoordinates: Hashable, Sendable { case shell }
private struct InterfaceBNavigationHitRegions: PreferenceKey {
    static let defaultValue: [InterfaceBNavigationHitRegion: CGRect] = [:]
    static func reduce(value: inout [InterfaceBNavigationHitRegion: CGRect], nextValue: () -> [InterfaceBNavigationHitRegion: CGRect]) {
        value.merge(nextValue(), uniquingKeysWith: { _, next in next })
    }
}

/// Column A offers workspace layout controls; column B owns the current organizer.
public struct InterfaceBEditorialNavigationShell<Content: View, Preview: View>: View {
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
    @State private var hitRegions: [InterfaceBNavigationHitRegion: CGRect] = [:]
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
                    .background { InterfaceBWorkspaceBackdrop() }
                Rectangle().fill(InterfaceBEditorialStyle.ink(scheme).opacity(0.15))
                    .frame(width: compact ? nil : 1, height: compact ? 1 : nil)
                    .padding(.top, compact ? 0 : InterfaceBWorkspaceLayout.verticalInset)
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
                            Text(area.title)
                                .font(InterfaceBEditorialStyle.heading(23))
                                .accessibilityAddTraits(.isHeader)
                            Spacer()
                        }.padding(.horizontal, 14).padding(.vertical, 16)
                        Divider()
                        preview(area, dismissPreview) { popupFocused = $0 }
                    }
                    .frame(width: min(320, geometry.size.width - (compact ? 24 : railWidth + columnGap + 18)), height: min(300, geometry.size.height - (compact ? 96 : 32)))
                    .foregroundStyle(InterfaceBEditorialStyle.ink(previewScheme))
                    .tint(InterfaceBEditorialStyle.ink(previewScheme))
                    .environment(\.colorScheme, previewScheme)
                    .background { InterfaceBEditorialGlassSurface().environment(\.colorScheme, scheme) }
                    .clipShape(RoundedRectangle(cornerRadius: 14))
                    .overlay {
                        if columnBVisible {
                            RoundedRectangle(cornerRadius: 14).strokeBorder(InterfaceBEditorialStyle.ink(scheme).opacity(0.2))
                                .allowsHitTesting(false)
                        } else {
                            GeometryReader { geometry in
                                InterfaceBEditorialCoveLight(area: area, bounds: CGRect(origin: .zero, size: geometry.size), cornerRadius: 14)
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
                    .environment(\.interfaceBGlassArea, area)
                }
            }
            .onChange(of: focusedIcon) { _, area in
                if case .keyboard = previewSession, let area { previewSession = .keyboard(area) }
            }
            .task(id: Activity(icon: hoveredIcon, inside: keepsPreviewOpen, session: previewSession)) {
                if let area = hoveredIcon {
                    do { try await Task.sleep(for: .milliseconds(previewSession == nil ? 1_000 : 0)) }
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
        .coordinateSpace(name: InterfaceBNavigationCoordinates.shell)
        .onPreferenceChange(InterfaceBNavigationHitRegions.self) { regions in
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
    private var previewScheme: ColorScheme { scheme }
    @ViewBuilder private func rail(horizontal: Bool) -> some View {
        if horizontal {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 4) { ForEach(MemberArea.allCases) { item($0) } }.padding(6)
            }.frame(height: 76)
        } else {
            VStack(spacing: 6) {
                ForEach(MemberArea.allCases) { item($0) }
                Spacer(minLength: 0)
            }
            .padding(.top, 64)
            .padding(.horizontal, railInset).frame(width: railWidth)
        }
    }
    private func item(_ area: MemberArea) -> some View {
        Button { activate(area) } label: {
            HStack(spacing: 0) {
                Image(systemName: area.icon)
            }
            .font(.system(size: 20))
            .frame(width: 48, height: 48)
            .accessibilityHidden(true)
        }
        .buttonStyle(InterfaceBQuietWorkspaceButtonStyle(selected: selected == area, focused: focusedIcon == area, navigation: true))
        .environment(\.interfaceBGlassArea, area)
        .focusable()
        .focusEffectDisabled()
        .focused($focusedIcon, equals: area)
        .onKeyPress(.space) { activateFromKeyboard(area); return .handled }
        .onKeyPress(.return) { activateFromKeyboard(area); return .handled }
        .onKeyPress(.rightArrow) { openKeyboardPreview(area); return .handled }
        .background { hitRegion(.icon(area)) }
        .accessibilityLabel(area.title)
        .accessibilityAddTraits(selected == area ? .isSelected : [])
        .accessibilityAction(named: "Preview navigation") { openKeyboardPreview(area) }
        .help("Open \(area.title). Hover or press Right Arrow for layout controls; Escape closes them.")
    }
    private func activate(_ area: MemberArea) {
        if selected != area { navigate(area) }
        // Pointer/accessibility activation selects the area without inventing
        // keyboard focus. The selected cove remains the steady indicator.
        focusedIcon = nil
    }
    private func activateFromKeyboard(_ area: MemberArea) {
        if selected != area { navigate(area) }
        focusedIcon = area
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
    private func hitRegion(_ target: InterfaceBNavigationHitRegion) -> some View {
        GeometryReader { geometry in
            Color.clear.preference(key: InterfaceBNavigationHitRegions.self,
                value: [target: geometry.frame(in: .named(InterfaceBNavigationCoordinates.shell))])
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

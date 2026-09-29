import ArtWindowKit
import SwiftUI
import WidgetKit

/// Changes are staged in `filters` and `style` and only reach `Preferences` when
/// *Apply changes* is pressed, so a half-edited set of filters never starts a fetch.
struct SettingsView: View {
    let shown: Artwork?
    let showGuide: () -> Void

    @State private var filters = Preferences.shared.filters
    @State private var style = Preferences.shared.style
    @State private var availability = Availability.everything
    @State private var edgeSwatches: [RenderStyle.Border] = []
    @State private var foldedArtists = Preferences.shared.filters.artists.isEmpty

    private var isChanged: Bool {
        filters != Preferences.shared.filters || style != Preferences.shared.style
    }

    var body: some View {
        List {
            Section {
                PreviewFrame(shown: shown, style: style)
                    .frame(maxWidth: .infinity)
                    .listRowBackground(Color.clear)
                StyleStrip(style: $style, swatches: edgeSwatches)
            }

            Section {
                FilterSections(filters: $filters, availability: availability, foldedArtists: $foldedArtists)
                Toggle("Hide religious scenes", isOn: $filters.hideReligious)
            }

            Section {
                if !availability.anyMatch {
                    Text("Nothing matches these filters — set one section to Any")
                        .font(.footnote).foregroundStyle(.orange)
                }
                Button("Apply changes", action: apply)
                    .frame(maxWidth: .infinity)
                    .disabled(!isChanged || !availability.anyMatch)
            }

            Section {
                Button("Daily wallpaper automation", action: showGuide)
            }
        }
        .scrollContentBackground(.hidden)
        // Availability walks the whole catalogue, so it never runs on the main thread;
        // a newer edit cancels the wait for an older one.
        .task(id: filters) {
            let staged = filters
            availability = await Task.detached(priority: .userInitiated) {
                Availability(for: staged, screen: DeviceScreen.current)
            }.value
        }
        .task(id: shown?.fileName) {
            guard let url = shown?.fileURL else { edgeSwatches = []; return }
            edgeSwatches = await Task.detached(priority: .utility) {
                Renderer.edgeColours(imageAt: url)
            }.value
        }
    }

    private func apply() {
        Preferences.shared.filters = filters
        Preferences.shared.style = style
        WidgetCenter.shared.reloadAllTimelines()
    }
}

/// What the catalogue can still offer under the staged filters.
struct Availability: Sendable {
    var regions: Set<Region>
    var subjects: Set<ArtworkSubject>
    var artists: Set<String>
    var anyMatch: Bool

    static let everything = Availability(
        regions: Set(Region.allCases), subjects: Set(ArtworkSubject.allCases), artists: [], anyMatch: true)

    init(regions: Set<Region>, subjects: Set<ArtworkSubject>, artists: Set<String>, anyMatch: Bool) {
        self.regions = regions
        self.subjects = subjects
        self.artists = artists
        self.anyMatch = anyMatch
    }

    init(for filters: Filters, screen: Screen) {
        let catalogue = Catalogue.shared
        regions = catalogue.availableRegions(filters, screen: screen)
        subjects = catalogue.availableSubjects(filters, screen: screen)
        artists = catalogue.availableArtists(filters, screen: screen)
        anyMatch = catalogue.anyMatch(filters, screen: screen)
    }
}

/// The wallpaper as it will be cut, in a frame the shape of this device's screen.
private struct PreviewFrame: View {
    let shown: Artwork?
    let style: RenderStyle

    private struct Input: Equatable {
        let fileName: String?
        let style: RenderStyle
    }

    @State private var image: UIImage?
    private static let width = 150

    var body: some View {
        let device = DeviceScreen.current
        let aspect = device.aspectRatio
        Group {
            if let image {
                Image(uiImage: image).resizable().aspectRatio(aspect, contentMode: .fit)
            } else {
                Text(shown == nil ? "Preview unavailable" : "")
                    .font(.footnote).foregroundStyle(.secondary)
            }
        }
        .frame(width: CGFloat(Self.width), height: CGFloat(Self.width) / aspect)
        .background(Color.black)
        .clipShape(RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(Color.hairline, lineWidth: 1))
        // Blur has no release-time debounce: every slider value re-renders, and `task(id:)`
        // drops the render it made stale.
        .task(id: Input(fileName: shown?.fileName, style: style)) {
            guard let url = shown?.fileURL else { image = nil; return }
            let style = style
            let height = Int((Double(Self.width * 2) / aspect).rounded())
            let small = Screen(width: Self.width * 2, height: height)
            let rendered = await Task.detached(priority: .userInitiated) {
                (try? Renderer.render(imageAt: url, style: style, screen: small)).map { UIImage(cgImage: $0) }
            }.value
            if !Task.isCancelled { image = rendered }
        }
    }
}

/// Zoom / Stretch / Blur / Borders in one strip; the extra controls appear only for
/// the mode that uses them.
private struct StyleStrip: View {
    @Binding var style: RenderStyle
    let swatches: [RenderStyle.Border]

    var body: some View {
        Picker("Placement", selection: $style.mode) {
            ForEach(RenderStyle.Mode.allCases, id: \.self) { Text(Self.label($0)).tag($0) }
        }
        .pickerStyle(.segmented)

        switch style.mode {
        case .blur:
            VStack(alignment: .leading) {
                Text("Blur strength").font(.footnote).foregroundStyle(.secondary)
                Slider(value: strength, in: 0...100)
            }
            Toggle("Blur the whole fill", isOn: $style.blurWholeFill)
        case .borders:
            borderChoices
        case .zoom, .stretch:
            EmptyView()
        }
    }

    private static func label(_ mode: RenderStyle.Mode) -> String {
        switch mode {
        case .zoom: "Zoom"
        case .stretch: "Stretch"
        case .blur: "Blur"
        case .borders: "Borders"
        }
    }

    private var strength: Binding<Double> {
        Binding(get: { Double(style.blurStrength) }, set: { style.blurStrength = Int($0.rounded()) })
    }

    private var choices: [RenderStyle.Border] {
        [.black, .edge] + swatches.filter { $0 != .black && $0 != .edge }
    }

    private var borderChoices: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Border colour").font(.footnote).foregroundStyle(.secondary)
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 12) {
                    ForEach(choices, id: \.self) { border in
                        Button { style.border = border } label: {
                            Swatch(border: border, edge: swatches.first, selected: style.border == border)
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(2)
            }
            ColorPicker("Custom", selection: customColour, supportsOpacity: false)
        }
    }

    private var customColour: Binding<Color> {
        Binding(
            get: {
                if case .custom(let r, let g, let b) = style.border { return Color(red: r, green: g, blue: b) }
                return .gray
            },
            set: { colour in
                var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
                UIColor(colour).getRed(&r, green: &g, blue: &b, alpha: &a)
                style.border = .custom(red: Double(r), green: Double(g), blue: Double(b))
            })
    }
}

private struct Swatch: View {
    let border: RenderStyle.Border
    /// What "edge" will look like on this painting, when a swatch is known.
    let edge: RenderStyle.Border?
    let selected: Bool

    var body: some View {
        ZStack {
            Circle().fill(fill)
            if border == .edge { Text("Edge").font(.system(size: 10, weight: .semibold)).foregroundStyle(.white) }
        }
        .frame(width: 34, height: 34)
        .overlay(Circle().stroke(selected ? Color.accentColor : Color.hairline, lineWidth: selected ? 3 : 1))
        .accessibilityLabel(border == .black ? "Black" : border == .edge ? "Edge colour" : "Colour from the painting")
    }

    private var fill: Color {
        switch border {
        case .black: return .black
        case .edge: if case .custom(let r, let g, let b)? = edge { return Color(red: r, green: g, blue: b) } else { return .gray }
        case .custom(let r, let g, let b): return Color(red: r, green: g, blue: b)
        }
    }
}

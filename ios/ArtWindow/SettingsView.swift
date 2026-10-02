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

    private struct Pool: Equatable {
        let filters: Filters
        let style: RenderStyle
    }

    private var poolStyle: RenderStyle {
        RenderStyle(mode: style.mode, blurWholeFill: style.blurWholeFill, rotateWide: style.rotateWide)
    }

    private struct EdgeInput: Equatable {
        let fileName: String?
        let rotateWide: Bool
    }

    private var isChanged: Bool {
        filters != Preferences.shared.filters || style != Preferences.shared.style
    }

    /// Filters already applied are not held against a change of style: a selection saved
    /// before the floor existed would otherwise lock the whole tab until it was widened.
    private var canApply: Bool {
        isChanged && (availability.matching >= needed || filters == Preferences.shared.filters)
    }

    /// Nothing is said while the pool is enough; below the floor the wording depends on
    /// whether the thin filters are the staged edit or the ones already in force.
    private var needed: Int { Catalogue.needed(for: filters) }

    private var shortfall: String? {
        let n = availability.matching
        guard n < needed else { return nil }
        if filters == Preferences.shared.filters {
            let few = n == 0 ? "No painting matches" : n == 1 ? "Only 1 painting matches" : "Only \(n) paintings match"
            return "\(few) these filters, so pictures come from a wider selection"
        }
        if n == 0 { return "No painting matches these filters — set one section to Any" }
        let few = n == 1 ? "Only 1 painting matches" : "Only \(n) paintings match"
        return "\(few) these filters — at least \(needed) are needed"
    }

    var body: some View {
        List {
            Section {
                PreviewFrame(shown: shown, style: $style)
                    .frame(maxWidth: .infinity)
                    .listRowBackground(Color.clear)
                StyleStrip(style: $style, swatches: edgeSwatches)
            }

            Section {
                FilterSections(filters: $filters, availability: availability)
                Toggle("Hide religious scenes", isOn: $filters.hideReligious)
            }

            Section {
                if let shortfall {
                    Text(shortfall)
                        .font(.footnote).foregroundStyle(.orange)
                }
                Button("Apply changes", action: apply)
                    .frame(maxWidth: .infinity)
                    .disabled(!canApply)
            }

            Section {
                Button("Daily wallpaper automation", action: showGuide)
            }
        }
        .scrollContentBackground(.hidden)
        // Availability walks the whole catalogue, so it never runs on the main thread;
        // a newer edit cancels the wait for an older one.
        // The style matters too: whether a painting can be drawn, and whether it is turned
        // first, changes the pool. Only the parts that do, so a blur slider stays cheap.
        .task(id: Pool(filters: filters, style: poolStyle)) {
            let staged = filters, style = poolStyle
            availability = await Task.detached(priority: .userInitiated) {
                Availability(for: staged, screen: DeviceScreen.current, style: style)
            }.value
        }
        .task(id: EdgeInput(fileName: shown?.fileName, rotateWide: style.rotateWide)) {
            guard let url = shown?.fileURL else { edgeSwatches = []; return }
            let style = style
            edgeSwatches = await Task.detached(priority: .utility) {
                Renderer.edgeColours(imageAt: url, style: style, screen: DeviceScreen.current)
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
    /// How many paintings the staged filters admit as a whole.
    var matching: Int

    static let everything = Availability(
        regions: Set(Region.allCases), subjects: Set(ArtworkSubject.allCases), artists: [], matching: Int.max)

    init(regions: Set<Region>, subjects: Set<ArtworkSubject>, artists: Set<String>, matching: Int) {
        self.regions = regions
        self.subjects = subjects
        self.artists = artists
        self.matching = matching
    }

    init(for filters: Filters, screen: Screen, style: RenderStyle) {
        let catalogue = Catalogue.shared
        regions = catalogue.availableRegions(filters, screen: screen, style: style)
        subjects = catalogue.availableSubjects(filters, screen: screen, style: style)
        artists = catalogue.availableArtists(filters, screen: screen, style: style)
        matching = catalogue.matchCount(filters, screen: screen, style: style)
    }
}

/// The wallpaper as it will be cut, in a frame the shape of this device's screen.
///
/// Two layers, so that framing never re-renders anything. Underneath, what does not depend
/// on framing (the blurred backdrop, the border colour) is rendered by `Renderer`; over it
/// the sharp painting is an image of its own that SwiftUI positions with `Framing.rect`,
/// the same geometry `Renderer` uses at full size. Gestures change three numbers and
/// nothing is decoded or drawn again until they end.
private struct PreviewFrame: View {
    let shown: Artwork?
    @Binding var style: RenderStyle

    private struct Backdrop: Equatable {
        let fileName: String?
        let style: RenderStyle
    }

    private struct Sharp: Equatable {
        let fileName: String?
        let rotateWide: Bool
    }

    /// The framing while a gesture is in progress, committed to `style` when it ends.
    private struct Live {
        var zoom: Double
        var panX: Double
        var panY: Double
    }

    @State private var backdrop: UIImage?
    @State private var sharp: UIImage?
    /// The painting's size as it will hang.
    @State private var hungSize: CGSize?
    @State private var live: Live?
    @State private var panStart: (x: Double, y: Double)?
    @State private var zoomStart: Double?

    private static let sharpPixels = 2400

    /// Nearly half the screen's height, so there is something to pinch, at the device's
    /// proportions and never wider than the screen allows.
    private static func size(aspect: Double) -> CGSize {
        let bounds = UIScreen.main.bounds
        var height = bounds.height * 0.45
        var width = height * aspect
        if width > bounds.width - 64 {
            width = bounds.width - 64
            height = width / aspect
        }
        return CGSize(width: width, height: height)
    }

    var body: some View {
        let device = DeviceScreen.current
        let size = Self.size(aspect: device.aspectRatio)
        let framing = style.canFrame(on: device) && sharp != nil && hungSize != nil
        VStack(spacing: 6) {
            ZStack(alignment: .topLeading) {
                Color.black
                if let backdrop {
                    Image(uiImage: backdrop).resizable().frame(width: size.width, height: size.height)
                } else if shown == nil {
                    Text("Preview unavailable").font(.footnote).foregroundStyle(.secondary)
                }
                if framing, let sharp, let rect = rect(in: size) {
                    Image(uiImage: sharp).resizable()
                        .frame(width: rect.width, height: rect.height)
                        .offset(x: rect.minX, y: rect.minY)
                }
            }
            .frame(width: size.width, height: size.height)
            .clipShape(RoundedRectangle(cornerRadius: 14))
            .overlay(RoundedRectangle(cornerRadius: 14).stroke(Color.hairline, lineWidth: 1))
            .overlay {
                if framing, let rect = rect(in: size) {
                    FrameGestures(
                        canMoveX: rect.width > size.width + 0.5, canMoveY: rect.height > size.height + 0.5,
                        onPan: { phase, translation in pan(phase, translation, in: size) },
                        onPinch: { phase, scale in pinch(phase, scale) })
                }
            }
            if framing {
                Text("Pinch and drag to frame it").font(.footnote).foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity)
        // The layer under the painting. Blur has no release-time debounce: every slider
        // value re-renders, and `task(id:)` drops the render it made stale. Framing is not
        // in the id on purpose.
        .task(id: Backdrop(fileName: shown?.fileName, style: style.unframed)) {
            guard let url = shown?.fileURL else { backdrop = nil; return }
            let style = style
            let scale = 2.0
            let preview = Screen(width: Int(size.width * scale), height: Int(size.height * scale))
            let withPicture = !style.canFrame(on: preview)
            let rendered = await Task.detached(priority: .userInitiated) {
                (try? Renderer.render(imageAt: url, style: style, screen: preview, picture: withPicture))
                    .map { UIImage(cgImage: $0) }
            }.value
            if !Task.isCancelled { backdrop = rendered }
        }
        // The sharp painting is decoded once per painting (and again if it is turned).
        .task(id: Sharp(fileName: shown?.fileName, rotateWide: style.rotateWide)) {
            guard let url = shown?.fileURL else { sharp = nil; hungSize = nil; return }
            let style = style
            let device = DeviceScreen.current
            let result = await Task.detached(priority: .userInitiated) { () -> (UIImage, CGSize)? in
                guard let file = Renderer.pixelSize(of: url) else { return nil }
                let hung = style.hung(width: file.width, height: file.height, on: device)
                let turned = hung != file
                guard let image = Renderer.picture(imageAt: url, maxPixel: Self.sharpPixels, turned: turned)
                else { return nil }
                return (UIImage(cgImage: image), CGSize(width: hung.width, height: hung.height))
            }.value
            if !Task.isCancelled { sharp = result?.0; hungSize = result?.1 }
        }
    }

    // MARK: Framing

    private func current() -> Live {
        if let live { return live }
        let pan = style.effectivePan(for: shown?.fileName)
        return Live(zoom: style.effectiveZoom, panX: pan.x, panY: pan.y)
    }

    private func rect(in size: CGSize, _ values: Live? = nil) -> CGRect? {
        guard let hungSize else { return nil }
        let v = values ?? current()
        return Framing.rect(
            painting: hungSize, screen: size, base: style.frameBase, zoom: v.zoom, panX: v.panX, panY: v.panY)
    }

    /// The picture moves exactly as far as the finger: a pan is the travel as a fraction
    /// of how far the zoomed painting overflows the frame, clamped at its edges.
    private func pan(_ phase: FrameGestures.Phase, _ translation: CGSize, in size: CGSize) {
        switch phase {
        case .began:
            let c = current()
            panStart = (c.panX, c.panY)
        case .changed:
            guard let start = panStart, let r = rect(in: size) else { return }
            var l = current()
            let (ox, oy) = (r.width - size.width, r.height - size.height)
            if ox > 0.5 { l.panX = min(max(start.x - Double(translation.width / ox), 0), 1) }
            if oy > 0.5 { l.panY = min(max(start.y - Double(translation.height / oy), 0), 1) }
            live = l
        case .ended:
            panStart = nil
            commit()
        }
    }

    private func pinch(_ phase: FrameGestures.Phase, _ scale: CGFloat) {
        switch phase {
        case .began: zoomStart = current().zoom
        case .changed:
            guard let start = zoomStart else { return }
            var l = current()
            l.zoom = min(max(start * Double(scale), 1), Framing.maxZoom)
            live = l
        case .ended:
            zoomStart = nil
            commit()
        }
    }

    private func commit() {
        guard let l = live, let name = shown?.fileName else { live = nil; return }
        style.frameZoom = l.zoom
        style.panX = l.panX
        style.panY = l.panY
        style.panFor = name
        live = nil
    }
}

/// A pan and a pinch on a plain UIKit view, because only UIKit lets a pan decline to begin.
/// A drag along an axis the picture cannot move on is refused at the start, so the Settings
/// list underneath scrolls from a touch on the preview as it does from anywhere else.
private struct FrameGestures: UIViewRepresentable {
    enum Phase { case began, changed, ended }

    let canMoveX: Bool
    let canMoveY: Bool
    let onPan: (Phase, CGSize) -> Void
    let onPinch: (Phase, CGFloat) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeUIView(context: Context) -> UIView {
        let view = UIView()
        view.backgroundColor = .clear
        let pan = UIPanGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.pan(_:)))
        let pinch = UIPinchGestureRecognizer(target: context.coordinator, action: #selector(Coordinator.pinch(_:)))
        pan.delegate = context.coordinator
        pinch.delegate = context.coordinator
        view.addGestureRecognizer(pan)
        view.addGestureRecognizer(pinch)
        return view
    }

    func updateUIView(_ view: UIView, context: Context) { context.coordinator.parent = self }

    final class Coordinator: NSObject, UIGestureRecognizerDelegate {
        var parent: FrameGestures

        init(_ parent: FrameGestures) { self.parent = parent }

        private func phase(_ state: UIGestureRecognizer.State) -> Phase? {
            switch state {
            case .began: .began
            case .changed: .changed
            case .ended, .cancelled, .failed: .ended
            default: nil
            }
        }

        @objc func pan(_ g: UIPanGestureRecognizer) {
            guard let phase = phase(g.state) else { return }
            let t = g.translation(in: g.view)
            parent.onPan(phase, CGSize(width: t.x, height: t.y))
        }

        @objc func pinch(_ g: UIPinchGestureRecognizer) {
            guard let phase = phase(g.state) else { return }
            parent.onPinch(phase, g.scale)
        }

        func gestureRecognizerShouldBegin(_ g: UIGestureRecognizer) -> Bool {
            guard let pan = g as? UIPanGestureRecognizer, let view = pan.view else { return true }
            let v = pan.velocity(in: view)
            return abs(v.x) >= abs(v.y) ? parent.canMoveX : parent.canMoveY
        }

        // Pan and pinch work together; the list's own scrolling is never joined.
        func gestureRecognizer(_ g: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool {
            g.view === other.view
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

        if !DeviceScreen.isPad {
            Toggle("Turn wide paintings", isOn: $style.rotateWide)
        }

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

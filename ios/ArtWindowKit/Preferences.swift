// Port of android/app/src/main/java/dev/artwindow/WallpaperPreferences.kt — defaults and
// rules must stay in step. Where Android keeps one flat preference bag (`WallpaperStyle`,
// `BlurVariant`, `BorderColorMode`, ...) this splits it in two: `Filters` decide *which*
// painting, `RenderStyle` decides *how it is drawn*, because the catalogue needs the
// first on its own and only a render needs the second.

import Foundation

public enum Region: String, CaseIterable, Codable, Sendable {
    case europe = "EUROPE", asia = "ASIA", africa = "AFRICA"
    case northAmerica = "NORTH_AMERICA", southAmerica = "SOUTH_AMERICA", oceania = "OCEANIA"

    public var label: String {
        switch self {
        case .europe: "Europe"
        case .asia: "Asia"
        case .africa: "Africa"
        case .northAmerica: "North America"
        case .southAmerica: "South America"
        case .oceania: "Oceania"
        }
    }
}

/// What a painting is of. `queries` are the catalogue terms that stand in for it,
/// matched against title and tags — several for a subject the collections hold thinly.
///
/// SMK's records are Danish, so each list carries the Danish words for the same subject,
/// each irregular plural spelled out because the matcher's "+s" rule does not form it.
/// Danish "by" (town) is deliberately absent: it collides with the English preposition.
/// These word lists live three times — Catalogue.kt, src/art/museums.rs and here — so a
/// change goes in all three.
public enum ArtworkSubject: String, CaseIterable, Codable, Sendable {
    case landscape, seascape, stillLife

    public var label: String {
        switch self {
        case .landscape: "Landscape"
        case .seascape: "Seascape"
        case .stillLife: "Still life"
        }
    }

    var queries: [String] {
        switch self {
        case .landscape:
            ["landscape", "cityscape", "city", "street",
             "landskab", "landskaber", "parti fra", "udsigt", "gade", "gader"]
        case .seascape:
            ["seascape", "marine", "boats",
             "havn", "skibe", "kyst", "strand", "hav", "både"]
        case .stillLife:
            ["still life", "flowers",
             "opstilling", "blomster", "stilleben"]
        }
    }
}

public enum ArtworkShape: String, CaseIterable, Codable, Sendable {
    /// Shaped like the screen: tall on a phone, roughly square on an iPad's square canvas.
    case screen
    /// `screen` plus paintings near square, on the side away from the screen's own shape.
    case nearSquare
    case any

    public func label(isPad: Bool) -> String {
        switch self {
        case .screen: isPad ? "iPad-shaped" : "Phone-shaped"
        case .nearSquare: "Include near-square"
        case .any: "Any shape"
        }
    }

    /// Five by four: the widest a painting may be and still count as near square on a portrait screen.
    private static let nearSquareMax = 1.25
    /// Four by five: the same limit turned on its side, for a landscape screen.
    private static let nearSquareMin = 0.8

    func accepts(aspect: Double, screen: Screen) -> Bool {
        guard aspect.isFinite, aspect > 0 else { return false }
        switch self {
        case .screen:
            return screen.holds(aspect)
        case .nearSquare:
            if screen.isLandscape {
                return aspect >= Self.nearSquareMin && aspect <= screen.aspectRatio * (1 + Screen.maxTrim)
            }
            return aspect >= screen.aspectRatio * (1 - Screen.maxTrim) && aspect <= Self.nearSquareMax
        case .any:
            return true
        }
    }
}

/// Sections are ANDed, the choices within a section are ORed, and an empty set means Any.
public struct Filters: Codable, Equatable, Sendable {
    public var shape: ArtworkShape
    public var regions: Set<Region>
    public var subjects: Set<ArtworkSubject>
    public var artists: Set<String>
    public var hideReligious: Bool

    public init(
        shape: ArtworkShape = .screen,
        regions: Set<Region> = [.europe, .asia],
        subjects: Set<ArtworkSubject> = [.landscape],
        artists: Set<String> = [],
        hideReligious: Bool = false
    ) {
        self.shape = shape
        self.regions = regions
        self.subjects = subjects
        self.artists = artists
        self.hideReligious = hideReligious
    }

    /// One painter at a time, so `artists` holds at most one name; it stays a set so nothing
    /// stored has to migrate and `Catalogue` keeps taking a collection. An earlier build could
    /// store several, and a mixture nobody asked for by name is worse than a guess, so only one
    /// is read — here and nowhere else. A set has no order, so "first" is the alphabetical one.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        shape = try c.decode(ArtworkShape.self, forKey: .shape)
        regions = try c.decode(Set<Region>.self, forKey: .regions)
        subjects = try c.decode(Set<ArtworkSubject>.self, forKey: .subjects)
        artists = Set(try c.decode(Set<String>.self, forKey: .artists).sorted().prefix(1))
        hideReligious = try c.decode(Bool.self, forKey: .hideReligious)
    }

    /// Makes `artist` the one painter chosen, in place of whoever was.
    public mutating func chooseArtist(_ artist: String) { artists = [artist] }

    /// Back to paintings by anyone.
    public mutating func anyArtist() { artists = [] }

    /// A fresh install starts from Europe and Asia, landscapes, no artist: not an unfiltered
    /// Any. Once saved, an empty section is honestly Any — there is no other fallback.
    public static let defaults = Filters()
}

public struct RenderStyle: Codable, Equatable, Sendable {
    public enum Mode: String, Codable, CaseIterable, Sendable { case zoom, stretch, blur, borders }

    public enum Border: Codable, Hashable, Sendable {
        case black, edge
        case custom(red: Double, green: Double, blue: Double)

        /// Android's `DEFAULT_CUSTOM_COLOR` (0xff3434c8), what the picker starts from.
        public static let defaultCustom = Border.custom(red: 0x34 / 255.0, green: 0x34 / 255.0, blue: 0xc8 / 255.0)
    }

    public var mode: Mode
    public var blurStrength: Int
    /// Android's `BlurVariant.WHOLE_IMAGE`: the blurred picture fills the screen and the sharp one is not drawn on top.
    public var blurWholeFill: Bool
    public var border: Border
    /// iPhone only: a painting wider than tall is turned 90° clockwise (its top ends at the
    /// screen's right edge) before it is hung on a screen taller than wide, and every mode
    /// then works on the turned picture. Not a fifth mode.
    public var rotateWide: Bool
    /// How the sharp painting is framed in Zoom, Blur and Borders. `frameZoom` (1...3, 1 being
    /// the style's own size) is a style option that outlives the painting; `panX` and `panY`
    /// (0...1 of the overflow on that axis, 0.5 centred) belong to the one painting named by
    /// `panFor` and are read only through `effectivePan(for:)`, so the next painting starts
    /// centred with nothing to reset.
    public var frameZoom: Double
    public var panX: Double
    public var panY: Double
    public var panFor: String?

    public init(
        mode: Mode = .zoom, blurStrength: Int = 50, blurWholeFill: Bool = false, border: Border = .black,
        rotateWide: Bool = false, frameZoom: Double = 1, panX: Double = 0.5, panY: Double = 0.5,
        panFor: String? = nil
    ) {
        self.frameZoom = frameZoom
        self.panX = panX
        self.panY = panY
        self.panFor = panFor
        self.rotateWide = rotateWide
        self.mode = mode
        self.blurStrength = blurStrength
        self.blurWholeFill = blurWholeFill
        self.border = border
    }

    public static let defaults = RenderStyle()

    private enum CodingKeys: String, CodingKey { case mode, blurStrength, blurWholeFill, border, rotateWide, frameZoom, panX, panY, panFor
        /// Written by the first framing build, which had one pan; read, never written.
        case pan
    }

    /// Styles saved before `rotateWide` have no such key; failing on it would reset every
    /// one of them to the default without a word.
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        mode = try c.decode(Mode.self, forKey: .mode)
        blurStrength = try c.decode(Int.self, forKey: .blurStrength)
        blurWholeFill = try c.decode(Bool.self, forKey: .blurWholeFill)
        border = try c.decode(Border.self, forKey: .border)
        rotateWide = try c.decodeIfPresent(Bool.self, forKey: .rotateWide) ?? false
        frameZoom = try c.decodeIfPresent(Double.self, forKey: .frameZoom) ?? 1
        // The old single pan moved along whichever axis the painting overflowed, and the
        // other axis did not overflow at zoom 1, so it is the right value for both.
        let old = try c.decodeIfPresent(Double.self, forKey: .pan) ?? 0.5
        panX = try c.decodeIfPresent(Double.self, forKey: .panX) ?? old
        panY = try c.decodeIfPresent(Double.self, forKey: .panY) ?? old
        panFor = try c.decodeIfPresent(String.self, forKey: .panFor)
    }

    public func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(mode, forKey: .mode)
        try c.encode(blurStrength, forKey: .blurStrength)
        try c.encode(blurWholeFill, forKey: .blurWholeFill)
        try c.encode(border, forKey: .border)
        try c.encode(rotateWide, forKey: .rotateWide)
        try c.encode(frameZoom, forKey: .frameZoom)
        try c.encode(panX, forKey: .panX)
        try c.encode(panY, forKey: .panY)
        try c.encodeIfPresent(panFor, forKey: .panFor)
    }

    /// The pan for the painting called `fileName`: the stored one when it was set for that
    /// painting, otherwise the centre.
    public func effectivePan(for fileName: String?) -> (x: Double, y: Double) {
        guard let fileName, panFor == fileName else { return (0.5, 0.5) }
        return (min(max(panX, 0), 1), min(max(panY, 0), 1))
    }

    public var effectiveZoom: Double { min(max(frameZoom, 1), Framing.maxZoom) }

    /// Whether the sharp painting can be pinched and dragged in this style: Zoom, Blur
    /// with a backdrop and Borders. Stretch shows the whole picture and Blur's whole fill
    /// has no sharp one. Only a portrait screen frames; the iPad's square canvas does not.
    public func canFrame(on screen: Screen) -> Bool {
        guard screen.height > screen.width else { return false }
        return switch mode {
        case .zoom, .borders: true
        case .blur: !blurWholeFill
        case .stretch: false
        }
    }

    public var frameBase: Framing.Base { mode == .zoom ? .cover : .fit }

    /// This style with the framing back at its default: the part of the style that the
    /// backdrop underneath the sharp painting depends on.
    public var unframed: RenderStyle {
        var style = self
        style.frameZoom = 1
        style.panX = 0.5
        style.panY = 0.5
        style.panFor = nil
        return style
    }

    /// Where the sharp painting of `fileName`, of size `painting` as hung, sits on `screen`.
    public func framedRect(for fileName: String?, painting: CGSize, on screen: Screen) -> CGRect {
        let size = CGSize(width: screen.width, height: screen.height)
        guard canFrame(on: screen) else { return Framing.rect(painting: painting, screen: size, base: frameBase) }
        let pan = effectivePan(for: fileName)
        return Framing.rect(painting: painting, screen: size, base: frameBase, zoom: effectiveZoom, panX: pan.x, panY: pan.y)
    }

    /// The size of a painting as it will be hung: turned when `rotateWide` is on, the
    /// screen is taller than wide and the painting is wider than tall. The one answer to
    /// that question — the shape filter, `canRender` and the renderer all ask it, so a
    /// square iPad canvas can never turn a picture even with a stale preference.
    public func hung(width: Int, height: Int, on screen: Screen) -> (width: Int, height: Int) {
        rotateWide && screen.height > screen.width && width > height ? (height, width) : (width, height)
    }

    /// Whether a picture of this size can be drawn in this style without exceeding
    /// `Screen.maxEnlargement` — the geometry `Renderer.render` applies, asked ahead of
    /// time so a candidate that would fail to render is never offered.
    func canRender(width: Int, height: Int, on screen: Screen) -> Bool {
        let (width, height) = hung(width: width, height: height, on: screen)
        return switch mode {
        case .zoom: screen.cover(width: width, height: height) != nil
        case .stretch: screen.canStretch(width: width, height: height)
        case .blur:
            blurWholeFill
                ? screen.cover(width: width, height: height) != nil
                : screen.fit(width: width, height: height) != nil
        case .borders: screen.fit(width: width, height: height) != nil
        }
    }
}

/// The persisted settings, in the App Group defaults so the app, the widget and the
/// Shortcuts intents agree on them. Unreadable stored values fall back to the defaults.
public final class Preferences: @unchecked Sendable {
    public static let shared = Preferences()

    private let defaults = SharedContainer.defaults

    public var filters: Filters {
        get { decode(Filters.self, key: "filters") ?? .defaults }
        set { encode(newValue, key: "filters") }
    }

    public var style: RenderStyle {
        get {
            var style = decode(RenderStyle.self, key: "renderStyle") ?? .defaults
            style.blurStrength = min(max(style.blurStrength, 0), 100)
            return style
        }
        set { encode(newValue, key: "renderStyle") }
    }

    public var hasSeenAutomationGuide: Bool {
        get { defaults.bool(forKey: "hasSeenAutomationGuide") }
        set { defaults.set(newValue, forKey: "hasSeenAutomationGuide") }
    }

    private func decode<T: Decodable>(_ type: T.Type, key: String) -> T? {
        defaults.data(forKey: key).flatMap { try? JSONDecoder().decode(type, from: $0) }
    }

    private func encode<T: Encodable>(_ value: T, key: String) {
        defaults.set(try? JSONEncoder().encode(value), forKey: key)
    }
}

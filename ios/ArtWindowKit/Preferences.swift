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

    public init(mode: Mode = .zoom, blurStrength: Int = 50, blurWholeFill: Bool = false, border: Border = .black) {
        self.mode = mode
        self.blurStrength = blurStrength
        self.blurWholeFill = blurWholeFill
        self.border = border
    }

    public static let defaults = RenderStyle()

    /// Whether a picture of this size can be drawn in this style without exceeding
    /// `Screen.maxEnlargement` — the geometry `Renderer.render` applies, asked ahead of
    /// time so a candidate that would fail to render is never offered.
    func canRender(width: Int, height: Int, on screen: Screen) -> Bool {
        switch mode {
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

// Port of android/app/src/main/java/dev/artwindow/Catalogue.kt (with `isPortrait` and
// `isReligious`) — rules must stay in step.
//
// The subject word lists (in Preferences.swift), the portrait exclusion and the religious
// word list live three times: Catalogue.kt, src/art/museums.rs and here. A change to any
// of them goes in all three, on purpose.

import Foundation

/// One catalogued painting, ready to download without any further live lookup.
public struct CatalogueEntry: Hashable, Identifiable, Sendable {
    /// "{source}-{objectId}" — also what `keyOf` recovers from a downloaded file's name.
    public var id: String { "\(source)-\(objectId)" }
    public let source: String
    public let objectId: String
    public let region: Region?
    public let width: Int
    public let height: Int
    public let imageURL: URL
    public let pageURL: URL?
    public let title: String
    /// The TSV's `byline` column: "Gilbert Stuart, 1789", or just a date.
    public let byline: String
    /// The TSV's `origin` column: "Japan, Edo period (1615–1868)".
    public let culture: String
    public let artist: String?

    let tags: [String]
}

/// A prebuilt, pixel-verified list of paintings from six museums, built offline by
/// `catalogue/build.py` (see CLAUDE.md). Because each row's pixel size was verified at
/// build time, shape and renderability are decided here outright, with no live preview.
/// Every rule that decides what a phone will show — subject and artist matching, the
/// portrait and religious exclusions, shape and render fit — lives in this file.
public final class Catalogue: @unchecked Sendable {
    public static let shared: Catalogue = {
        let bundle = Bundle(for: Catalogue.self)
        guard let url = bundle.url(forResource: "paintings", withExtension: "tsv"),
              let text = try? String(contentsOf: url, encoding: .utf8)
        else { return Catalogue(tsv: "") }
        return Catalogue(tsv: text)
    }()

    /// The promise behind every filter Settings will apply: at least this many paintings
    /// remain to pick from, so a day's picture is never one of a handful coming round
    /// again. Mirrors `MIN_POOL` in src/art/museums.rs and `MIN_POOL` in Catalogue.kt,
    /// on purpose, like the word lists.
    public static let minPool = 20

    /// How many paintings `filters` must leave to be enough. A chosen painter is a narrower
    /// origin than any region and their paintings, however few, are what was asked for, so
    /// one is enough; the floor exists for combinations nobody knew were thin. Used by
    /// `hasEnough`, `widened` and Settings' Apply rule alike. Mirrors `needed` in
    /// src/art/museums.rs and Catalogue.kt, on purpose.
    public static func needed(for filters: Filters) -> Int {
        filters.artists.isEmpty ? minPool : 1
    }

    public let entries: [CatalogueEntry]

    private let lock = NSLock()
    private var cachedTraits: [Traits]?

    /// Parses the TSV `catalogue/build.py` writes: a leading `#` comment line, then one
    /// `source id region width height image_url details_url title byline origin tags
    /// [artist]` row per painting. The artist column names the painter where the painter has an *Artist* chip, so
    /// eleven and twelve fields are both accepted. A row naming an unknown source or
    /// region, or a non-positive size, is skipped rather than failing the whole list.
    public init(tsv: String) {
        entries = tsv.split(whereSeparator: \.isNewline).compactMap { Catalogue.parse(line: String($0)) }
    }

    private static func parse(line: String) -> CatalogueEntry? {
        guard !line.hasPrefix("#"), !line.allSatisfy(\.isWhitespace) else { return nil }
        let f = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        guard f.count == 11 || f.count == 12,
              MuseumSource(rawValue: f[0]) != nil,
              let region = Region(rawValue: f[2]),
              let width = Int(f[3]), let height = Int(f[4]), width > 0, height > 0,
              let imageURL = URL(string: f[5])
        else { return nil }
        return CatalogueEntry(
            source: f[0], objectId: f[1], region: region, width: width, height: height,
            imageURL: imageURL, pageURL: f[6].isEmpty ? nil : URL(string: f[6]),
            title: f[7], byline: f[8], culture: f[9],
            artist: f.count > 11 && !f[11].isEmpty ? f[11] : nil,
            tags: f[10].isEmpty ? [] : f[10].components(separatedBy: "|")
        )
    }

    // MARK: Queries

    /// Every entry the filters offer on `screen`, shuffled so a fixed prefix of the list
    /// does not always favour the same paintings.
    ///
    /// `style` is an addition to the fixed contract, defaulted to the saved one: whether a
    /// picture can be drawn depends on it (a stretch tolerates what a zoom does not).
    public func candidates(_ filters: Filters, screen: Screen, style: RenderStyle = Preferences.shared.style) -> [CatalogueEntry] {
        matching(filters, screen: screen, style: style).shuffled()
    }

    /// How many paintings the filters admit, counted in full — for the message, not the gate.
    public func matchCount(_ filters: Filters, screen: Screen, style: RenderStyle = Preferences.shared.style) -> Int {
        indices(filters, screen: screen, style: style, ignoring: []).count
    }

    /// Whether the filters leave at least `needed(for:)` paintings. Counting stops at the floor:
    /// the catalogue holds thousands and only "enough or not" is asked, once per chip per edit.
    public func hasEnough(_ filters: Filters, screen: Screen, style: RenderStyle = Preferences.shared.style) -> Bool {
        let traits = self.traits()
        let needed = Catalogue.needed(for: filters)
        return entries.indices.lazy
            .filter { self.admits($0, filters, screen, style, [], traits) }
            .prefix(needed).count == needed
    }

    public func matching(_ filters: Filters, screen: Screen, style: RenderStyle = Preferences.shared.style) -> [CatalogueEntry] {
        indices(filters, screen: screen, style: style, ignoring: []).map { entries[$0] }
    }

    /// Which regions leave at least `minPool` paintings when chosen alone within Origins, the
    /// other sections held at `filters` — the chips Settings should offer. Equivalent to
    /// trying each region as a singleton, but one pass instead of six. A chosen painter makes
    /// Origins idle, so it is lifted here too: no region is hidden on the painter's account.
    public func availableRegions(_ filters: Filters, screen: Screen, style: RenderStyle = Preferences.shared.style) -> Set<Region> {
        var counts: [Region: Int] = [:]
        for i in indices(filters, screen: screen, style: style, ignoring: [.region, .artist]) {
            if let region = entries[i].region { counts[region, default: 0] += 1 }
        }
        return Set(counts.filter { $0.value >= Catalogue.minPool }.keys)
    }

    public func availableSubjects(_ filters: Filters, screen: Screen, style: RenderStyle = Preferences.shared.style) -> Set<ArtworkSubject> {
        let traits = self.traits()
        var counts: [ArtworkSubject: Int] = [:]
        for i in indices(filters, screen: screen, style: style, ignoring: [.subject]) {
            for subject in traits[i].subjects { counts[subject, default: 0] += 1 }
        }
        let needed = Catalogue.needed(for: filters)
        return Set(counts.filter { $0.value >= needed }.keys)
    }

    /// Who can be chosen: anyone with a painting left once Subject and *Hide religious* have
    /// had their say. Shape and Origins are lifted because choosing the painter would lift
    /// them, and one painting is enough for the same reason.
    public func availableArtists(_ filters: Filters, screen: Screen, style: RenderStyle = Preferences.shared.style) -> Set<String> {
        var found: Set<String> = []
        for i in indices(filters, screen: screen, style: style, ignoring: [.artist, .region, .shape]) {
            if let artist = entries[i].artist { found.insert(artist) }
        }
        return found
    }

    /// Why a painter cannot be chosen under the staged filters, when Subject or *Hide
    /// religious* is what leaves them nothing; `nil` when they have a painting to show.
    public enum Emptied: Sendable { case subject, religious }

    public func whatEmpties(
        _ artist: String, _ filters: Filters, screen: Screen, style: RenderStyle = Preferences.shared.style
    ) -> Emptied? {
        var only = filters
        only.artists = [artist]
        if matchCount(only, screen: screen, style: style) > 0 { return nil }
        only.hideReligious = false
        return matchCount(only, screen: screen, style: style) > 0 ? .religious : .subject
    }

    /// `filters`, relaxed just far enough to leave `needed(for:)` paintings to pick from: the
    /// smallest set of active sections that cures it, single sections first, then pairs,
    /// and so on, ties broken in the order Shape, Origin, Subject, Artist, Content.
    /// Settings saved before the floor existed, a catalogue that shrank, or a screen-shaped
    /// filter on a device of another shape must not leave the rotation alternating between
    /// a handful. Adequate filters come back untouched, and so do filters nothing can
    /// rescue, so the caller's "nothing matches" still fires.
    public func widened(_ filters: Filters, screen: Screen, style: RenderStyle = Preferences.shared.style) -> Filters {
        if hasEnough(filters, screen: screen, style: style) { return filters }
        // While a painter is chosen Shape and Origins are not asked (see `admits`), so
        // relaxing them changes nothing and they are not candidates. Relaxing the Artist
        // section brings the floor back to `minPool`, because the threshold belongs to
        // the filters being tested.
        let painter = !filters.artists.isEmpty
        let relaxers: [(active: Bool, relax: (inout Filters) -> Void)] = [
            (!painter && filters.shape != .any, { $0.shape = .any }),
            (!painter && !filters.regions.isEmpty, { $0.regions = [] }),
            (!filters.subjects.isEmpty, { $0.subjects = [] }),
            (!filters.artists.isEmpty, { $0.artists = [] }),
            (filters.hideReligious, { $0.hideReligious = false }),
        ].filter { $0.active }
        guard !relaxers.isEmpty else { return filters }
        for count in 1...relaxers.count {
            for mask in 1..<(1 << relaxers.count) where mask.nonzeroBitCount == count {
                var relaxed = filters
                for (i, r) in relaxers.enumerated() where mask & (1 << i) != 0 { r.relax(&relaxed) }
                if hasEnough(relaxed, screen: screen, style: style) { return relaxed }
            }
        }
        return filters
    }

    /// Every distinct artist name, sorted so the list is stable across loads.
    public func artists() -> [String] {
        Array(Set(entries.compactMap(\.artist))).sorted()
    }

    /// How many of `artist`'s paintings can ever be picked. Portraits are excluded whatever
    /// the filters say, so they are left out of the count too. Mirrors `museums::paintings_by`.
    public func paintings(by artist: String) -> Int {
        let traits = self.traits()
        return entries.indices.reduce(0) { $0 + (entries[$1].artist == artist && !traits[$1].portrait ? 1 : 0) }
    }

    private enum Section { case shape, region, subject, artist }

    /// An entry qualifies when it passes every section — shape, origins, subjects and
    /// artists — with an empty selection meaning Any. The portrait and religious
    /// exclusions apply whatever qualified an entry. `ignoring` lifts sections, which
    /// is how the `available…` queries ask "what could this section still offer?".
    ///
    /// A chosen painter wins over Shape and Origins: they are not asked at all, because
    /// choosing a painter means "show me their work". Subject, *Hide religious* and the
    /// enlargement check still apply. Mirrors `admits` in src/art/museums.rs and Catalogue.kt.
    private func indices(_ filters: Filters, screen: Screen, style: RenderStyle, ignoring: Set<Section>) -> [Int] {
        let traits = self.traits()
        return entries.indices.filter { admits($0, filters, screen, style, ignoring, traits) }
    }

    private func admits(
        _ i: Int, _ filters: Filters, _ screen: Screen, _ style: RenderStyle,
        _ ignoring: Set<Section>, _ traits: [Traits]
    ) -> Bool {
        let entry = entries[i]
        let trait = traits[i]
        let byPainter = !ignoring.contains(.artist) && !filters.artists.isEmpty
        if !byPainter, !ignoring.contains(.region), !filters.regions.isEmpty {
            guard let region = entry.region, filters.regions.contains(region) else { return false }
        }
        if !ignoring.contains(.subject), !filters.subjects.isEmpty,
           filters.subjects.isDisjoint(with: trait.subjects) { return false }
        if byPainter {
            guard let artist = entry.artist, filters.artists.contains(artist) else { return false }
        }
        if trait.portrait { return false }
        if filters.hideReligious && trait.religious { return false }
        // The shape is judged as the painting will hang, so a turned wide painting counts
        // as the tall one it becomes.
        if !byPainter, !ignoring.contains(.shape) {
            let hung = style.hung(width: entry.width, height: entry.height, on: screen)
            guard filters.shape.accepts(aspect: Double(hung.width) / Double(hung.height), screen: screen)
            else { return false }
        }
        return style.canRender(width: entry.width, height: entry.height, on: screen)
    }

    // MARK: Text judgements

    /// What the words of an entry say, worked out once: about thirteen thousand rows times
    /// five regexes is too much to redo for every chip Settings asks about.
    private struct Traits {
        var subjects: Set<ArtworkSubject>
        var portrait: Bool
        var religious: Bool
    }

    private func traits() -> [Traits] {
        lock.lock()
        defer { lock.unlock() }
        if let cachedTraits { return cachedTraits }
        let computed = entries.map { entry in
            // Title and tags are matched as one text, newline-separated: every pattern is
            // whole-word, and none can span a newline, so this reads exactly as
            // "the title or any tag" without a regex run per tag.
            let text = ([entry.title] + entry.tags).joined(separator: "\n")
            return Traits(
                subjects: Set(ArtworkSubject.allCases.filter { Words.subject[$0]!.matches(text) }),
                portrait: Catalogue.isPortrait(title: entry.title, tags: entry.tags),
                religious: Catalogue.isReligious(text: text)
            )
        }
        cachedTraits = computed
        return computed
    }

    /// Whether the title or a tag names a portrait. Danish "portræt" (SMK's titles are
    /// Danish) is checked the same substring way as "portrait", since a portrait is
    /// excluded outright and never itself a subject someone selects.
    static func isPortrait(title: String, tags: [String]) -> Bool {
        title.range(of: "portrait", options: .caseInsensitive) != nil
            || title.range(of: "portræt", options: .caseInsensitive) != nil
            || tags.contains { $0.caseInsensitiveCompare("Portraits") == .orderedSame }
    }

    /// Whether title or tags name a religious scene or figure. One whole-word,
    /// case-insensitive regex, so "Christmas" is never mistaken for "Christ" and "holy"
    /// or "magi" do not fire inside longer words. "St." is deliberately left out — it
    /// would hide views of St. Petersburg. The Danish terms are a short run for the
    /// common subjects, not a translation of the English list.
    static func isReligious(text: String) -> Bool { Words.religious.matches(text) }

    static func isReligious(title: String, tags: [String]) -> Bool {
        isReligious(text: ([title] + tags).joined(separator: "\n"))
    }
}

private struct Words {
    let regex: NSRegularExpression

    func matches(_ text: String) -> Bool {
        regex.firstMatch(in: text, range: NSRange(text.startIndex..., in: text)) != nil
    }

    /// A whole-word, case-insensitive query with an optional trailing "s", so a search
    /// term and its plural both match.
    static let subject: [ArtworkSubject: Words] = Dictionary(uniqueKeysWithValues: ArtworkSubject.allCases.map { subject in
        let alternatives = subject.queries.map(NSRegularExpression.escapedPattern(for:)).joined(separator: "|")
        return (subject, Words(pattern: "\\b(?:\(alternatives))s?\\b"))
    })

    static let religious = Words(pattern: "\\b(?:" + [
        "christ", "jesus", "madonna", "virgin", "saints?", "holy", "annunciation",
        "crucifixion", "crucified", "nativity", "adoration", "magi", "piet[aà]",
        "lamentation", "resurrection", "ascension", "assumption", "transfiguration",
        "apostles?", "evangelists?", "baptism", "angels?", "deposition", "entombment",
        "magdalene", "pope", "bible", "biblical", "gospel", "prophets?",
        "martyrs?", "martyrdom", "last supper", "pentecost", "flight into egypt",
        "moses", "abraham", "noah", "jonah", "tobias", "judith", "susanna", "samson",
        "buddha", "bodhisattva", "arhat", "deit(?:y|ies)",
        "kristus", "jomfru maria", "helgen", "apostel", "engel", "korsfæstelse",
    ].joined(separator: "|") + ")\\b")

    private init(pattern: String) {
        // A pattern that does not compile is a programming error in the lists above.
        regex = try! NSRegularExpression(pattern: pattern, options: .caseInsensitive)
    }
}

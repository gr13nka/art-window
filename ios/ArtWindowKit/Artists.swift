// Mirrors src/art/artists.rs and Pending::artist_cards on the desktop, and the Android
// copy — the row and browser rules live three times, on purpose, like the word lists.

import Foundation

/// One painter the artist browser can show: who they are, the painting they are known
/// by, and where to read more. The picture is bundled, so showing one needs no network.
public struct Painter: Hashable, Identifiable, Sendable {
    public var id: String { name }
    /// Byte-identical to the catalogue's artist column — it is what a stored filter holds.
    public let name: String
    public let region: Region
    /// The Wikipedia article, handed to the browser and never fetched by the app.
    public let about: URL
    /// The painting shown for them.
    public let title: String
    /// Its year, when the catalogue's byline carried one.
    public let year: String?
    public let imageURL: URL

    /// "Golden summer, Eaglemont, 1889 · Oceania, 40 paintings" — the picture, then what
    /// the catalogue holds of this painter.
    public func caption(paintings: Int) -> String {
        let painting = year.map { "\(title), \($0)" } ?? title
        return "\(painting) · \(region.label), \(paintings) \(paintings == 1 ? "painting" : "paintings")"
    }
}

/// The index of `catalogue/dist/artists/`, bundled as a folder reference so the files
/// are the repository's own with no copy to drift.
public final class Artists: @unchecked Sendable {
    public static let shared: Artists = {
        let bundle = Bundle(for: Artists.self)
        guard let directory = bundle.url(forResource: "artists", withExtension: nil),
              let text = try? String(contentsOf: directory.appendingPathComponent("index.tsv"), encoding: .utf8)
        else { return Artists(tsv: "", directory: bundle.bundleURL) }
        return Artists(tsv: text, directory: directory)
    }()

    public let painters: [Painter]

    /// Parses `index.tsv`: a leading `#` comment, then `name region about showcase title
    /// byline file` rows. The file is generated, so a malformed row (wrong column count,
    /// unknown region, no usable URL) is skipped rather than failing the rest.
    public init(tsv: String, directory: URL) {
        painters = tsv.split(whereSeparator: \.isNewline).compactMap {
            Artists.parse(line: String($0), directory: directory)
        }
    }

    private static func parse(line: String, directory: URL) -> Painter? {
        guard !line.hasPrefix("#"), !line.allSatisfy(\.isWhitespace) else { return nil }
        let f = line.split(separator: "\t", omittingEmptySubsequences: false).map(String.init)
        guard f.count == 7, !f[0].isEmpty, !f[4].isEmpty, !f[6].isEmpty,
              let region = Region(rawValue: f[1]),
              let about = URL(string: f[2])
        else { return nil }
        // The byline is "painter, year": the painter is already the heading, so only the
        // year is wanted, and some paintings have none.
        let rest = f[5].hasPrefix(f[0]) ? f[5].dropFirst(f[0].count) : ""
        let year = String(rest.drop { $0 == "," || $0 == " " })
        return Painter(
            name: f[0], region: region, about: about, title: f[4],
            year: year.isEmpty ? nil : year,
            imageURL: directory.appendingPathComponent(f[6]))
    }

    /// The browser's shelf: those of `names` the index has a picture for, by region in
    /// the enum's order and then by name. A catalogue artist with no row is left out.
    public func shelf(of names: [String]) -> [Painter] {
        let known = Set(names)
        let place = { (region: Region) in Region.allCases.firstIndex(of: region) ?? Region.allCases.count }
        return painters.filter { known.contains($0.name) }
            .sorted { (place($0.region), $0.name) < (place($1.region), $1.name) }
    }
}

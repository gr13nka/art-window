// Port of `Artwork`, `keyOf` and `MuseumSource` (Artwork/keyOf in Museums.kt, the enum in
// MuseumSource.kt) from android/app/src/main/java/dev/artwindow/.

import Foundation

/// One of the sources `catalogue/build.py` draws from. `rawValue` is the TSV's `source`
/// column and the prefix a download's file name carries — `keyOf` reads it back out to
/// recognise this app's own work, so a code can never change once paintings carrying it
/// exist on a device. That is also why `wmc` is still here: the catalogue no longer draws
/// from Wikimedia Commons, which warrants no licence per file, but a Commons picture
/// already on a device is still this app's own by its file name.
enum MuseumSource: String, CaseIterable {
    case met, nga, cma, smk, rijks, getty, wmc

    var displayName: String {
        switch self {
        case .met: "The Metropolitan Museum of Art"
        case .nga: "National Gallery of Art, Washington"
        case .cma: "Cleveland Museum of Art"
        case .smk: "SMK – National Gallery of Denmark"
        case .rijks: "Rijksmuseum, Amsterdam"
        case .getty: "J. Paul Getty Museum"
        case .wmc: "Wikimedia Commons"
        }
    }
}

/// Recovers `{source}-{id}` from a file name `Museums` wrote, or nil if the file came from
/// somewhere else.
///
/// The key has to outlive the process so tomorrow's painting is not today's, and a
/// download already spells it into its file name; remembering it a second time would only
/// create something that could disagree with the file on disk. Favourite copies keep
/// their name precisely so this still recognises them.
func keyOf(_ fileName: String) -> String? {
    let stem = (fileName as NSString).deletingPathExtension
    guard let source = MuseumSource.allCases.first(where: { stem.hasPrefix("\($0.rawValue)-") }) else { return nil }
    let id = stem.dropFirst(source.rawValue.count + 1)
    return id.isEmpty ? nil : "\(source.rawValue)-\(id)"
}

/// One picture, ready to hang, with what a viewer would want to know about it. A file is
/// named by folder plus name rather than by absolute path, because the App Group
/// container's path is not stable across reinstalls and this is persisted.
public struct Artwork: Codable, Equatable, Hashable, Sendable {
    public enum Folder: String, Codable, Sendable { case cache, favourites }

    public let fileName: String
    public let folder: Folder
    public let title: String
    public let caption: String
    public let pageURL: URL?
    public let museum: String

    public init(fileName: String, folder: Folder, title: String, caption: String, pageURL: URL?, museum: String) {
        self.fileName = fileName
        self.folder = folder
        self.title = title
        self.caption = caption
        self.pageURL = pageURL
        self.museum = museum
    }

    public var fileURL: URL {
        let base = folder == .cache ? SharedContainer.cache : SharedContainer.favourites
        return base.appendingPathComponent(fileName)
    }

    /// The same painting, whichever folder its file sits in: the same file, or the same `keyOf` key.
    func isSamePainting(as other: Artwork) -> Bool {
        if folder == other.folder && fileName == other.fileName { return true }
        guard let key = keyOf(fileName) else { return false }
        return key == keyOf(other.fileName)
    }
}

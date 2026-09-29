// Port of android/app/src/main/java/dev/artwindow/Favourites.kt — rules must stay in step.

import Foundation

public struct Favourite: Identifiable, Hashable, Sendable {
    public var id: String { key }
    public let key: String
    public let artwork: Artwork
}

/// Durable copies of the paintings the user chose to keep, and the index describing them.
///
/// A favourite is a copy, and the copy is the whole point: the cache holds one picture and
/// deletes every other download the moment a new one arrives, so remembering a path would
/// remember a file that is already gone. The copy keeps the original file name so `keyOf`
/// still recognises it and tomorrow's painting avoids being the favourite on screen.
///
/// This folder is owned like a source owns the cache: `discardAllBut` follows the same
/// rule, and the file on the screen is never rubbish — that is what lets `forget` drop the
/// very picture in use without blanking it.
public final class Favourites: @unchecked Sendable {
    public static let shared = Favourites(directory: SharedContainer.favourites)

    private let directory: URL
    private let lock = NSLock()
    private static let indexName = "index.json"

    init(directory: URL) { self.directory = directory }

    /// Newest first.
    public func all() -> [Favourite] {
        lock.lock()
        defer { lock.unlock() }
        return ((try? read()) ?? []).reversed().map { Favourite(key: $0.key, artwork: artwork(of: $0)) }
    }

    public func contains(_ artwork: Artwork) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        // By file name whatever the folder: the copy keeps the original name, so a shown
        // cache painting reads as favourited once its copy exists.
        return ((try? read()) ?? []).contains { $0.key == artwork.fileName || $0.matches(artwork) }
    }

    @discardableResult
    public func keep(_ artwork: Artwork) throws -> Favourite {
        lock.lock()
        defer { lock.unlock() }
        var kept = try read()
        if let existing = kept.first(where: { $0.matches(artwork) }) {
            return Favourite(key: existing.key, artwork: self.artwork(of: existing))
        }

        let fm = FileManager.default
        try fm.createDirectory(at: directory, withIntermediateDirectories: true)
        let alreadyOwned = artwork.folder == .favourites
        let name = alreadyOwned ? artwork.fileName : freeName(for: artwork.fileName, among: kept)
        let copy = directory.appendingPathComponent(name)
        if !alreadyOwned { try fm.copyItem(at: artwork.fileURL, to: copy) }
        let item = Kept(
            key: name, origin: "\(artwork.folder.rawValue)/\(artwork.fileName)",
            title: artwork.title, caption: artwork.caption, pageURL: artwork.pageURL, museum: artwork.museum
        )
        kept.append(item)
        do {
            try write(kept)
        } catch {
            if !alreadyOwned { try? fm.removeItem(at: copy) }
            throw error
        }
        return Favourite(key: name, artwork: self.artwork(of: item))
    }

    /// Drops the row. The file stays until a sweep finds the desktop pointing elsewhere.
    public func forget(_ key: String) {
        lock.lock()
        defer { lock.unlock() }
        guard let kept = try? read(), kept.contains(where: { $0.key == key }) else { return }
        try? write(kept.filter { $0.key != key })
    }

    /// Deletes only unclaimed files this folder owns, sparing the picture still shown.
    /// An unreadable index deletes nothing: the sweep must never act on a list it could
    /// not read, or a damaged file would take every favourite with it.
    public func discardAllBut(shown: Artwork?) {
        lock.lock()
        defer { lock.unlock() }
        guard let kept = try? read() else { return }
        let claimed = Set(kept.map(\.key))
        let spared = shown.flatMap { $0.folder == .favourites ? $0.fileName : nil }
        let files = (try? FileManager.default.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil)) ?? []
        for file in files {
            let name = file.lastPathComponent
            if name != Favourites.indexName, !claimed.contains(name), name != spared {
                try? FileManager.default.removeItem(at: file)
            }
        }
    }

    // MARK: Index

    private struct Kept: Codable {
        let key: String
        /// "folder/fileName" of the picture this was copied from, so keeping the same
        /// painting twice finds the first copy even after its name was made unique.
        let origin: String
        let title: String
        let caption: String
        let pageURL: URL?
        let museum: String

        func matches(_ other: Artwork) -> Bool {
            let mine = Artwork(fileName: key, folder: .favourites, title: title, caption: caption, pageURL: pageURL, museum: museum)
            return mine.isSamePainting(as: other) || origin == "\(other.folder.rawValue)/\(other.fileName)"
        }
    }

    private func artwork(of kept: Kept) -> Artwork {
        Artwork(fileName: kept.key, folder: .favourites, title: kept.title, caption: kept.caption, pageURL: kept.pageURL, museum: kept.museum)
    }

    /// A missing index is an empty list; a damaged one is an error.
    private func read() throws -> [Kept] {
        let url = directory.appendingPathComponent(Favourites.indexName)
        guard FileManager.default.fileExists(atPath: url.path) else { return [] }
        let kept = try JSONDecoder().decode([Kept].self, from: Data(contentsOf: url))
        // A name with a path in it would let the index point outside this folder.
        guard kept.allSatisfy({ ($0.key as NSString).lastPathComponent == $0.key }) else {
            throw CocoaError(.fileReadCorruptFile)
        }
        return kept
    }

    private func write(_ kept: [Kept]) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let data = try JSONEncoder().encode(kept)
        try data.write(to: directory.appendingPathComponent(Favourites.indexName), options: .atomic)
    }

    /// Two pictures out of different places can share a name; the later one is numbered.
    private func freeName(for fileName: String, among kept: [Kept]) -> String {
        let ns = fileName as NSString
        let stem = ns.deletingPathExtension.isEmpty ? "painting" : ns.deletingPathExtension
        let ext = ns.pathExtension.isEmpty ? "" : ".\(ns.pathExtension)"
        var candidate = "\(stem)\(ext)"
        var number = 2
        while FileManager.default.fileExists(atPath: directory.appendingPathComponent(candidate).path)
            || kept.contains(where: { $0.key == candidate }) {
            candidate = "\(stem)-\(number)\(ext)"
            number += 1
        }
        return candidate
    }
}

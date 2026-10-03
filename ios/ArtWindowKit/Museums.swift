// Port of android/app/src/main/java/dev/artwindow/Museums.kt — TOLERANCE, MAX_ATTEMPTS,
// REQUEST_GAP_MS, Refused, the User-Agent and the cache naming must stay in step.

import Foundation

/// A museum's CDN answered 403 or 429 — a refusal aimed at this client, not a complaint
/// about one candidate, so `fetch` gives up instead of moving on to the next entry, which
/// would only keep hammering a client already being throttled and extend the block.
private struct Refused: Error { let museum: String }

/// Minimum gap between requests to one host, so a run of downloads reads as a person
/// turning pages rather than a burst a CDN blocks. Hosts pace independently, since each
/// museum's CDN enforces its own limit. An actor so two requests to the same host cannot
/// both read a stale timestamp and fire together.
private actor HostPacer {
    static let shared = HostPacer()
    private var nextFree: [String: Date] = [:]

    func wait(for host: String, gap: TimeInterval) async throws {
        let now = Date()
        let start = max(now, nextFree[host] ?? now)
        nextFree[host] = start.addingTimeInterval(gap)
        let delay = start.timeIntervalSince(now)
        if delay > 0 { try await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000)) }
    }
}

/// Downloads one painting chosen from the local catalogue — the collection is picked and
/// shape/render-filtered entirely on the device, so a turn makes exactly one HTTP request
/// for an image rather than a live search plus per-candidate lookups.
///
/// `maxAttempts` exists only because the catalogue's numbers can drift from the file
/// actually served (a museum re-encodes an image between build and download). The decoded
/// size is checked against what the catalogue promised, and a mismatch discards the file
/// and moves on rather than showing a picture that does not fit.
final class Museums: @unchecked Sendable {
    static let userAgent = "ArtWindow-iOS/0.1.0 (+https://github.com/gr13nka/art-window)"
    /// No museum here resizes on request; this is the only size control there is.
    static let maxImageBytes: Int64 = 64 * 1024 * 1024
    /// How much a downloaded image's pixel size may differ from the catalogue's figure.
    static let tolerance = 0.02
    /// Catalogue entries tried, at most, before a fetch gives up on the day.
    static let maxAttempts = 3
    static let requestGap: TimeInterval = 0.75
    /// Generous enough for an original-resolution painting on a link that has just woken up.
    static let requestTimeout: TimeInterval = 45

    private static let flushBytes = 64 * 1024
    private static let reportBytes: Int64 = 256 * 1024

    private let cacheDir: URL
    private let catalogue: Catalogue
    private let session: URLSession

    init(cacheDir: URL = SharedContainer.cache, catalogue: Catalogue = .shared) {
        self.cacheDir = cacheDir
        self.catalogue = catalogue
        // A museum's CDN can treat a client that never returns its session cookie as a
        // fresh bot on every request, so cookies are kept.
        let configuration = URLSessionConfiguration.default
        configuration.timeoutIntervalForRequest = Museums.requestTimeout
        configuration.httpCookieAcceptPolicy = .onlyFromMainDocumentDomain
        configuration.httpShouldSetCookies = true
        session = URLSession(configuration: configuration)
    }

    /// Finds a painting matching the filters and style, downloads it, and returns it.
    /// `avoid`'s file, if any, is skipped by its `keyOf` key, so a rotation does not
    /// repeat the picture on screen.
    func fetch(
        avoid: Artwork?,
        screen: Screen,
        filters: Filters,
        style: RenderStyle,
        onProgress: (FetchProgress) async -> Void
    ) async throws -> Artwork {
        let avoidKey = avoid.flatMap { keyOf($0.fileName) }
        // Catalogue.candidates already shuffles every qualifying entry, so there is no
        // per-choice draw or fallback pass left to do here. The filters are widened first
        // when they admit fewer than `Catalogue.minPool` paintings, so a thin saved choice
        // never leaves the rotation alternating between a handful.
        let wide = catalogue.widened(filters, screen: screen, style: style)
        let ordered = catalogue.candidates(wide, screen: screen, style: style)
            .filter { avoidKey == nil || $0.id != avoidKey }
        guard !ordered.isEmpty else { throw RotationError.nothingMatches }

        await onProgress(.choosing)
        var lastError: Error?
        var attempted = 0
        var downloadedAny = false
        for entry in ordered.prefix(Museums.maxAttempts) {
            attempted += 1
            do {
                let file = try await download(entry, onProgress: onProgress)
                downloadedAny = true
                if let size = Renderer.pixelSize(of: file),
                   Museums.closeEnough(size.width, entry.width), Museums.closeEnough(size.height, entry.height),
                   style.canRender(width: size.width, height: size.height, on: screen) {
                    return artwork(for: entry, file: file)
                }
                try? FileManager.default.removeItem(at: file)
            } catch let refused as Refused {
                throw RotationError.refused(museum: refused.museum)
            } catch is CancellationError {
                throw CancellationError()
            } catch {
                lastError = error
            }
        }
        // Nothing downloaded at all is a connection problem, not a catalogue that drifted.
        if !downloadedAny, let lastError {
            throw lastError as? RotationError ?? RotationError.network(lastError.localizedDescription)
        }
        throw RotationError.noneHeldUp(tried: attempted)
    }

    /// Deletes every download this source made except `keep`; a file belongs to it
    /// exactly when `keyOf` recognises its name, which is why a favourite or anything
    /// else that lands here by mistake cannot be swept away.
    func discardAllBut(keep: Artwork) {
        let files = (try? FileManager.default.contentsOfDirectory(at: cacheDir, includingPropertiesForKeys: nil)) ?? []
        for file in files where keyOf(file.lastPathComponent) != nil {
            if keep.folder == .cache && file.lastPathComponent == keep.fileName { continue }
            try? FileManager.default.removeItem(at: file)
        }
    }

    private static func closeEnough(_ actual: Int, _ catalogued: Int) -> Bool {
        abs(Double(actual - catalogued)) <= Double(catalogued) * tolerance
    }

    private func artwork(for entry: CatalogueEntry, file: URL) -> Artwork {
        let caption = [entry.byline, entry.culture].filter { !$0.isEmpty }.joined(separator: " · ")
        return Artwork(
            fileName: file.lastPathComponent,
            folder: .cache,
            title: entry.title.isEmpty ? "Untitled" : entry.title,
            caption: caption,
            pageURL: entry.pageURL,
            museum: MuseumSource(rawValue: entry.source)?.displayName ?? entry.source
        )
    }

    /// Downloads `entry`'s image to `{source}-{id}.{ext}` in the cache. The name is
    /// load-bearing: `keyOf` reads the source and id back out of it, which is how
    /// tomorrow's painting avoids being today's and how `discardAllBut` recognises this
    /// module's own downloads.
    private func download(_ entry: CatalogueEntry, onProgress: (FetchProgress) async -> Void) async throws -> URL {
        let museum = MuseumSource(rawValue: entry.source)?.displayName ?? entry.source
        try await HostPacer.shared.wait(for: entry.imageURL.host ?? "", gap: Museums.requestGap)

        var request = URLRequest(url: entry.imageURL)
        request.setValue(Museums.userAgent, forHTTPHeaderField: "User-Agent")
        let (bytes, response) = try await session.bytes(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw RotationError.network("\(entry.imageURL) returned a response that was not HTTP")
        }
        if http.statusCode == 403 || http.statusCode == 429 { throw Refused(museum: museum) }
        guard (200..<300).contains(http.statusCode) else {
            throw RotationError.network("\(entry.imageURL) returned HTTP \(http.statusCode)")
        }
        let length: Int64? = response.expectedContentLength > 0 ? response.expectedContentLength : nil
        if let length, length > Museums.maxImageBytes {
            throw RotationError.network("image is \(length) bytes, over the \(Museums.maxImageBytes)-byte limit")
        }

        try FileManager.default.createDirectory(at: cacheDir, withIntermediateDirectories: true)
        let ext = Museums.fileExtension(of: entry.imageURL)
        let file = cacheDir.appendingPathComponent("\(entry.source)-\(entry.objectId).\(ext)")
        FileManager.default.createFile(atPath: file.path, contents: nil)
        do {
            let handle = try FileHandle(forWritingTo: file)
            defer { try? handle.close() }
            var buffer = [UInt8]()
            buffer.reserveCapacity(Museums.flushBytes)
            var total: Int64 = 0
            var lastPercent = -1
            var lastReported: Int64 = 0
            for try await byte in bytes {
                buffer.append(byte)
                if buffer.count < Museums.flushBytes { continue }
                try handle.write(contentsOf: buffer)
                total += Int64(buffer.count)
                buffer.removeAll(keepingCapacity: true)
                if total > Museums.maxImageBytes {
                    throw RotationError.network("image is over the \(Museums.maxImageBytes)-byte limit")
                }
                // Throttled so the UI is not flooded: a whole-percent change when the
                // server sent Content-Length, or every ~256 KB when it did not.
                if let length {
                    let percent = Int(total * 100 / length)
                    if percent != lastPercent {
                        lastPercent = percent
                        await onProgress(.downloading(received: total, total: length))
                    }
                } else if total - lastReported >= Museums.reportBytes {
                    lastReported = total
                    await onProgress(.downloading(received: total, total: nil))
                }
            }
            try handle.write(contentsOf: buffer)
            total += Int64(buffer.count)
            await onProgress(.downloading(received: total, total: length ?? total))
        } catch {
            try? FileManager.default.removeItem(at: file)
            throw error
        }
        return file
    }

    /// The URL's own extension, at most four characters, `jpg` when it has none.
    private static func fileExtension(of url: URL) -> String {
        let ext = url.pathExtension
        return ext.isEmpty ? "jpg" : String(ext.prefix(4))
    }
}

// Port of android/app/src/main/java/dev/artwindow/Rotation.kt (the turn, the sweeps, the
// try-lock) and StatusLine.kt (the status text).

import Foundation
import Observation
import UniformTypeIdentifiers
import ImageIO
import WidgetKit

/// Where a fetch is, detailed enough for the UI to narrate it.
public enum FetchProgress: Equatable, Sendable {
    case idle
    case choosing
    case downloading(received: Int64, total: Int64?)
}

public enum RotationError: LocalizedError {
    case nothingMatches
    case refused(museum: String)
    case noneHeldUp(tried: Int)
    case network(String)

    public var errorDescription: String? {
        switch self {
        case .nothingMatches:
            "No paintings match these filters"
        case .refused(let museum):
            "\(museum) is refusing requests from this device for now — try again later"
        case .noneHeldUp(let tried):
            "None of \(tried) downloaded paintings matched the catalogue"
        case .network(let message):
            message
        }
    }
}

/// A failure that has a sentence but no case worth branching on.
struct PictureError: LocalizedError {
    let errorDescription: String?
    init(_ message: String) { errorDescription = message }
}

/// What the UI observes. Progress text is a function of the state rather than stored, so
/// it cannot disagree with it.
@MainActor @Observable
public final class RotationStatus {
    public static let shared = RotationStatus()

    public private(set) var progress: FetchProgress = .idle
    public private(set) var lastError: String?

    public var isFetching: Bool { progress != .idle }

    public var statusLine: String {
        switch progress {
        case .choosing:
            return "Choosing a painting…"
        case .downloading(let received, let total):
            let mb = 1024.0 * 1024.0
            if let total, total > 0 {
                return String(format: "Downloading %.1f of %.1f MB…", Double(received) / mb, Double(total) / mb)
            }
            return String(format: "Downloading %.1f MB…", Double(received) / mb)
        case .idle:
            return lastError ?? ""
        }
    }

    func begin() {
        lastError = nil
        progress = .choosing
    }

    func update(_ progress: FetchProgress) { self.progress = progress }

    func finish() { progress = .idle }

    func fail(_ message: String) {
        progress = .idle
        lastError = message
    }
}

/// One turn of the rotation — the only place `Museums.fetch` is ever called.
///
/// The day only advances once the picture has actually arrived (`recordFetched` runs after
/// the download), so a failed attempt leaves the day unspent for the next run to retry —
/// after a cooling-off, because without one an error would retry instantly and forever.
///
/// Everything that touches the picture or the sweeps runs one at a time: a second turn
/// awaits the first's result instead of queueing a second download, and a hand-pick waits
/// for a download in the air, since two writers racing for the cache would leave the
/// loser writing into a folder the winner's sweep had already run for.
public actor Rotation {
    public static let shared = Rotation()

    /// How long a failed fetch leaves the source alone.
    private static let coolingOff: TimeInterval = 15 * 60
    private static let widgetLongSide = 1000

    private var inFlight: Task<Artwork, Error>?

    /// Downloads a new painting (`force`) or one only if due and not cooling off, and
    /// returns nil when nothing was owed.
    public func turn(force: Bool) async throws -> Artwork? {
        if let running = inFlight { return try await running.value }
        let state = StateStore.shared.current
        if !force && (!state.isDue() || state.isCoolingOff()) { return nil }

        let task = Task { try await Rotation.fetchNewPainting() }
        inFlight = task
        defer { inFlight = nil }
        return try await task.value
    }

    /// A hand-picked painting — a favourite, or back to the day's own — takes the screen
    /// without taking the day (`recordChosen` decides whether it was owed anyway).
    ///
    /// The source's sweep spares `state.fetched`; the favourites' sweep spares
    /// `state.shown`. Handing one the other's picture is the bug that once deleted today's
    /// download: whoever wrote a file may delete it, but never the one on screen and never
    /// the one there is still a way back to.
    public func show(_ artwork: Artwork) async {
        _ = try? await inFlight?.value
        let store = StateStore.shared
        store.recordChosen(artwork)
        if let fetched = store.current.fetched {
            Museums().discardAllBut(keep: fetched)
        }
        Favourites.shared.discardAllBut(shown: artwork)
        Rotation.publishWidgetImage(of: artwork)
    }

    /// The current painting drawn for this device's wallpaper canvas, as a PNG in the
    /// temporary directory, ready to hand to the Photos or Wallpaper shortcut action.
    public func renderedWallpaper() async throws -> URL {
        _ = try? await inFlight?.value
        guard let shown = StateStore.shared.current.shown else {
            throw PictureError("There is no painting to set yet")
        }
        let style = Preferences.shared.style
        let screen = DeviceScreen.current
        return try await Task.detached {
            guard FileManager.default.fileExists(atPath: shown.fileURL.path) else {
                throw PictureError("\(shown.fileName) is no longer available")
            }
            let image = try Renderer.render(imageAt: shown.fileURL, style: style, screen: screen)
            let out = FileManager.default.temporaryDirectory.appendingPathComponent("ArtWindow-wallpaper.png")
            try Rotation.write(image, to: out, type: .png)
            return out
        }.value
    }

    private static func fetchNewPainting() async throws -> Artwork {
        let status = await RotationStatus.shared
        await status.begin()
        let store = StateStore.shared
        do {
            let before = store.current
            let museums = Museums()
            let artwork = try await museums.fetch(
                // The shown work is the one tomorrow must avoid; favourite copies keep
                // their {source}-{id} name precisely so keyOf recognises them here.
                avoid: before.shown ?? before.fetched,
                screen: DeviceScreen.current,
                filters: Preferences.shared.filters,
                style: Preferences.shared.style,
                onProgress: { await status.update($0) }
            )
            store.recordFetched(artwork)
            museums.discardAllBut(keep: artwork)
            Favourites.shared.discardAllBut(shown: artwork)
            publishWidgetImage(of: artwork)
            await status.finish()
            return artwork
        } catch {
            store.coolOff(for: coolingOff)
            await status.fail(error.localizedDescription)
            throw error
        }
    }

    /// The widget reads a small copy from the shared container rather than decoding a
    /// full painting inside its tight memory limit.
    private static func publishWidgetImage(of artwork: Artwork) {
        if let thumbnail = Renderer.thumbnail(imageAt: artwork.fileURL, maxPixel: widgetLongSide) {
            try? write(thumbnail, to: SharedContainer.widgetImage, type: .jpeg)
        }
        WidgetCenter.shared.reloadAllTimelines()
    }

    fileprivate static func write(_ image: CGImage, to url: URL, type: UTType) throws {
        guard let destination = CGImageDestinationCreateWithURL(url as CFURL, type.identifier as CFString, 1, nil) else {
            throw PictureError("Could not create \(url.lastPathComponent)")
        }
        let options: [CFString: Any] = [kCGImageDestinationLossyCompressionQuality: 0.85]
        CGImageDestinationAddImage(destination, image, options as CFDictionary)
        guard CGImageDestinationFinalize(destination) else {
            throw PictureError("Could not write \(url.lastPathComponent)")
        }
    }
}

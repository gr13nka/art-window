import AppIntents
import ArtWindowKit
import Foundation
import UniformTypeIdentifiers

/// Shortcuts gives an intent about thirty seconds; past that the automation fails
/// visibly at 00:05. A download that is still going is left running for the next
/// caller — `Rotation` hands it the same turn — and this one returns what it has.
private let turnBudget: Duration = .seconds(25)

private enum IntentFailure: Error, CustomLocalizedStringResourceConvertible {
    case nothingShownYet

    var localizedStringResource: LocalizedStringResource {
        "Art Window has no painting yet. Open the app once and let it fetch one."
    }
}

/// Resumes its continuation once, whichever of the download and the clock gets there first.
private final class Gate: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, Never>?

    init(_ continuation: CheckedContinuation<Void, Never>) { self.continuation = continuation }

    func open() {
        lock.lock()
        let pending = continuation
        continuation = nil
        lock.unlock()
        pending?.resume()
    }
}

/// A failed or slow fetch is not an error here: the current painting is still a fine
/// answer, and `Rotation` has already put the failure into its cooling-off period.
private func boundedTurn(force: Bool) async {
    await withCheckedContinuation { (done: CheckedContinuation<Void, Never>) in
        let gate = Gate(done)
        Task {
            _ = try? await Rotation.shared.turn(force: force)
            gate.open()
        }
        Task {
            try? await Task.sleep(for: turnBudget)
            gate.open()
        }
    }
}

private func wallpaper(force: Bool) async throws -> IntentFile {
    await boundedTurn(force: force)
    guard StateStore.shared.current.shown != nil else { throw IntentFailure.nothingShownYet }
    let url = try await Rotation.shared.renderedWallpaper()
    return IntentFile(fileURL: url, filename: "Art Window.png", type: .png)
}

struct GetTodaysPaintingIntent: AppIntent {
    static var title: LocalizedStringResource = "Get Today's Painting"
    static var description = IntentDescription(
        "Fetches today's painting if a new one is owed and returns it, sized for this screen, for Set Wallpaper.")
    static var openAppWhenRun = false

    func perform() async throws -> some IntentResult & ReturnsValue<IntentFile> {
        .result(value: try await wallpaper(force: false))
    }
}

struct NextPictureIntent: AppIntent {
    static var title: LocalizedStringResource = "Next Picture"
    static var description = IntentDescription("Fetches a different painting now and returns it, sized for this screen.")
    static var openAppWhenRun = false

    func perform() async throws -> some IntentResult & ReturnsValue<IntentFile> {
        .result(value: try await wallpaper(force: true))
    }
}

struct ArtWindowShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(
            intent: GetTodaysPaintingIntent(),
            phrases: ["Get today's painting from \(.applicationName)", "Today's painting in \(.applicationName)"],
            shortTitle: "Today's Painting",
            systemImageName: "photo.artframe")
        AppShortcut(
            intent: NextPictureIntent(),
            phrases: ["Next picture in \(.applicationName)"],
            shortTitle: "Next Picture",
            systemImageName: "arrow.right.circle")
    }
}

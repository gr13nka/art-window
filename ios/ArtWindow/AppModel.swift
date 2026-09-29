import ArtWindowKit
import Observation
import SwiftUI

/// What the artwork view shows, kept in step with `StateStore`, which is not observable.
/// Whoever changes the shown painting calls `reload()`; the fetch status itself comes
/// from `RotationStatus`.
@MainActor @Observable
final class AppModel {
    private(set) var shown: Artwork?
    private(set) var isFavourite = false

    init() { reload() }

    func reload() {
        shown = StateStore.shared.current.shown
        isFavourite = shown.map(Favourites.shared.contains) ?? false
    }

    /// Settles the day if it is owed; a no-op otherwise, so it is safe on every foreground.
    func turnIfDue() {
        Task {
            _ = try? await Rotation.shared.turn(force: false)
            reload()
        }
    }

    func next() {
        Task {
            _ = try? await Rotation.shared.turn(force: true)
            reload()
        }
    }

    func toggleFavourite() {
        guard let shown else { return }
        if isFavourite {
            // The copy keeps the original file name, so that is how the row is found.
            if let row = Favourites.shared.all().first(where: { $0.artwork.fileName == shown.fileName }) {
                Favourites.shared.forget(row.key)
                Favourites.shared.discardAllBut(shown: shown)
            }
        } else {
            _ = try? Favourites.shared.keep(shown)
        }
        reload()
    }
}

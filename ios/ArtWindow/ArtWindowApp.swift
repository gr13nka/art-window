import ArtWindowKit
import BackgroundTasks
import SwiftUI

/// The prefetch is only an optimisation: the automation's intent is what guarantees
/// a painting, so a refresh the system never grants costs nothing but a render.
enum RefreshSchedule {
    static let identifier = "dev.artwindow.refresh"

    /// A few minutes past local midnight, so the day has changed by the time it runs
    /// and the file is usually on disk before the 00:05 automation asks for it.
    static func schedule() {
        let request = BGAppRefreshTaskRequest(identifier: identifier)
        request.earliestBeginDate = Day.nextMidnight(after: .now).addingTimeInterval(5 * 60)
        try? BGTaskScheduler.shared.submit(request)
    }
}

@main
struct ArtWindowApp: App {
    init() {
        // Off-main callers (the intents) read the size this remembers.
        DeviceScreen.refresh()
        RefreshSchedule.schedule()
    }

    var body: some Scene {
        WindowGroup {
            RootView()
                .preferredColorScheme(.dark)
        }
        // Registers the handler with BGTaskScheduler; the identifier must match Info.plist.
        .backgroundTask(.appRefresh(RefreshSchedule.identifier)) {
            _ = try? await Rotation.shared.turn(force: false)
            RefreshSchedule.schedule()
        }
    }
}

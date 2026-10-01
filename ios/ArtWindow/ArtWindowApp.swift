import ArtWindowKit
import BackgroundTasks
import SwiftUI

/// The prefetch is only an optimisation: the automation's intent is what guarantees
/// a painting, so a refresh the system never grants costs nothing but a render.
enum RefreshSchedule {
    static let identifier = "dev.artwindow.refresh"

    /// A few minutes past the start of the day, so the day has changed by the time it
    /// runs and the file is usually on disk when the 05:05 automation asks for it.
    static func schedule() {
        let request = BGAppRefreshTaskRequest(identifier: identifier)
        request.earliestBeginDate = Day.nextStart(after: .now).addingTimeInterval(2 * 60)
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

// Port of the desktop's `Paths` (src/config.rs) and `day.rs` (there is no Kotlin counterpart for the
// container: Android keeps everything in one private app directory).

import Foundation

/// Where the app, its widget and its Shortcuts intents share files. Three processes
/// touch the same picture, state and favourites, which is why nothing lives in a
/// process-private location.
public enum SharedContainer {
    public static let groupID = "group.dev.artwindow"

    /// The App Group container, or Application Support when there is none — a unit test
    /// host has no entitlement, and failing there would only hide the code under test.
    public static var root: URL { resolvedRoot }

    private static let resolvedRoot: URL = {
        let fm = FileManager.default
        let base = fm.containerURL(forSecurityApplicationGroupIdentifier: groupID)
            ?? fm.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("ArtWindow", isDirectory: true)
        try? fm.createDirectory(at: base, withIntermediateDirectories: true)
        return base
    }()

    /// Museums' downloads. Disposable, and swept down to one picture after every change.
    public static var cache: URL { directory("cache") }

    /// Favourites' copies. Durable state, never mixed into the cache.
    public static var favourites: URL { directory("favourites") }

    /// The widget's copy of the current painting, at most 1000 px on the long side.
    public static var widgetImage: URL { root.appendingPathComponent("widget.jpg") }

    public static var defaults: UserDefaults { sharedDefaults }

    private static let sharedDefaults = UserDefaults(suiteName: groupID) ?? .standard

    private static func directory(_ name: String) -> URL {
        let url = root.appendingPathComponent(name, isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }
}

/// The whole calendar this program has: one number per instant, comparison the only
/// operation — `day::local` on the desktop, `LocalDate.toEpochDay` on Android.
///
/// A day begins at five in the morning on the wall clock, not at midnight — the same
/// rule as `day::DAY_BEGINS` on the desktop. Midnight is often still last night: a
/// painting fetched at half past twelve would be the "old" one waiting in the morning.
///
/// The number is the days-since-1970 of the *wall-clock* date the day began on, taken
/// by adding the zone's offset for that very instant to local midnight rather than for
/// now, so the hours either side of a daylight-saving change do not read as the wrong
/// day. It is deliberately independent of the calendar's era or year numbering.
public enum Day {
    /// The wall-clock hour a day begins at.
    public static let beginsAtHour = 5

    public static func local(_ date: Date, calendar: Calendar = .current) -> Int {
        var midnight = calendar.startOfDay(for: date)
        // Before five the day has not begun, so the instant belongs to the date before.
        if calendar.component(.hour, from: date) < beginsAtHour,
           let yesterday = calendar.date(byAdding: .day, value: -1, to: midnight) {
            midnight = calendar.startOfDay(for: yesterday)
        }
        let offset = calendar.timeZone.secondsFromGMT(for: midnight)
        let localSeconds = Int(midnight.timeIntervalSince1970.rounded()) + offset
        return Int((Double(localSeconds) / 86_400).rounded(.down))
    }

    /// The first instant of the next day, for a widget timeline that should turn over
    /// exactly when a new painting becomes owed.
    public static func nextStart(after date: Date, calendar: Calendar = .current) -> Date {
        calendar.nextDate(after: date, matching: DateComponents(hour: beginsAtHour), matchingPolicy: .nextTime)
            ?? date.addingTimeInterval(86_400)
    }
}

// Port of android/app/src/main/java/dev/artwindow/RotationState.kt, with the desktop's
// `cooling_off` (state.rs) added — Android leans on WorkManager's own back-off instead.

import Foundation

/// What survives the process: the day the last picture settled on, the picture fetched
/// for that day, the picture currently shown, and when a failed fetch may be retried.
///
/// `lastSuccessDay` is a calendar day (`Day.local`), never a timestamp or a countdown —
/// a device that slept past the appointed moment must still see today as owed, and
/// `isDue` is the whole of that decision.
public struct RotationState: Codable, Equatable, Sendable {
    /// The day's picture from the source; it must survive while a favourite is shown.
    public var fetched: Artwork?
    /// What the intent and the widget show now.
    public var shown: Artwork?
    public var lastSuccessDay: Int?
    public var coolingOffUntil: Date?

    public init(fetched: Artwork? = nil, shown: Artwork? = nil, lastSuccessDay: Int? = nil, coolingOffUntil: Date? = nil) {
        self.fetched = fetched
        self.shown = shown
        self.lastSuccessDay = lastSuccessDay
        self.coolingOffUntil = coolingOffUntil
    }

    public func isDue(now: Date = Date(), calendar: Calendar = .current) -> Bool {
        lastSuccessDay != Day.local(now, calendar: calendar)
    }

    public func isCoolingOff(now: Date = Date()) -> Bool {
        coolingOffUntil.map { $0 > now } ?? false
    }
}

/// `RotationState` on disk. Two methods may move `lastSuccessDay` and no others: `recordFetched`
/// always, because a picture arrived, and `recordChosen` only when `isDue` already said
/// one was owed. Each stamps the clock, remembers the picture and writes the file as one
/// operation, so there is no way to do half of it from outside.
///
/// The file is re-read on every access rather than cached: the app, the widget and the
/// Shortcuts intents are separate processes, and a cached copy would be the one that
/// undoes another's write.
public final class StateStore: @unchecked Sendable {
    public static let shared = StateStore(url: SharedContainer.root.appendingPathComponent("state.json"))

    private let url: URL
    private let calendar: Calendar
    private let lock = NSLock()

    /// `calendar` is an addition to the fixed contract, so tests can pin a time zone.
    public init(url: URL, calendar: Calendar = .current) {
        self.url = url
        self.calendar = calendar
    }

    public var current: RotationState {
        lock.lock()
        defer { lock.unlock() }
        return read()
    }

    /// The day's picture arrived: it settles the day, becomes what is shown, and ends any cooling-off.
    public func recordFetched(_ artwork: Artwork, now: Date = Date()) {
        update { state in
            state.lastSuccessDay = Day.local(now, calendar: calendar)
            state.fetched = artwork
            state.shown = artwork
            state.coolingOffUntil = nil
        }
    }

    /// A hand-picked picture takes the desktop without taking the day — unless a picture
    /// was already owed, when the choice settles it, because otherwise the overdue fetch
    /// would start seconds later and take the screen straight back.
    public func recordChosen(_ artwork: Artwork, now: Date = Date()) {
        update { state in
            if state.isDue(now: now, calendar: calendar) {
                state.lastSuccessDay = Day.local(now, calendar: calendar)
                state.coolingOffUntil = nil
            }
            state.shown = artwork
        }
    }

    /// Every failure path must call this: the day is marked done only on success, so an
    /// error with no cooling-off retries instantly and forever.
    public func coolOff(for seconds: TimeInterval, now: Date = Date()) {
        update { $0.coolingOffUntil = now.addingTimeInterval(seconds) }
    }

    private func update(_ change: (inout RotationState) -> Void) {
        lock.lock()
        defer { lock.unlock() }
        var state = read()
        change(&state)
        if let data = try? JSONEncoder().encode(state) {
            try? data.write(to: url, options: .atomic)
        }
    }

    /// A missing or unreadable file is a fresh install: due, with nothing shown.
    private func read() -> RotationState {
        guard let data = try? Data(contentsOf: url),
              let state = try? JSONDecoder().decode(RotationState.self, from: data)
        else { return RotationState() }
        return state
    }
}

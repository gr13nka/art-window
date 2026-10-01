import XCTest
@testable import ArtWindowKit

final class DayStateTests: XCTestCase {
    private var calendar: Calendar = {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "America/New_York")!
        return c
    }()

    private func date(_ iso: String) -> Date { ISO8601DateFormatter().date(from: iso)! }

    private func artwork(_ name: String, folder: Artwork.Folder = .cache) -> Artwork {
        Artwork(fileName: name, folder: folder, title: name, caption: "", pageURL: nil, museum: "Test")
    }

    private func store() -> StateStore {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("state-\(UUID().uuidString).json")
        addTeardownBlock { try? FileManager.default.removeItem(at: url) }
        return StateStore(url: url, calendar: calendar)
    }

    // MARK: Day

    func testDayChangesAtFiveInTheLocalMorningNotAtMidnight() {
        // 04:00Z is midnight in New York (EDT) and still the evening's day; 09:00Z is
        // five in the morning, when the new one begins.
        let evening = Day.local(date("2026-06-10T03:59:00Z"), calendar: calendar)
        let pastMidnight = Day.local(date("2026-06-10T04:00:00Z"), calendar: calendar)
        let before = Day.local(date("2026-06-10T08:59:00Z"), calendar: calendar)
        let after = Day.local(date("2026-06-10T09:00:00Z"), calendar: calendar)
        XCTAssertEqual(evening, pastMidnight)
        XCTAssertEqual(pastMidnight, before)
        XCTAssertEqual(after - before, 1)
        XCTAssertEqual(Day.local(date("1970-01-01T12:00:00Z"), calendar: Calendar(identifier: .gregorian).with(tz: "UTC")), 0)
    }

    func testDaylightSavingStartDoesNotSkipOrRepeatADay() {
        // 2026-03-08 has 23 hours in New York.
        let early = Day.local(date("2026-03-08T09:00:00Z"), calendar: calendar)  // 05:00 EDT
        let late = Day.local(date("2026-03-09T08:59:00Z"), calendar: calendar)   // 04:59 EDT
        let next = Day.local(date("2026-03-09T09:00:00Z"), calendar: calendar)   // 05:00 EDT
        XCTAssertEqual(early, late)
        XCTAssertEqual(next, early + 1)
    }

    func testDaylightSavingEndDoesNotSkipOrRepeatADay() {
        // 2026-11-01 has 25 hours in New York.
        let early = Day.local(date("2026-11-01T10:00:00Z"), calendar: calendar)  // 05:00 EST
        let late = Day.local(date("2026-11-02T09:59:00Z"), calendar: calendar)   // 04:59 EST
        let next = Day.local(date("2026-11-02T10:00:00Z"), calendar: calendar)   // 05:00 EST
        XCTAssertEqual(early, late)
        XCTAssertEqual(next, early + 1)
    }

    func testNextStartIsFiveInTheMorningOfTheFollowingLocalDay() {
        let next = Day.nextStart(after: date("2026-03-08T12:00:00Z"), calendar: calendar)
        XCTAssertEqual(next, date("2026-03-09T09:00:00Z"))
        // Before five, the next start is later the same date.
        XCTAssertEqual(Day.nextStart(after: date("2026-06-10T05:00:00Z"), calendar: calendar), date("2026-06-10T09:00:00Z"))
    }

    // MARK: RotationState

    func testIsDueOnlyWhenTheDateHasChanged() {
        var state = RotationState()
        let morning = date("2026-06-10T14:00:00Z")
        XCTAssertTrue(state.isDue(now: morning, calendar: calendar))
        state.lastSuccessDay = Day.local(morning, calendar: calendar)
        XCTAssertFalse(state.isDue(now: date("2026-06-11T03:00:00Z"), calendar: calendar), "still the same evening")
        XCTAssertFalse(state.isDue(now: date("2026-06-11T04:30:00Z"), calendar: calendar), "past midnight is still last night")
        XCTAssertTrue(state.isDue(now: date("2026-06-11T09:00:00Z"), calendar: calendar))
    }

    func testCoolingOffIsWallClock() {
        var state = RotationState()
        let now = date("2026-06-10T14:00:00Z")
        XCTAssertFalse(state.isCoolingOff(now: now))
        state.coolingOffUntil = now.addingTimeInterval(60)
        XCTAssertTrue(state.isCoolingOff(now: now))
        XCTAssertFalse(state.isCoolingOff(now: now.addingTimeInterval(61)))
    }

    func testRecordFetchedAlwaysStampsAndShows() {
        let store = store()
        let day1 = date("2026-06-10T14:00:00Z")
        store.coolOff(for: 900, now: day1)
        store.recordFetched(artwork("met-1.jpg"), now: day1)
        var state = store.current
        XCTAssertEqual(state.fetched, artwork("met-1.jpg"))
        XCTAssertEqual(state.shown, artwork("met-1.jpg"))
        XCTAssertEqual(state.lastSuccessDay, Day.local(day1, calendar: calendar))
        XCTAssertNil(state.coolingOffUntil, "a picture arriving ends the cooling-off")

        // Fetched again the same day (Next picture): still stamps, replaces both.
        store.recordFetched(artwork("met-2.jpg"), now: day1.addingTimeInterval(60))
        state = store.current
        XCTAssertEqual(state.fetched?.fileName, "met-2.jpg")
        XCTAssertEqual(state.shown?.fileName, "met-2.jpg")
    }

    func testRecordChosenStampsOnlyWhenDue() {
        let store = store()
        let day1 = date("2026-06-10T14:00:00Z")
        let day2 = date("2026-06-11T14:00:00Z")

        // Owed (fresh): the choice settles the day, but the day's own picture is unset.
        store.recordChosen(artwork("met-9.jpg", folder: .favourites), now: day1)
        var state = store.current
        XCTAssertEqual(state.lastSuccessDay, Day.local(day1, calendar: calendar))
        XCTAssertEqual(state.shown?.fileName, "met-9.jpg")
        XCTAssertNil(state.fetched)

        // Not owed: the clock and the fetched picture stay put.
        store.recordFetched(artwork("met-1.jpg"), now: day1)
        store.recordChosen(artwork("met-9.jpg", folder: .favourites), now: day1.addingTimeInterval(3600))
        state = store.current
        XCTAssertEqual(state.lastSuccessDay, Day.local(day1, calendar: calendar))
        XCTAssertEqual(state.fetched?.fileName, "met-1.jpg")
        XCTAssertEqual(state.shown?.fileName, "met-9.jpg")

        // Next day it is owed again, so a choice settles that day too.
        XCTAssertTrue(state.isDue(now: day2, calendar: calendar))
        store.recordChosen(artwork("met-8.jpg", folder: .favourites), now: day2)
        state = store.current
        XCTAssertEqual(state.lastSuccessDay, Day.local(day2, calendar: calendar))
        XCTAssertEqual(state.fetched?.fileName, "met-1.jpg", "a hand-pick never replaces the day's own picture")
    }

    func testStateSurvivesAReopenedStore() {
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("state-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: url) }
        StateStore(url: url, calendar: calendar).recordFetched(artwork("met-1.jpg"), now: date("2026-06-10T14:00:00Z"))
        XCTAssertEqual(StateStore(url: url, calendar: calendar).current.fetched?.fileName, "met-1.jpg")
    }

    // MARK: Cache names

    func testKeyOfRoundTripsTheDownloadName() {
        XCTAssertEqual(keyOf("met-436535.jpg"), "met-436535")
        XCTAssertEqual(keyOf("cma-1919.910.jpg"), "cma-1919.910")
        XCTAssertEqual(keyOf("smk-KMS1.jpeg"), "smk-KMS1")
        XCTAssertEqual(keyOf("wmc-File_Foo.jpg"), "wmc-File_Foo")
        XCTAssertNil(keyOf("IMG_0001.jpg"))
        XCTAssertNil(keyOf("met-.jpg"))
        XCTAssertNil(keyOf("index.json"))

        // The entry id and the file's key must agree, or tomorrow repeats today.
        let entry = Catalogue(tsv: "nga\t123\tEUROPE\t10\t10\thttps://x/y.jpg\t\tT\t\t\t").entries[0]
        XCTAssertEqual(keyOf("\(entry.source)-\(entry.objectId).jpg"), entry.id)
    }

    func testSamePaintingSpansFolders() {
        XCTAssertTrue(artwork("met-1.jpg").isSamePainting(as: artwork("met-1.jpg", folder: .favourites)))
        XCTAssertFalse(artwork("met-1.jpg").isSamePainting(as: artwork("met-2.jpg")))
        // A file with no museum key is only the same painting as itself, in the same folder.
        XCTAssertFalse(artwork("mine.jpg").isSamePainting(as: artwork("mine.jpg", folder: .favourites)))
    }
}

private extension Calendar {
    func with(tz: String) -> Calendar {
        var copy = self
        copy.timeZone = TimeZone(identifier: tz)!
        return copy
    }
}

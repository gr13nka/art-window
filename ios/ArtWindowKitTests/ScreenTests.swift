import XCTest
@testable import ArtWindowKit

final class ScreenTests: XCTestCase {
    private let phone = Screen(width: 1000, height: 2000)
    private let padSquare = Screen(width: 2000, height: 2000)

    func testHoldsWithinTrimBothWays() {
        XCTAssertTrue(phone.holds(0.5))
        XCTAssertTrue(phone.holds(0.5 * 1.15))
        XCTAssertFalse(phone.holds(1.0))
        XCTAssertFalse(phone.holds(2.0))
        // Symmetric: a landscape screen holds the mirrored proportions.
        XCTAssertTrue(Screen(width: 2000, height: 1000).holds(2.0))
        XCTAssertFalse(Screen(width: 2000, height: 1000).holds(0.5))
    }

    func testCoverCentresAndCrops() throws {
        let placement = try XCTUnwrap(padSquare.cover(width: 4000, height: 2000))
        XCTAssertEqual(placement.scaledWidth, 4000)
        XCTAssertEqual(placement.scaledHeight, 2000)
        XCTAssertEqual(placement.crop, Box(left: 1000, top: 0, right: 3000, bottom: 2000))
    }

    func testCoverRefusesEnlargementPastLimit() {
        XCTAssertNotNil(phone.cover(width: 800, height: 1600))  // exactly 1.25
        XCTAssertNil(phone.cover(width: 700, height: 1400))
        XCTAssertNil(phone.cover(width: 0, height: 1400))
    }

    func testFitLeavesBorders() throws {
        let placement = try XCTUnwrap(padSquare.fit(width: 4000, height: 2000))
        XCTAssertEqual(placement.scaledWidth, 2000)
        XCTAssertEqual(placement.scaledHeight, 1000)
        XCTAssertEqual(placement.destination, Box(left: 0, top: 500, right: 2000, bottom: 1500))
    }

    func testCanStretchFollowsTheLargerAxis() {
        XCTAssertTrue(phone.canStretch(width: 800, height: 1600))
        XCTAssertFalse(phone.canStretch(width: 700, height: 2000))
    }

    /// The iPad canvas is a square, so "shaped like the screen" means roughly square.
    func testIPadSquareCanvasPrefersNearSquarePaintings() {
        XCTAssertTrue(padSquare.holds(1.0))
        XCTAssertTrue(padSquare.holds(0.9))
        XCTAssertFalse(padSquare.holds(0.5))
        XCTAssertTrue(ArtworkShape.screen.accepts(aspect: 1.1, screen: padSquare))
        XCTAssertFalse(ArtworkShape.screen.accepts(aspect: 1.6, screen: padSquare))
        XCTAssertTrue(ArtworkShape.any.accepts(aspect: 1.6, screen: padSquare))
    }

    func testNearSquareAddsTheSideAwayFromTheScreensOwnShape() {
        // Portrait screen: near square reaches up to five by four wide.
        XCTAssertFalse(ArtworkShape.screen.accepts(aspect: 1.2, screen: phone))
        XCTAssertTrue(ArtworkShape.nearSquare.accepts(aspect: 1.2, screen: phone))
        XCTAssertFalse(ArtworkShape.nearSquare.accepts(aspect: 1.3, screen: phone))
        XCTAssertFalse(ArtworkShape.nearSquare.accepts(aspect: .nan, screen: phone))
    }
}

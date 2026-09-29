import XCTest
@testable import ArtWindowKit

final class ImageColoursTests: XCTestCase {
    func testEdgeAverageIgnoresTheMiddle() {
        // 10x10, red border band of one pixel (5% of 10 rounds up to 1), blue centre.
        var pixels = [UInt32](repeating: 0xFF00_00FF, count: 100)
        for y in 0..<10 { for x in 0..<10 where x == 0 || x == 9 || y == 0 || y == 9 { pixels[y * 10 + x] = 0xFFFF_0000 } }
        XCTAssertEqual(edgeAverageColour(pixels, width: 10, height: 10), 0xFFFF_0000)
        XCTAssertEqual(edgeAverageColour([], width: 10, height: 10), 0xFF00_0000)
    }

    func testCommonColoursAreDistinctAndMostFrequentFirst() {
        let pixels = [UInt32](repeating: 0xFF20_2020, count: 50)
            + [UInt32](repeating: 0xFF22_2222, count: 20)   // same bucket as the first
            + [UInt32](repeating: 0xFFF0_1010, count: 30)
        let colours = commonImageColours(pixels, limit: 5)
        XCTAssertEqual(colours.count, 2)
        XCTAssertEqual(colours.first.map { $0 & 0xFF }, 0x20 + 0)  // dominated by the dark grey bucket
    }
}

import XCTest
@testable import AmiscaraDeck

final class RelativeTimeTests: XCTestCase {
    func testShortTexts() {
        XCTAssertEqual(RelativeTime.short(sinceMs: 4_000, nowMs: 10_000), "6 s")
        XCTAssertEqual(RelativeTime.short(sinceMs: 20_000, nowMs: 200_000), "3 min")
        XCTAssertEqual(RelativeTime.short(sinceMs: 0, nowMs: 8_000_000), "2 Std.")
        XCTAssertEqual(RelativeTime.short(sinceMs: 0, nowMs: 200_000_000), "2 Tg.")
    }

    func testAClockThatWentBackDoesNotUnderflow() {
        XCTAssertEqual(RelativeTime.short(sinceMs: 5_000, nowMs: 0), "0 s")
    }
}

import XCTest
@testable import AmiscaraDeck

@MainActor
final class TextSizeTests: XCTestCase {
    func freshDefaults() -> UserDefaults {
        UserDefaults(suiteName: "deck.test.\(UUID().uuidString)") ?? .standard
    }

    func testNothingSavedIsNormal() {
        XCTAssertEqual(TextSizeStore(defaults: freshDefaults()).load(), .normal)
    }

    func testAChoiceIsKept() {
        let defaults = freshDefaults()
        TextSizeStore(defaults: defaults).save(.extraLarge)
        XCTAssertEqual(TextSizeStore(defaults: defaults).load(), .extraLarge, "a new start reads it again")
    }

    func testAnUnknownValueIsNormal() {
        let defaults = freshDefaults()
        defaults.set("huge", forKey: "deck.textSize")
        XCTAssertEqual(TextSizeStore(defaults: defaults).load(), .normal)
        defaults.set(42, forKey: "deck.textSize")
        XCTAssertEqual(TextSizeStore(defaults: defaults).load(), .normal)
    }

    func testTheStepsStopAtTheEnds() {
        XCTAssertEqual(TextSize.small.smaller, .small)
        XCTAssertEqual(TextSize.small.larger, .normal)
        XCTAssertEqual(TextSize.normal.larger, .large)
        XCTAssertEqual(TextSize.large.larger, .extraLarge)
        XCTAssertEqual(TextSize.extraLarge.larger, .extraLarge)
        XCTAssertEqual(TextSize.extraLarge.smaller, .large)
    }

    func testNormalIsTheSizeOfBefore() {
        XCTAssertEqual(TextSize.allCases.map(\.scale), [0.9, 1.0, 1.15, 1.3])
        XCTAssertEqual(TextSize.extraLarge.title, "Sehr groß")
    }

    func testTheModelSavesEveryChangeAtOnce() {
        let defaults = freshDefaults()
        let m = DeckModel(socketPath: "x", connector: { _, _ in throw DeckError.Failed(message: "no") }, launcher: FakeLauncher())
        m.textSizeStore = TextSizeStore(defaults: defaults)
        XCTAssertEqual(m.textSize, .normal)
        m.textSize = .large
        XCTAssertEqual(TextSizeStore(defaults: defaults).load(), .large)
        m.textSize = m.textSize.smaller
        XCTAssertEqual(TextSizeStore(defaults: defaults).load(), .normal)
    }
}

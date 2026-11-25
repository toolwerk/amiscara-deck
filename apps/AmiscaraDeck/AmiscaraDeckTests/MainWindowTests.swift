import XCTest
@testable import AmiscaraDeck

@MainActor
final class MainWindowTests: XCTestCase {
    func testTheFilterSelectsByStatusAndCounts() {
        let agents = PanelRenderTests.fixtureAgents()
        XCTAssertEqual(AgentFilter.all.count(in: agents), 5)
        XCTAssertEqual(AgentFilter.waiting.apply(to: agents).map(\.id), ["archiv"])
        XCTAssertEqual(AgentFilter.working.count(in: agents), 3)
        XCTAssertEqual(AgentFilter.done.apply(to: agents).map(\.id), ["blog"])
        XCTAssertEqual(AgentFilter.other.count(in: agents), 0)
    }

    func testOtherCollectsIdleErrorAndEnded() {
        let agents = [
            makeAgent("a", status: .idle), makeAgent("b", status: .error), makeAgent("c", status: .exited),
            makeAgent("d", status: .working),
        ]
        XCTAssertEqual(AgentFilter.other.apply(to: agents).map(\.id), ["a", "b", "c"])
    }

    func testTheFilterKeepsTheOrderOfTheList() {
        let sorted = AgentOrder.sorted(PanelRenderTests.fixtureAgents())
        XCTAssertEqual(AgentFilter.working.apply(to: sorted).map(\.id), ["api", "portal", "preis"])
    }

    func testContextAndTokenTexts() {
        var a = makeAgent("a")
        XCTAssertNil(a.contextText)
        a.contextUsed = 128_000; a.contextWindow = 1_000_000
        XCTAssertEqual(a.contextText, "128 Tsd. von 1 Mio.")
        XCTAssertEqual(AgentView.tokens(950), "950")
        XCTAssertEqual(AgentView.tokens(1_500_000), "1,5 Mio.")
    }
}

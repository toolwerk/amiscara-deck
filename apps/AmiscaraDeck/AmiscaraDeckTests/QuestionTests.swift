import SwiftUI
import XCTest
@testable import AmiscaraDeck

@MainActor
final class QuestionTests: XCTestCase {
    static func request(_ id: String = "r9") -> RequestView {
        var r = makeFullRequest(id, summary: "Wie weiter?…", truncated: true, full: "Wie weiter?", tool: "AskUserQuestion")
        r.questions = [
            QuestionView(question: "Wie weiter?", header: "Weg", multiSelect: false, options: [
                QuestionOptionView(label: "Links", description: "der kurze Weg"),
                QuestionOptionView(label: "Rechts", description: nil),
            ]),
            QuestionView(question: "Welche Früchte?", header: nil, multiSelect: true, options: [
                QuestionOptionView(label: "Apfel", description: nil),
                QuestionOptionView(label: "Birne", description: nil),
                QuestionOptionView(label: "Kirsche", description: nil),
            ]),
        ]
        return r
    }

    func connected() async -> (DeckModel, ScriptedConnector, RequestView) {
        let script = ScriptedConnector([.connect])
        let m = DeckModel(socketPath: "x", connector: script.connector, launcher: FakeLauncher())
        m.start()
        await waitUntil("observer") { script.observer != nil }
        let r = Self.request()
        script.observer?.onState(
            host: "w", settings: SettingsView(mode: .deck, waitSecs: 120, retentionDays: 7),
            agents: [makeAgent("a", project: "amiscara-deck", status: .blocked, request: r)]
        )
        await waitUntil("agent") { !m.agents.isEmpty }
        return (m, script, r)
    }

    func testEveryQuestionNeedsAChoiceBeforeAnythingIsSent() async {
        let (m, script, r) = await connected()
        XCTAssertFalse(m.canAnswer(r))
        XCTAssertFalse(m.answer(r))
        m.pick(r, question: 0, option: 0)
        XCTAssertFalse(m.canAnswer(r), "the second question is open")
        m.pick(r, question: 0, option: 1)
        XCTAssertFalse(m.isPicked(r.id, question: 0, option: 0), "single choice: the last one counts")
        m.pick(r, question: 1, option: 2)
        m.pick(r, question: 1, option: 0)
        m.pick(r, question: 1, option: 1)
        m.pick(r, question: 1, option: 1)
        XCTAssertTrue(m.answer(r))
        XCTAssertEqual(script.link?.calls, ["answer r9 [[1], [0, 2]]"])
        m.stop()
    }

    func testAQuestionIsNeverAllowedNotFromTheCardAndNotFromANotification() async {
        let (m, script, r) = await connected()
        m.expand(requestId: r.id)
        XCTAssertFalse(m.canAllow(r))
        XCTAssertFalse(m.allow(request: r))
        m.handle(action: .allow, requestId: r.id, agentId: "a")
        XCTAssertEqual(script.link?.calls, [])
        m.stop()
    }

    func testPicksOfAGoneRequestAreForgottenAndNonsenseIsIgnored() async {
        let (m, script, r) = await connected()
        m.pick(r, question: 5, option: 0)
        m.pick(r, question: 0, option: 9)
        XCTAssertTrue(m.picks.isEmpty)
        m.pick(r, question: 0, option: 0)
        script.observer?.onAgentRemoved(id: "a")
        await waitUntil("gone") { m.agents.isEmpty }
        XCTAssertTrue(m.picks.isEmpty)
        m.stop()
    }

    func testQuestionsComeOneAfterTheOtherAndGoingBackKeepsTheChoices() async {
        let (m, script, r) = await connected()
        XCTAssertEqual(m.step(r), 0)
        m.nextQuestion(r)
        XCTAssertEqual(m.step(r), 0, "no choice, no next question")
        m.pick(r, question: 0, option: 1)
        m.nextQuestion(r)
        XCTAssertEqual(m.step(r), 1)
        XCTAssertFalse(m.isStepPicked(r))
        m.previousQuestion(r)
        XCTAssertEqual(m.step(r), 0)
        XCTAssertTrue(m.isPicked(r.id, question: 0, option: 1))
        m.previousQuestion(r)
        XCTAssertEqual(m.step(r), 0)
        m.nextQuestion(r)
        m.pick(r, question: 1, option: 0)
        m.nextQuestion(r)
        XCTAssertEqual(m.step(r), 1, "the last one stays")
        XCTAssertTrue(m.answer(r))
        XCTAssertEqual(script.link?.calls, ["answer r9 [[1], [0]]"])
        script.observer?.onAgentRemoved(id: "a")
        await waitUntil("gone") { m.agents.isEmpty }
        XCTAssertTrue(m.questionSteps.isEmpty)
        m.stop()
    }

    func testThePositionIsNamed() {
        XCTAssertEqual(QuestionWording.position(0, of: 3), "Frage 1 von 3")
    }

    func testTheNotificationOffersOnlyToLook() {
        let agent = makeAgent("a", project: "amiscara-deck", status: .blocked, request: Self.request())
        let n = NotificationBuilder.make(agent: agent, kind: .held, requestId: "r9")
        XCTAssertEqual(n?.title, "amiscara-deck hat eine Frage")
        XCTAssertEqual(n?.body, "Wie weiter?")
        XCTAssertEqual(n?.actions, [.view])
    }

    func testTheCardShowsTheChoices() async {
        let (m, _, r) = await connected()
        m.pick(r, question: 0, option: 0)
        let agent = m.agents["a"]!
        let first = await RenderSupport.hosted(AgentCardView(agent: agent, nowMs: 0, model: m).frame(width: 356))
        RenderSupport.save(first.image, named: "card-answer")
        XCTAssertGreaterThan(first.size.height, 200)
        XCTAssertLessThan(first.size.height, 380, "only the first question is on the card")
        m.nextQuestion(r)
        m.pick(r, question: 1, option: 1)
        let second = await RenderSupport.hosted(AgentCardView(agent: agent, nowMs: 0, model: m).frame(width: 356))
        RenderSupport.save(second.image, named: "card-answer-2")
        m.stop()
    }
}

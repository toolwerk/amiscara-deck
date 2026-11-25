import UserNotifications
import XCTest
@testable import AmiscaraDeck

/// The texts in both languages. The test scheme runs in German
/// (`DECK_LANGUAGE=de`); these tests switch with `L10n.language`.
@MainActor
final class LanguageTests: XCTestCase {
    override func tearDown() async throws {
        L10n.language = nil
    }

    private func agent() -> AgentView {
        var a = makeAgent("api", project: "api-server", status: .blocked, request: makeFullRequest(
            summary: "rm -rf build…", truncated: true, full: "a.rs\n--- 1 old ---\nx\n--- 1 new ---\ny"
        ))
        a.request?.extra = ["dangerouslyDisableSandbox=true"]
        a.request?.headings = [1, 3]
        a.statusSinceMs = 0
        a.contextUsed = 128_000
        a.contextWindow = 1_000_000
        a.history = [
            HistoryEntryView(atMs: 0, event: .sessionStarted),
            HistoryEntryView(atMs: 0, event: .activity(activity: .run(command: "make pdf"))),
            HistoryEntryView(atMs: 0, event: .denied),
        ]
        return a
    }

    /// Most of what the app says, gathered from the places that say it.
    private func texts() -> [String] {
        let a = agent()
        let request = a.request ?? makeRequest()
        var all: [String] = []
        all += AgentFilter.allCases.map(\.title)
        all += [AgentStatus.working, .blocked, .done, .idle, .error, .exited].map { makeAgent("x", status: $0).statusLabel }
        all += ApprovalWording.all.flatMap { [ApprovalWording.title($0), ApprovalWording.caption($0)] }
        all += ["session", "localSettings", "projectSettings", "userSettings", "other", nil].map(AlwaysAllowText.target)
        all += TrayMenuEntry.allCases.map(\.title)
        all += [NotificationState.denied, .silent].compactMap(\.warning)
        all += [a.contextText ?? "", CostText.today(3_050_000), CostText.hint, RelativeTime.short(sinceMs: 0, nowMs: 7_200_000)]
        all += [RequestWording.intro(tool: "Bash"), RequestWording.terminalHint(tool: "Bash"), RequestText.full(request)]
        all += a.history.map { ActivityText.text($0.event) }
        all += [ActivityText.text(.edit(path: nil)), ActivityText.text(.web), ActivityText.text(ActivityView.compacting), ActivityText.text(HistoryEventView.compacting)]
        all += [HooksText.codexNote, HooksText.codexApprovalNote, HooksText.versionLine(.unknown), Retention.label(7)]
        all += [FocusWording.howTo, FocusWording.quietNow, QuestionWording.position(0, of: 3)]
        all += [UpdateNotice(version: "1", readyToInstall: true).text, UpdateNotice(version: "1", readyToInstall: false).button]
        for kind in [NotifyKindView.held, .inTerminal, .done] {
            var n = a
            if kind == .inTerminal { n.request?.state = .passedThrough }
            if let note = NotificationBuilder.make(agent: n, kind: kind, requestId: request.id) {
                all += [note.title, note.body]
            }
        }
        all += SystemNotifier.categories().flatMap { $0.actions.map(\.title) }
        return all
    }

    func testEnglishHasNoGermanText() {
        L10n.language = "en"
        let german = ["ä", "ö", "ü", "ß", "Ä", "Ö", "Ü", " und ", " nicht", "wartet", "Freigabe", "Sitzung", "Einstellungen", "Ablehnen", "Erlauben"]
        for text in texts() {
            for word in german {
                XCTAssertFalse(text.contains(word), "\(word) in \(text)")
            }
        }
    }

    func testEnglishReadsAsInTheMockup() {
        L10n.language = "en"
        let a = agent()
        let card = AgentCardView(agent: a, nowMs: 42_000)
        XCTAssertEqual(card.stateText, "waiting · 42 s")
        XCTAssertEqual(CostText.today(3_050_000), "Today ≈ $3.05")
        XCTAssertEqual(CostText.amount(0), "$0.00")
        XCTAssertEqual(CostText.amount(1), "< $0.01")
        XCTAssertEqual(a.contextText, "128K of 1M")
        XCTAssertEqual(AgentView.tokens(1_500_000), "1.5M")
        XCTAssertEqual(ActivityText.text(.run(command: "make pdf")), "running: make pdf")
        XCTAssertEqual(ApprovalWording.title(.deck), "In Deck")
        XCTAssertEqual(TrayMenuEntry.settings.title, "Settings …")
        XCTAssertEqual(Retention.label(1), "1 day")
        XCTAssertEqual(Retention.label(30), "30 days")
        XCTAssertEqual(RequestText.full(a.request ?? makeRequest()), "a.rs\n--- 1 old ---\nx\n--- 1 new ---\ny\n--- further details ---\ndangerouslyDisableSandbox=true")
        let actions = SystemNotifier.categories().first { $0.identifier == NotificationKind.held.category }?.actions.map(\.title)
        XCTAssertEqual(actions, ["Allow", "Deny"])
    }

    func testGermanStaysAsItWas() {
        L10n.language = "de"
        let a = agent()
        XCTAssertEqual(AgentCardView(agent: a, nowMs: 120_000).stateText, "wartet · 2 min")
        XCTAssertEqual(CostText.today(3_050_000), "Heute ≈ 3,05 $")
        XCTAssertEqual(CostText.amount(0), "0,00 $")
        XCTAssertEqual(CostText.amount(1), "< 0,01 $")
        XCTAssertEqual(a.contextText, "128 Tsd. von 1 Mio.")
        XCTAssertEqual(ActivityText.text(.run(command: "make pdf")), "führt aus: make pdf")
        XCTAssertEqual(a.history.map { ActivityText.text($0.event) }, ["Sitzung gestartet", "führt aus: make pdf", "Freigabe abgelehnt"])
        XCTAssertEqual(RelativeTime.short(sinceMs: 0, nowMs: 7_200_000), "2 Std.")
        XCTAssertEqual(Retention.label(1), "1 Tag")
        XCTAssertEqual(Retention.label(7), "7 Tage")
        XCTAssertEqual(RequestText.full(a.request ?? makeRequest()), "a.rs\n--- 1 alt ---\nx\n--- 1 neu ---\ny\n--- Weitere Angaben ---\ndangerouslyDisableSandbox=true")
        XCTAssertEqual(TrayIcon.image(waiting: true, count: 1).accessibilityDescription, "Amiscara Deck, ein Agent wartet")
    }

    /// An agent of another machine only sends data: it reads in the language
    /// of this app.
    func testActivityOfAnotherMachineReadsInTheOwnLanguage() {
        var a = makeAgent("far", project: "far", status: .working)
        a.host = "other-mac"
        a.activity = .edit(path: "src/a.rs")
        L10n.language = "en"
        XCTAssertEqual(AgentCardView(agent: a, nowMs: 0).detailLine, "main · editing src/a.rs")
        L10n.language = "de"
        XCTAssertEqual(AgentCardView(agent: a, nowMs: 0).detailLine, "main · bearbeitet src/a.rs")
    }

    func testContentThatLooksLikeAHeadingStaysAsWritten() {
        L10n.language = "de"
        var r = makeFullRequest(full: "a.md\n--- old ---\n--- new ---\nx")
        r.headings = [1]
        XCTAssertEqual(RequestText.full(r), "a.md\n--- alt ---\n--- new ---\nx")
    }

    func testPlaceholdersAreFilledOnce() {
        XCTAssertEqual(L10n.fill("{a} and {b}", ["a": "{b}", "b": "x"]), "{b} and x")
        XCTAssertEqual(L10n.fill("{unknown}", ["a": "1"]), "{unknown}")
        XCTAssertEqual(L10n.fill("open {", ["a": "1"]), "open {")
    }

    func testAnUnknownTextStaysAsItIs() {
        L10n.language = "de"
        XCTAssertEqual(L10n.t("no such text {x}", ["x": "1"]), "no such text 1")
    }
}

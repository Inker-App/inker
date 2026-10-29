import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// Find results (AppFindSearch): a flat list headed "N OF M MATCH", the
/// matched text underlined in whichever field it hit, a snippet for text hits.
struct FindViewModelTests {
    private func doc(_ code: String, _ name: String, _ kind: String, payer: String?, folder: String? = nil) -> DocItem {
        DocItem(id: code, codename: code, originalName: name, path: "/p/\(name)", kindLabel: kind, state: .ready, inked: 6,
                statusNote: nil, loader: .idle, question: nil, year: 2025, payer: payer, tag: "Taxes",
                droppedAt: Date(timeIntervalSince1970: 1_800_000_000), folderName: folder)
    }

    private func match(_ d: DocItem, _ field: MatchField, _ matched: String, snippet: String? = nil, exact: Bool = false) -> FindMatch {
        FindMatch(doc: d, field: field, matched: matched, snippet: snippet, exactHandle: exact)
    }

    @Test func headerCountsMatchesAgainstTheWholeBoard() {
        let acme = doc("w2-2025", "W2_AcmeRobotics_2025.pdf", "W-2", payer: "Acme Robotics")
        let two = FindViewModel.build(matches: [match(acme, .fileName, "Acme"), match(acme, .payer, "Acme")], total: 30,
                                      style: .octopus, showPayer: true)
        #expect(two.header == "2 OF 30 MATCH")
        let one = FindViewModel.build(matches: [match(acme, .fileName, "Acme")], total: 30, style: .octopus, showPayer: true)
        #expect(one.header == "1 OF 30 MATCHES")
        let none = FindViewModel.build(matches: [], total: 30, style: .octopus, showPayer: true)
        #expect(none.header == "0 OF 30 MATCH")
        #expect(none.rows.isEmpty)
    }

    @Test func eachHitSaysWhereToUnderline() {
        let acme = doc("w2-2025", "W2_AcmeRobotics_2025.pdf", "W-2", payer: "Acme Robotics", folder: "Taxes 2025")
        let m = FindViewModel.build(matches: [
            match(acme, .fileName, "Acme"),
            match(acme, .handle, "w2-2025", exact: true),
            match(acme, .payer, "Acme"),
            match(acme, .kind, "W-2"),
            match(acme, .text, "Wage and Tax", snippet: "Wage and Tax Statement · Employee [PERSON_1]"),
            match(acme, .folder, "Taxes 20"),
            match(acme, .tag, "Tax"),
        ], total: 30, style: .octopus, showPayer: true)
        #expect(m.rows.map(\.highlight) == [.name, .handle, .detail, .detail, .snippet, .snippet, .snippet])
        #expect(m.rows.map(\.matched) == ["Acme", "w2-2025", "Acme", "W-2", "Wage and Tax", "Taxes 20", "Tax"])
        #expect(m.rows[0].row.name == "W2_AcmeRobotics_2025.pdf")
        #expect(m.rows[0].row.handle == "🐙/w2-2025")
        #expect(m.rows[0].row.detail == "W-2 · Acme Robotics · 6 inked")
        #expect(m.rows[0].snippet == nil)
        #expect(m.rows[4].snippet == "Wage and Tax Statement · Employee [PERSON_1]")
        #expect(m.rows[4].snippetIsInkedText)
        #expect(m.rows[5].snippet == "Taxes 2025", "a folder hit shows the folder name under the row")
        #expect(!m.rows[5].snippetIsInkedText)
        #expect(m.rows[6].snippet == "Taxes")
        #expect(m.rows[1].exact)
        #expect(m.exactMatchID == nil, "an exact handle is only the located row when it is the sole result")
    }

    @Test func anExactHandleAloneIsTheLocatedRow() {
        let acme = doc("w2-2025", "W2.pdf", "W-2", payer: nil)
        let m = FindViewModel.build(matches: [match(acme, .handle, "w2-2025", exact: true)], total: 30, style: .octopus, showPayer: true)
        #expect(m.exactMatchID == "w2-2025")
    }

    @Test func rowIDsStayUniqueWhenTheSameDocumentNeverRepeats() {
        let a = doc("w2-2025", "a.pdf", "W-2", payer: nil)
        let b = doc("w2-2025-2", "b.pdf", "W-2", payer: nil)
        let m = FindViewModel.build(matches: [match(a, .fileName, "a"), match(b, .fileName, "b")], total: 2, style: .ascii, showPayer: true)
        #expect(m.rows.map(\.id) == ["w2-2025", "w2-2025-2"])
        #expect(m.rows[0].row.handle == "ink/w2-2025")
    }
}

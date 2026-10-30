import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// The ready board at scale (AppFindScale): year groups, folder sub-groups,
/// tag chips, the Find field above eight files, and the row menus.
struct BoardViewModelTests {
    private let day: TimeInterval = 86_400
    private let base = Date(timeIntervalSince1970: 1_800_000_000)

    private func doc(_ id: String, _ code: String, _ name: String, _ kind: String, year: UInt16?, payer: String? = nil,
                     tag: String? = nil, days: Double = 0, folder: String? = nil, state: DocState = .ready,
                     inked: UInt32 = 6, note: String? = nil, loader: Loader = .idle) -> DocItem {
        DocItem(id: id, codename: code, originalName: name, path: "/Users/test/Private/\(folder.map { "\($0)/" } ?? "")\(name)",
                kindLabel: kind, state: state, inked: inked, statusNote: note, loader: loader, question: nil,
                year: year, payer: payer, tag: tag, droppedAt: base.addingTimeInterval(days * day), folderName: folder)
    }

    private func folder(_ id: String, _ code: String, _ name: String, _ children: [DocItem]) -> FolderItem {
        var s = FolderSummary(total: 0, ready: 0, inking: 0, needsAttention: 0, failed: 0, inked: 0)
        for c in children {
            s = FolderSummary(total: s.total + 1, ready: s.ready + (c.state == .ready ? 1 : 0), inking: s.inking,
                              needsAttention: s.needsAttention, failed: s.failed, inked: s.inked + c.inked)
        }
        return FolderItem(id: id, codename: code, name: name, path: "/Users/test/Private/\(name)", summary: s, children: children)
    }

    private func board() -> Board {
        let f25 = folder("f-1", "folder-0412", "Taxes 2025", (1...9).map {
            doc("c-\($0)", $0 == 1 ? "w2-2025-3" : "w2-2025-\($0 + 2)", "W2_copy_\($0).pdf", "W-2", year: 2025,
                payer: "Acme Robotics", tag: "Taxes", days: 10, folder: "Taxes 2025", inked: $0 == 9 ? 1 : 5)
        })
        let g25 = YearGroup(year: 2025, files: 14, entries: [
            .doc(item: doc("d-1", "w2-2025", "W2_AcmeRobotics_2025.pdf", "W-2", year: 2025, payer: "Acme Robotics", tag: "Taxes", days: 20)),
            .doc(item: doc("d-2", "w2-2025-2", "w2-bluefin.pdf", "W-2", year: 2025, payer: "Bluefin Analytics", tag: "Taxes", days: 19)),
            .doc(item: doc("d-3", "1099-int-2025", "IMG_4471.heic", "1099-INT", year: 2025, payer: "Harbor Street CU", tag: "Taxes", days: 18, inked: 4)),
            .doc(item: doc("d-4", "bank-statement-2025", "statement.pdf", "bank statement", year: 2025, tag: "Banking", days: 17, inked: 7)),
            .doc(item: doc("d-5", "receipt-4471", "IMG_0001.heic", "receipt", year: 2025, days: 16, inked: 2)),
            .folder(item: f25),
        ])
        let g24 = YearGroup(year: 2024, files: 1, entries: [
            .doc(item: doc("d-6", "w2-2024", "w2-acme-2024.pdf", "W-2", year: 2024, payer: "Acme Robotics", tag: "Taxes", days: -300)),
        ])
        let undated = YearGroup(year: nil, files: 1, entries: [
            .doc(item: doc("d-7", "scan-4472", "IMG_0002.heic", "scan", year: nil, days: -5, inked: 0)),
        ])
        return Board(groups: [g25, g24, undated],
                     tags: [TagCount(tag: "Taxes", count: 13), TagCount(tag: "Banking", count: 1)], total: 16)
    }

    private func build(showPayer: Bool = true, tag: String? = nil, openYears: Set<String> = ["2025"],
                       openFolders: Set<String> = []) -> BoardModel {
        BoardViewModel.build(board: board(), style: .octopus, showPayer: showPayer, activeTag: tag,
                             isYearOpen: { openYears.contains($0) }, isFolderOpen: { openFolders.contains($0) })
    }

    @Test func yearGroupsCarryTheirLabelsAndOpenState() {
        let m = build()
        #expect(m.total == 16)
        #expect(m.showsFind, "more than eight files")
        #expect(m.groups.map(\.label) == ["2025 · 14 FILES", "2024 · 1 FILE", "UNDATED · 1 FILE"])
        #expect(m.groups.map(\.id) == ["2025", "2024", "undated"])
        #expect(m.groups.map(\.isOpen) == [true, false, false])
        #expect(m.groups[1].rows.count == 1, "rows are built even when collapsed")
    }

    @Test func aReadyRowIsNameThenHandleTypePayerAndInked() {
        let m = build()
        guard case .doc(let row) = m.groups[0].rows[1] else { Issue.record("a doc row"); return }
        #expect(row.name == "w2-bluefin.pdf")
        #expect(row.handle == "🐙/w2-2025-2")
        #expect(row.detail == "W-2 · Bluefin Analytics · 6 inked")
        #expect(row.copyEnabled)
        #expect(row.path == "/Users/test/Private/w2-bluefin.pdf")
        #expect(!row.indented)
        #expect(row.menu.copyOctopus == "Copy 🐙/w2-2025-2")
        #expect(row.menu.copyAscii == "Copy ink/w2-2025-2")
        #expect(row.menu.asciiHandle == "ink/w2-2025-2")
        #expect(row.menu.forget == "Forget this file")
        #expect(row.menu.inkAgain == "Ink again")
        #expect(row.menu.showInFinder == "Show in Finder")
    }

    @Test func thePayerIsLeftOutWhenHiddenOrMissing() {
        let hidden = build(showPayer: false)
        guard case .doc(let row) = hidden.groups[0].rows[1] else { Issue.record("a doc row"); return }
        #expect(row.detail == "W-2 · 6 inked")
        let shown = build()
        guard case .doc(let bank) = shown.groups[0].rows[3] else { Issue.record("a doc row"); return }
        #expect(bank.detail == "bank statement · 7 inked", "no payer on a bank statement")
    }

    @Test func theAsciiStyleChangesEveryHandle() {
        let m = BoardViewModel.build(board: board(), style: .ascii, showPayer: true, activeTag: nil,
                                     isYearOpen: { _ in true }, isFolderOpen: { _ in false })
        guard case .doc(let row) = m.groups[0].rows[0] else { Issue.record("a doc row"); return }
        #expect(row.handle == "ink/w2-2025")
        #expect(row.menu.copyOctopus == "Copy 🐙/w2-2025", "the menu always offers both")
        #expect(row.menu.copyAscii == "Copy ink/w2-2025")
    }

    @Test func aFolderIsOneRowCollapsedByDefaultWithItsOwnCopyAndMenu() {
        let m = build()
        guard case .folder(let f) = m.groups[0].rows[5] else { Issue.record("a folder row"); return }
        #expect(f.name == "Taxes 2025")
        #expect(f.countLine == "· 9 files")
        #expect(f.handle == "🐙/folder-0412")
        #expect(f.summary == "all ready · 41 inked")
        #expect(f.copyEnabled)
        #expect(!f.isOpen)
        #expect(f.children.count == 9)
        #expect(f.children.allSatisfy { $0.indented })
        #expect(f.menu.forget == "Forget this folder")
        #expect(f.menu.copyOctopus == "Copy 🐙/folder-0412")
        let open = build(openFolders: ["f-1"])
        guard case .folder(let o) = open.groups[0].rows[5] else { Issue.record("a folder row"); return }
        #expect(o.isOpen)
    }

    @Test func theMenuTargetIsFoundOnTheBoardInsideFoldersAndInResults() {
        let m = build()
        #expect(m.menuTarget(for: "d-2")?.path == "/Users/test/Private/w2-bluefin.pdf")
        #expect(m.menuTarget(for: "f-1")?.menu.forget == "Forget this folder")
        #expect(m.menuTarget(for: "c-3")?.menu.copyOctopus == "Copy 🐙/w2-2025-5", "a folder's child")
        #expect(m.menuTarget(for: "d-6")?.path == "/Users/test/Private/w2-acme-2024.pdf", "a collapsed year")
        #expect(m.menuTarget(for: "nope") == nil)
    }

    @Test func chipsListAllThenEachTagWithItsCount() {
        let m = build()
        #expect(m.chips.map(\.title) == ["All", "Taxes · 13", "Banking · 1"])
        #expect(m.chips.map(\.isActive) == [true, false, false])
        let banking = build(tag: "Banking")
        #expect(banking.chips.map(\.isActive) == [false, false, true])
        #expect(banking.chips[2].tag == "Banking")
        #expect(banking.chips[0].tag == nil)
    }

    @Test func fewFilesHideTheFindField() {
        let small = Board(groups: [board().groups[1]], tags: [], total: 8)
        let m = BoardViewModel.build(board: small, style: .octopus, showPayer: true, activeTag: nil,
                                     isYearOpen: { _ in true }, isFolderOpen: { _ in false })
        #expect(!m.showsFind, "eight files or fewer are short enough to scan")
        #expect(m.chips.isEmpty, "no chips without a tagged file")
    }

    @Test func aProcessingOrFailedChildKeepsItsNameAndNote() {
        var b = board()
        let reading = doc("d-8", "doc-1001", "scan.heic", "document", year: nil, days: 30, state: .reading, inked: 0, note: "reading the scan", loader: .ocr)
        let failed = doc("d-9", "doc-1002", "broken.pdf", "document", year: nil, days: 29, state: .failed, inked: 0, note: "could not read this file")
        let typed = doc("d-10", "doc-1003", "w2.pdf", "W-2", year: 2025, days: 28, state: .inking, inked: 0, note: "inking", loader: .read)
        b = Board(groups: [YearGroup(year: nil, files: 3, entries: [.doc(item: reading), .doc(item: failed), .doc(item: typed)])], tags: [], total: 3)
        let m = BoardViewModel.build(board: b, style: .octopus, showPayer: true, activeTag: nil,
                                     isYearOpen: { _ in true }, isFolderOpen: { _ in false })
        guard case .doc(let r) = m.groups[0].rows[0], case .doc(let f) = m.groups[0].rows[1], case .doc(let t) = m.groups[0].rows[2] else {
            Issue.record("doc rows"); return
        }
        #expect(r.state == .processing(.ocr))
        #expect(r.detail == "reading the scan")
        #expect(!r.copyEnabled)
        #expect(f.state == .failed)
        #expect(f.detail == "could not read this file")
        #expect(t.detail == "W-2 · inking", "the type shows once classified, before the row is ready")
    }
}

import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// The ready board as a tree of the private folder: folder rows with a
/// chevron, open or collapsed, children one 12-point step in per level,
/// the Find field above eight files, and the row menus.
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

    /// A folder whose summary covers everything under it, as the core sends it.
    private func folder(_ id: String, _ code: String?, _ relPath: String, folders: [FolderItem] = [],
                        files: [DocItem]) -> FolderItem {
        var s = FolderSummary(total: 0, ready: 0, inking: 0, needsAttention: 0, failed: 0, inked: 0)
        for c in files {
            s = FolderSummary(total: s.total + 1, ready: s.ready + (c.state == .ready ? 1 : 0), inking: s.inking,
                              needsAttention: s.needsAttention, failed: s.failed, inked: s.inked + c.inked)
        }
        for f in folders {
            let t = f.summary
            s = FolderSummary(total: s.total + t.total, ready: s.ready + t.ready, inking: s.inking + t.inking,
                              needsAttention: s.needsAttention + t.needsAttention, failed: s.failed + t.failed,
                              inked: s.inked + t.inked)
        }
        let name = relPath.split(separator: "/").last.map(String.init) ?? ""
        return FolderItem(id: id, codename: code, name: name, relPath: relPath,
                          path: "/Users/test/Private\(relPath.isEmpty ? "" : "/\(relPath)")", summary: s,
                          folders: folders, files: files)
    }

    /// ~/Private: Archive/2023 (only paths), Taxes 2025 (dropped, with a
    /// Receipts sub-folder), then seven loose files. Already in Finder order,
    /// as the core sends it.
    private func board() -> Board {
        let archive = folder("dir:Archive", nil, "Archive", folders: [
            folder("dir:Archive/2023", nil, "Archive/2023", files: [
                doc("d-8", "1040-2023", "1040.pdf", "1040", year: 2023, folder: "Archive/2023"),
            ]),
        ], files: [])
        let receipts = folder("dir:Taxes 2025/Receipts", nil, "Taxes 2025/Receipts", files: [
            doc("r-1", "receipt-0001", "r1.heic", "receipt", year: 2025, folder: "Taxes 2025/Receipts", inked: 3),
        ])
        let taxes = folder("f-1", "folder-0412", "Taxes 2025", folders: [receipts], files: (1...9).map {
            doc("c-\($0)", $0 == 1 ? "w2-2025-3" : "w2-2025-\($0 + 2)", "W2_copy_\($0).pdf", "W-2", year: 2025,
                payer: "Acme Robotics", tag: "Taxes", days: 10, folder: "Taxes 2025", inked: $0 == 9 ? 1 : 5)
        })
        let root = folder("dir:", nil, "", folders: [archive, taxes], files: [
            doc("d-1", "w2-2025", "W2_AcmeRobotics_2025.pdf", "W-2", year: 2025, payer: "Acme Robotics", tag: "Taxes", days: 20),
            doc("d-2", "w2-2025-2", "w2-bluefin.pdf", "W-2", year: 2025, payer: "Bluefin Analytics", tag: "Taxes", days: 19),
            doc("d-3", "1099-int-2025", "IMG_4471.heic", "1099-INT", year: 2025, payer: "Harbor Street CU", tag: "Taxes", days: 18, inked: 4),
            doc("d-4", "bank-statement-2025", "statement.pdf", "bank statement", year: 2025, tag: "Banking", days: 17, inked: 7),
            doc("d-5", "receipt-4471", "IMG_0001.heic", "receipt", year: 2025, days: 16, inked: 2),
            doc("d-6", "w2-2024", "w2-acme-2024.pdf", "W-2", year: 2024, payer: "Acme Robotics", tag: "Taxes", days: -300),
            doc("d-7", "scan-4472", "IMG_0002.heic", "scan", year: nil, days: -5, inked: 0),
        ])
        return Board(root: root, total: 18)
    }

    private func build(showPayer: Bool = true, style: HandleStyle = .octopus, open: Set<String> = []) -> BoardModel {
        BoardViewModel.build(board: board(), style: style, showPayer: showPayer, isFolderOpen: { open.contains($0) })
    }

    private func doc(_ m: BoardModel, _ id: String) -> DocRowModel? {
        for case .doc(let d) in m.rows where d.id == id { return d }
        return nil
    }

    private func folder(_ m: BoardModel, _ id: String) -> FolderRowModel? {
        for case .folder(let f) in m.rows where f.id == id { return f }
        return nil
    }

    @Test func theRowsMirrorTheTreeFoldersFirstAndCollapsed() {
        let m = build()
        #expect(m.total == 18)
        #expect(m.showsFind, "more than eight files")
        #expect(m.rows.map(\.id) == ["dir:Archive", "f-1", "d-1", "d-2", "d-3", "d-4", "d-5", "d-6", "d-7"])
        #expect(m.rows.allSatisfy { $0.depth == 0 })
        #expect(folder(m, "f-1")?.isOpen == false, "folders start collapsed")
        #expect(folder(m, "dir:Archive")?.isOpen == false)
    }

    @Test func anOpenFolderShowsItsSubFoldersThenItsFilesOneStepIn() {
        let m = build(open: ["Taxes 2025", "Taxes 2025/Receipts", "Archive/2023"])
        var expected: [String] = ["dir:Archive", "f-1", "dir:Taxes 2025/Receipts", "r-1"]
        expected.append(contentsOf: (1...9).map { "c-\($0)" })
        expected.append(contentsOf: ["d-1", "d-2", "d-3", "d-4", "d-5", "d-6", "d-7"])
        #expect(m.rows.map(\.id) == expected, "Archive/2023 stays hidden while Archive is shut")
        #expect(folder(m, "f-1")?.isOpen == true)
        #expect(folder(m, "dir:Taxes 2025/Receipts")?.depth == 1)
        #expect(folder(m, "dir:Taxes 2025/Receipts")?.indent == 12)
        #expect(doc(m, "r-1")?.depth == 2)
        #expect(doc(m, "r-1")?.indent == 24)
        #expect(doc(m, "c-1")?.indent == 12)
        #expect(doc(m, "d-1")?.indent == 0)
        #expect(BoardViewModel.indentStep == 12)
    }

    @Test func aReadyRowIsNameThenHandleTypeYearPayerAndInked() {
        let m = build()
        guard let row = doc(m, "d-2") else { Issue.record("a doc row"); return }
        #expect(row.name == "w2-bluefin.pdf")
        #expect(row.handle == "🐙/w2-2025-2")
        #expect(row.detail == "W-2 · 2025 · Bluefin Analytics · 6 inked")
        #expect(row.copyEnabled)
        #expect(row.path == "/Users/test/Private/w2-bluefin.pdf")
        #expect(row.menu.copyOctopus == "Copy 🐙/w2-2025-2")
        #expect(row.menu.copyAscii == "Copy ink/w2-2025-2")
        #expect(row.menu.asciiHandle == "ink/w2-2025-2")
        #expect(row.menu.forget == "Forget this file")
        #expect(row.menu.inkAgain == "Ink again")
        #expect(row.menu.showInFinder == "Show in Finder")
        #expect(doc(m, "d-7")?.detail == "scan · 0 inked", "no year, nothing in its place")
    }

    @Test func thePayerIsLeftOutWhenHiddenOrMissing() {
        #expect(doc(build(showPayer: false), "d-2")?.detail == "W-2 · 2025 · 6 inked")
        #expect(doc(build(), "d-4")?.detail == "bank statement · 2025 · 7 inked", "no payer on a bank statement")
    }

    @Test func theAsciiStyleChangesEveryHandle() {
        let m = build(style: .ascii)
        #expect(doc(m, "d-1")?.handle == "ink/w2-2025")
        #expect(doc(m, "d-1")?.menu.copyOctopus == "Copy 🐙/w2-2025", "the menu always offers both")
        #expect(doc(m, "d-1")?.menu.copyAscii == "Copy ink/w2-2025")
        #expect(folder(m, "f-1")?.handle == "ink/folder-0412")
    }

    @Test func aDroppedFolderHasItsHandleSummaryCopyAndMenu() {
        guard let f = folder(build(), "f-1") else { Issue.record("a folder row"); return }
        #expect(f.name == "Taxes 2025")
        #expect(f.countLine == "· 10 files", "everything under it")
        #expect(f.handle == "🐙/folder-0412")
        #expect(f.summary == "all ready · 44 inked")
        #expect(f.copyEnabled)
        #expect(f.relPath == "Taxes 2025")
        #expect(f.path == "/Users/test/Private/Taxes 2025")
        #expect(f.menu?.forget == "Forget this folder")
        #expect(f.menu?.copyOctopus == "Copy 🐙/folder-0412")
    }

    @Test func aFolderThatIsOnlyAPathHasANameAndACountButNoHandle() {
        guard let f = folder(build(), "dir:Archive") else { Issue.record("a folder row"); return }
        #expect(f.name == "Archive")
        #expect(f.countLine == "· 1 file")
        #expect(f.handle == nil)
        #expect(f.menu == nil)
        #expect(!f.copyEnabled)
    }

    @Test func theMenuTargetIsFoundAmongTheRowsShown() {
        let m = build(open: ["Taxes 2025"])
        #expect(m.menuTarget(for: "d-2")?.path == "/Users/test/Private/w2-bluefin.pdf")
        #expect(m.menuTarget(for: "f-1")?.menu.forget == "Forget this folder")
        #expect(m.menuTarget(for: "c-3")?.menu.copyOctopus == "Copy 🐙/w2-2025-5", "a folder's child")
        #expect(m.menuTarget(for: "dir:Archive") == nil, "no handle, no menu")
        #expect(build().menuTarget(for: "c-3") == nil, "inside a shut folder")
        #expect(m.menuTarget(for: "nope") == nil)
    }

    @Test func locatingARowOpensEveryFolderAboveIt() {
        let b = board()
        #expect(BoardViewModel.pathsToOpen(for: "r-1", in: b) == ["Taxes 2025", "Taxes 2025/Receipts"])
        #expect(BoardViewModel.pathsToOpen(for: "d-8", in: b) == ["Archive", "Archive/2023"])
        #expect(BoardViewModel.pathsToOpen(for: "c-4", in: b) == ["Taxes 2025"])
        #expect(BoardViewModel.pathsToOpen(for: "f-1", in: b) == ["Taxes 2025"], "a folder opens itself")
        #expect(BoardViewModel.pathsToOpen(for: "d-1", in: b) == [String](), "a file at the root needs nothing open")
        #expect(BoardViewModel.pathsToOpen(for: "nope", in: b) == nil)
    }

    @Test func fewFilesHideTheFindField() {
        let small = Board(root: folder("dir:", nil, "", files: [doc("d-1", "w2-2025", "w2.pdf", "W-2", year: 2025)]), total: 8)
        let m = BoardViewModel.build(board: small, style: .octopus, showPayer: true, isFolderOpen: { _ in false })
        #expect(!m.showsFind, "eight files or fewer are short enough to scan")
        #expect(m.rows.map(\.id) == ["d-1"])
    }

    @Test func aProcessingOrFailedRowKeepsItsNameAndNote() {
        let reading = doc("d-8", "doc-1001", "scan.heic", "document", year: nil, days: 30, state: .reading, inked: 0, note: "reading the scan", loader: .ocr)
        let failed = doc("d-9", "doc-1002", "broken.pdf", "document", year: nil, days: 29, state: .failed, inked: 0, note: "could not read this file")
        let typed = doc("d-10", "doc-1003", "w2.pdf", "W-2", year: 2025, days: 28, state: .inking, inked: 0, note: "inking", loader: .read)
        let b = Board(root: folder("dir:", nil, "", files: [reading, failed, typed]), total: 3)
        let m = BoardViewModel.build(board: b, style: .octopus, showPayer: true, isFolderOpen: { _ in false })
        guard let r = doc(m, "d-8"), let f = doc(m, "d-9"), let t = doc(m, "d-10") else {
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

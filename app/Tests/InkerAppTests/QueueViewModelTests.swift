import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// Backend records to popover state, rows and copy.
struct QueueViewModelTests {
    private let listening = Status(mcpListening: true, privateFolderDisplay: "~/Private")
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    private func doc(_ id: String, _ codename: String, _ name: String, _ kind: String, _ state: DocState,
                     inked: UInt32 = 0, note: String? = nil, loader: Loader = .idle, question: ReviewQuestion? = nil) -> DocItem {
        DocItem(id: id, codename: codename, originalName: name, path: "/Users/test/Private/taxes-2025/\(name)", kindLabel: kind, state: state, inked: inked,
                statusNote: note, loader: loader, question: question, year: 2025, payer: nil, tag: "Taxes", droppedAt: now,
                folderName: "taxes-2025")
    }

    private func folder(_ children: [DocItem]) -> QueueItem {
        var s = FolderSummary(total: 0, ready: 0, inking: 0, needsAttention: 0, failed: 0, inked: 0)
        var total: UInt32 = 0, ready: UInt32 = 0, inking: UInt32 = 0, held: UInt32 = 0, failed: UInt32 = 0, inked: UInt32 = 0
        for c in children {
            total += 1
            inked += c.inked
            switch c.state {
            case .ready: ready += 1
            case .needsAttention: held += 1
            case .failed: failed += 1
            case .queued, .reading, .inking: inking += 1
            }
        }
        s = FolderSummary(total: total, ready: ready, inking: inking, needsAttention: held, failed: failed, inked: inked)
        return QueueItem(id: "g-1", codename: "taxes-2025", isFolder: true, summary: s, children: children)
    }

    private func build(_ queue: [QueueItem], reads: [ReadEvent] = [], lastDrop: String? = nil, style: HandleStyle = .octopus) -> PopoverModel {
        QueueViewModel.build(queue: queue, status: listening, reads: reads, lastDrop: lastDrop, style: style, now: now)
    }

    private var question: ReviewQuestion {
        ReviewQuestion(id: "q-4", prompt: "Looks like an account number, but the scan is rough. Ink it?",
                       location: "page 3 of 11", pageIndex: 2, contextBefore: "…wire to", valueLength: 9,
                       span: TextSpan(start: 120, end: 129))
    }

    // MARK: modes

    @Test func emptyQueueIsTheEmptyBoard() {
        let m = build([])
        #expect(m.mode == .empty)
        #expect(m.badgeCount == 0)
        #expect(m.footer.left == .text("Nothing filed yet"))
        #expect(m.footer.right == "MCP running")
    }

    @Test func processingBoard() {
        let m = build([folder([
            doc("1", "w2-acme-2025", "W2.pdf", "W-2", .ready, inked: 6),
            doc("2", "1099-int-northwind-2025", "1099-INT_northwind.pdf", "1099-INT", .reading, note: "reading", loader: .read),
            doc("3", "receipt-4471", "IMG_4471.heic", "scan", .reading, note: "reading the scan", loader: .ocr),
            doc("4", "closing-statement-2025", "closing-statement.pdf", "closing statement", .queued, note: "queued", loader: .queued),
        ])], lastDrop: "Moved 4 files from ~/Downloads")
        #expect(m.mode == .processing)
        let g = m.groups[0]
        #expect(g.handle == "🐙/taxes-2025")
        #expect(g.summaryLine == "4 files · 1 ready · 3 inking")
        #expect(g.copyEnabled, "decision 5.5: a folder copies once one child is ready")
        #expect(g.rows.map(\.kind) == [.ready, .processing(.read), .processing(.ocr), .processing(.queued)])
        #expect(g.rows[0].title == "🐙/w2-acme-2025")
        #expect(g.rows[0].detail == "W-2 · 6 inked")
        #expect(g.rows[0].copyEnabled)
        #expect(g.rows[1].title == "1099-INT_northwind.pdf")
        #expect(g.rows[1].detail == "1099-INT · reading", "the type shows once classified")
        #expect(!g.rows[1].copyEnabled)
        #expect(g.rows[3].detail == "closing statement · queued")
        #expect(m.footer.left == .text("Moved 4 files from ~/Downloads"))
        #expect(m.footer.right == "4 files · MCP running")
    }

    @Test func attentionBoard() {
        let m = build([folder([
            doc("4", "closing-statement-2025", "closing-statement.pdf", "closing statement", .needsAttention, inked: 10, note: "page 3 of 11", question: question),
            doc("1", "w2-acme-2025", "W2.pdf", "W-2", .ready, inked: 6),
            doc("2", "1099-int-northwind-2025", "x.pdf", "1099-INT", .ready, inked: 4),
            doc("3", "receipt-4471", "IMG.heic", "scan", .ready, inked: 2),
        ])])
        #expect(m.mode == .attention)
        #expect(m.badgeCount == 1)
        let g = m.groups[0]
        #expect(g.summaryLine == "4 files · 3 ready · 1 needs you")
        let card = g.attention!
        #expect(card.fileName == "closing-statement.pdf")
        #expect(card.handle == "🐙/closing-statement-2025", "the pending handle sits under the name")
        #expect(card.filePath == "/Users/test/Private/taxes-2025/closing-statement.pdf", "Open file reveals it")
        #expect(card.location == "page 3 of 11")
        #expect(card.prompt == "Looks like an account number, but the scan is rough. Ink it?")
        #expect(card.questionID == "q-4")
        #expect(card.valueLength == 9)
        #expect(card.pageIndex == 2 && card.span == 120..<129, "the page crop knows where to look")
        #expect(card.footnote == "Handle is held until you answer. Agents asking for it are told \"pending\".")
        #expect(g.rows.map(\.kind) == [.ready, .ready, .ready], "the held file is the card, not a row")
        #expect(g.rows.map(\.detail) == ["ready", "ready", "ready"], "compact rows on the attention board")
        #expect(m.footer.left == .text("1 waiting on you"))
    }

    @Test func readyBoard() {
        let read = ReadEvent(agent: "Claude Code", codename: "w2-acme-2025", at: now.addingTimeInterval(-120))
        let m = build([folder([
            doc("1", "w2-acme-2025", "W2.pdf", "W-2", .ready, inked: 6),
            doc("2", "1099-int-northwind-2025", "x.pdf", "1099-INT", .ready, inked: 4),
            doc("3", "receipt-4471", "IMG.heic", "scan", .ready, inked: 2),
            doc("4", "closing-statement-2025", "c.pdf", "closing statement", .ready, inked: 11),
        ])], reads: [read])
        #expect(m.mode == .ready)
        #expect(m.groups[0].summaryLine == "4 files · all ready · 23 inked")
        #expect(m.groups[0].rows.map(\.detail) == ["W-2 · 6 inked", "1099-INT · 4 inked", "scan · 2 inked", "closing statement · 11 inked"])
        #expect(m.footer.left == .lastRead(agent: "Claude Code", handle: "🐙/w2-acme-2025", ago: "2m"))
        #expect(m.footer.showsPeeker)
    }

    @Test func attentionWinsOverProcessingAcrossGroups() {
        let single = QueueItem(id: "g-2", codename: "notes", isFolder: false,
                               summary: FolderSummary(total: 1, ready: 0, inking: 1, needsAttention: 0, failed: 0, inked: 0),
                               children: [doc("9", "notes", "notes.pdf", "document", .reading, loader: .read)])
        let held = folder([doc("4", "c", "c.pdf", "x", .needsAttention, question: question)])
        #expect(build([single, held]).mode == .attention)
        #expect(build([single]).mode == .processing)
    }

    // MARK: copy and handles

    @Test func folderCopyIsDisabledUntilAChildIsReady() {
        let g = build([folder([doc("2", "a", "a.pdf", "x", .reading, loader: .read)])]).groups[0]
        #expect(!g.copyEnabled)
    }

    @Test func singleFileGroupUsesItsOwnHandle() {
        let single = QueueItem(id: "g-2", codename: "w2-acme-2025", isFolder: false,
                               summary: FolderSummary(total: 1, ready: 1, inking: 0, needsAttention: 0, failed: 0, inked: 6),
                               children: [doc("1", "w2-acme-2025", "W2.pdf", "W-2", .ready, inked: 6)])
        let g = build([single], style: .ascii).groups[0]
        #expect(g.handle == "ink/w2-acme-2025")
        #expect(g.summaryLine == "W-2 · 6 inked")
        #expect(g.copyEnabled)
    }

    /// Ben saw "🐙/tax-form-2023" twice, the second indented: the single-file
    /// group drew a header and then the same document as a row.
    @Test func singleFileGroupIsOneRowNotTwo() {
        let ready = QueueItem(id: "g-2", codename: "w2-acme-2025", isFolder: false,
                              summary: FolderSummary(total: 1, ready: 1, inking: 0, needsAttention: 0, failed: 0, inked: 6),
                              children: [doc("1", "w2-acme-2025", "W2.pdf", "W-2", .ready, inked: 6)])
        let g = build([ready]).groups[0]
        #expect(g.showsHeader)
        #expect(g.rows.isEmpty, "the header is the row")

        let reading = QueueItem(id: "g-3", codename: "notes", isFolder: false,
                                summary: FolderSummary(total: 1, ready: 0, inking: 1, needsAttention: 0, failed: 0, inked: 0),
                                children: [doc("2", "notes", "notes.pdf", "x", .reading, loader: .read)])
        let pm = build([reading])
        let p = pm.groups[0]
        #expect(!p.showsHeader, "a file still being read shows its name and loader, once")
        #expect(p.rows.count == 1)
        #expect(p.rows[0].title == "notes.pdf")
        #expect(pm.footer.right == "1 file · MCP running")

        let folder = build([folder([doc("3", "a", "a.pdf", "W-2", .ready, inked: 2)])]).groups[0]
        #expect(folder.showsHeader)
        #expect(folder.rows.count == 1)
    }

    @Test func summaryLineVariants() {
        func s(_ t: UInt32, _ r: UInt32, _ i: UInt32, _ n: UInt32, _ f: UInt32, _ k: UInt32) -> String {
            SummaryLine.text(FolderSummary(total: t, ready: r, inking: i, needsAttention: n, failed: f, inked: k))
        }
        #expect(s(1, 1, 0, 0, 0, 6) == "1 file · all ready · 6 inked")
        #expect(s(3, 0, 3, 0, 0, 0) == "3 files · 0 ready · 3 inking")
        #expect(s(4, 2, 1, 0, 1, 9) == "4 files · 2 ready · 1 inking · 1 failed")
    }

    @Test func anUnclassifiedRowShowsOnlyItsNote() {
        let g = build([folder([doc("1", "doc-1001", "a.pdf", "document", .reading, note: "reading", loader: .read)])]).groups[0]
        #expect(g.rows[0].detail == "reading", "no type until a template matches")
        let q = build([folder([doc("1", "doc-1001", "a.pdf", "document", .queued, loader: .queued)])]).groups[0]
        #expect(q.rows[0].detail == "queued")
    }

    @Test func failedRowsAreShownWithoutCopy() {
        let g = build([folder([doc("1", "a", "a.pdf", "x", .failed, note: "couldn't read it")])]).groups[0]
        #expect(g.rows[0].kind == .failed)
        #expect(!g.rows[0].copyEnabled)
    }

    @Test func mcpNotListening() {
        let m = QueueViewModel.build(queue: [], status: Status(mcpListening: false, privateFolderDisplay: "~/Private"),
                                     reads: [], lastDrop: nil, style: .octopus, now: now)
        #expect(m.footer.right == "MCP not running")
    }

    @Test func emptyCopyNamesThePrivateFolder() {
        let m = QueueViewModel.build(queue: [], status: Status(mcpListening: true, privateFolderDisplay: "~/Vault"),
                                     reads: [], lastDrop: nil, style: .octopus, now: now)
        #expect(m.emptySubtitlePrefix == "They move into ~/Vault. You get a ")
        #expect(m.handlePrefix == "🐙/")
    }

    @Test func loaderMapping() {
        #expect(InkLoaderMode(Loader.read) == .read)
        #expect(InkLoaderMode(Loader.ocr) == .ocr)
        #expect(InkLoaderMode(Loader.queued) == .queued)
        #expect(InkLoaderMode(Loader.idle) == nil)
    }
}

import Foundation
import Testing
@testable import InkerKit

/// What the status item and the popover do with dropped URLs.
struct DropCollectorTests {
    private let home = URL(fileURLWithPath: "/Users/test")
    private let privateFolder = URL(fileURLWithPath: "/Users/test/Private")

    private func url(_ path: String) -> URL { URL(fileURLWithPath: path) }

    private func collect(_ urls: [URL], missing: Set<String> = []) -> DropBatch {
        DropCollector.collect(urls, home: home, privateFolder: privateFolder, exists: { !missing.contains($0.path) })
    }

    @Test func keepsFileURLsInOrderAndDropsDuplicates() {
        let a = url("/Users/test/Downloads/a.pdf")
        let b = url("/Users/test/Downloads/b.pdf")
        let batch = collect([a, b, a, URL(string: "https://example.com/x.pdf")!])
        #expect(batch.urls == [a, b])
    }

    @Test func skipsHiddenAndMissingFiles() {
        let batch = collect([url("/Users/test/Downloads/.DS_Store"), url("/Users/test/Downloads/gone.pdf"), url("/Users/test/Downloads/w2.pdf")],
                            missing: ["/Users/test/Downloads/gone.pdf"])
        #expect(batch.urls.map(\.lastPathComponent) == ["w2.pdf"])
    }

    @Test func dropsItemsInsideAFolderThatIsAlsoDropped() {
        let folder = url("/Users/test/Downloads/Taxes 2025")
        let inner = url("/Users/test/Downloads/Taxes 2025/w2.pdf")
        let sibling = url("/Users/test/Downloads/Taxes 2025 notes.pdf")
        #expect(collect([inner, folder, sibling]).urls == [folder, sibling])
    }

    @Test func standardisesPaths() {
        let batch = collect([url("/Users/test/Downloads/../Downloads/./a.pdf")])
        #expect(batch.urls.map(\.path) == ["/Users/test/Downloads/a.pdf"])
    }

    @Test func reportsACommonSourceFolder() {
        let batch = collect((1...4).map { url("/Users/test/Downloads/f\($0).pdf") })
        #expect(batch.sourceDisplay == "~/Downloads")
        #expect(batch.count == 4)
        let mixed = collect([url("/Users/test/Downloads/a.pdf"), url("/Users/test/Desktop/b.pdf")])
        #expect(mixed.sourceDisplay == nil)
    }

    @Test func filesAlreadyInThePrivateFolderNeedNoMoveDecision() {
        // Decision 5.10: indexed in place.
        #expect(!collect([url("/Users/test/Private/w2.pdf")]).needsMoveDecision)
        #expect(collect([url("/Users/test/Private/w2.pdf"), url("/Users/test/Downloads/a.pdf")]).needsMoveDecision)
        #expect(!collect([]).needsMoveDecision)
    }

    @Test func dialogTitleMatchesTheBoard() {
        let batch = collect((1...4).map { url("/Users/test/Downloads/f\($0).pdf") })
        #expect(DropDialogCopy.title(for: batch) == "Move these 4 files out of ~/Downloads?")
        let one = collect([url("/Users/test/Downloads/w2.pdf")])
        #expect(DropDialogCopy.title(for: one) == "Move this file out of ~/Downloads?")
        let mixed = collect([url("/Users/test/Downloads/a.pdf"), url("/Users/test/Desktop/b.pdf")])
        #expect(DropDialogCopy.title(for: mixed) == "Move these 2 files?")
        #expect(DropDialogCopy.body == "Moving it leaves nothing in Downloads to drag into a chat by mistake.")
    }
}

import Testing
@testable import InkerKit

/// The row menu panel: the brief's items in order, "Preview inked" for a
/// document only, Forget turning into a red "Are you sure?" that stays up,
/// and where the panel opens.
struct RowMenuTests {
    private let file = RowMenuModel(codename: "w2-2025-2", isFolder: false)
    private let folder = RowMenuModel(codename: "folder-0412", isFolder: true)

    @Test func aDocumentHasSixItemsInOrderWithASeparatorBeforeForget() {
        let items = RowMenu.items(file, confirmingForget: false)
        #expect(items.map(\.title) == ["Copy 🐙/w2-2025-2", "Copy ink/w2-2025-2", "Show in Finder", "Preview inked",
                                        "Ink again", "Forget this file"])
        #expect(items.map(\.action) == [.copyOctopus, .copyAscii, .showInFinder, .previewInked, .inkAgain, .forget])
        #expect(items.map(\.separatorAbove) == [false, false, false, false, false, true])
        #expect(items.allSatisfy { !$0.isDestructive })
    }

    @Test func aFolderHasNoPreviewItem() {
        // A folder has no inked view: the item is left out, not disabled.
        let items = RowMenu.items(folder, confirmingForget: false)
        #expect(items.map(\.action) == [.copyOctopus, .copyAscii, .showInFinder, .inkAgain, .forget])
        #expect(items.map(\.separatorAbove) == [false, false, false, false, true])
    }

    @Test func forgetAsksAreYouSureInRedAfterOneClick() {
        let items = RowMenu.items(file, confirmingForget: true)
        #expect(items.last?.title == "Are you sure?")
        #expect(items.last?.isDestructive == true)
        #expect(items.dropLast().allSatisfy { !$0.isDestructive }, "only Forget changes")
    }

    @Test func aFolderForgetsTheFolder() {
        #expect(RowMenu.items(folder, confirmingForget: false).last?.title == "Forget this folder")
        #expect(RowMenu.items(folder, confirmingForget: true).last?.title == "Are you sure?")
    }

    @Test func theEstimatedHeightCountsTheItems() {
        // 29 points an item, plus the separator block and the padding.
        #expect(RowMenu.estimatedHeight(folder) == 168)
        #expect(RowMenu.estimatedHeight(file) == 197)
    }

    @Test func thePanelOpensBelowUnlessItWouldLeaveTheBoard() {
        #expect(RowMenu.placement(rowMaxY: 100, panelHeight: 170, boardHeight: 400) == .below)
        #expect(RowMenu.placement(rowMaxY: 300, panelHeight: 170, boardHeight: 400) == .above)
        #expect(RowMenu.placement(rowMaxY: 226, panelHeight: 170, boardHeight: 400) == .below, "exactly fits")
    }
}

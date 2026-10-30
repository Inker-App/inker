import Testing
@testable import InkerKit

/// The row menu panel: five items in the brief's order, Forget turning into
/// a red "Are you sure?" that stays up, and where the panel opens.
struct RowMenuTests {
    private let file = RowMenuModel(codename: "w2-2025-2", isFolder: false)
    private let folder = RowMenuModel(codename: "folder-0412", isFolder: true)

    @Test func fiveItemsInOrderWithASeparatorBeforeForget() {
        let items = RowMenu.items(file, confirmingForget: false)
        #expect(items.map(\.title) == ["Copy 🐙/w2-2025-2", "Copy ink/w2-2025-2", "Show in Finder", "Ink again", "Forget this file"])
        #expect(items.map(\.action) == [.copyOctopus, .copyAscii, .showInFinder, .inkAgain, .forget])
        #expect(items.map(\.separatorAbove) == [false, false, false, false, true])
        #expect(items.allSatisfy { !$0.isDestructive })
    }

    @Test func forgetAsksAreYouSureInRedAfterOneClick() {
        let items = RowMenu.items(file, confirmingForget: true)
        #expect(items[4].title == "Are you sure?")
        #expect(items[4].isDestructive)
        #expect(items[0...3].allSatisfy { !$0.isDestructive }, "only Forget changes")
    }

    @Test func aFolderForgetsTheFolder() {
        #expect(RowMenu.items(folder, confirmingForget: false)[4].title == "Forget this folder")
        #expect(RowMenu.items(folder, confirmingForget: true)[4].title == "Are you sure?")
    }

    @Test func thePanelOpensBelowUnlessItWouldLeaveTheBoard() {
        #expect(RowMenu.placement(rowMaxY: 100, panelHeight: 170, boardHeight: 400) == .below)
        #expect(RowMenu.placement(rowMaxY: 300, panelHeight: 170, boardHeight: 400) == .above)
        #expect(RowMenu.placement(rowMaxY: 226, panelHeight: 170, boardHeight: 400) == .below, "exactly fits")
    }
}

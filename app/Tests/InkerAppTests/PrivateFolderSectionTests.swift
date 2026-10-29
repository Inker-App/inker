import Foundation
import Testing
@testable import InkerKit

/// Settings shows where the private folder is and opens it in Finder.
struct PrivateFolderSectionTests {
    private let home = URL(fileURLWithPath: "/Users/test")

    @Test func unsetMeansTheDefaultFolder() {
        #expect(PrivateFolderSection.folder(stored: nil, home: home).path == "/Users/test/Private")
        #expect(PrivateFolderSection.display(stored: nil, home: home) == "~/Private")
    }

    @Test func storedPathWins() {
        let url = PrivateFolderSection.folder(stored: "/Users/test/Taxes/Private", home: home)
        #expect(url.path == "/Users/test/Taxes/Private")
        #expect(PrivateFolderSection.display(stored: "/Users/test/Taxes/Private", home: home) == "~/Taxes/Private")
    }

    @Test func tildeInStoredPathIsExpanded() {
        #expect(PrivateFolderSection.folder(stored: "~/Vault", home: home).path == "/Users/test/Vault")
    }

    @Test func folderOutsideHomeShowsInFull() {
        #expect(PrivateFolderSection.display(stored: "/Volumes/Keep/Private", home: home) == "/Volumes/Keep/Private")
    }

    @Test func emptyStoredPathFallsBackToDefault() {
        #expect(PrivateFolderSection.folder(stored: "", home: home).path == "/Users/test/Private")
    }

    @Test func copy() {
        #expect(PrivateFolderSection.title == "Private folder")
        #expect(PrivateFolderSection.openButton == "Open in Finder")
    }
}

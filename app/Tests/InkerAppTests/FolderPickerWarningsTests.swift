import Foundation
import Testing
@testable import InkerKit

/// Decision 5.15: default ~/Private; warn on ~/Desktop, ~/Documents and any
/// path inside a known agent workspace.
struct FolderPickerWarningsTests {
    private let home = URL(fileURLWithPath: "/Users/test")

    private func warnings(_ path: String, gitRoots: Set<String> = []) -> [FolderWarning] {
        FolderPickerWarnings.warnings(for: URL(fileURLWithPath: path), home: home,
                                      containsGitDirectory: { gitRoots.contains($0.path) })
    }

    @Test func defaultIsPrivateInHomeAndIsClean() {
        #expect(FolderPickerWarnings.defaultFolder(home: home).path == "/Users/test/Private")
        #expect(warnings("/Users/test/Private").isEmpty)
    }

    @Test func desktopAndDocumentsWarnAboutICloud() {
        #expect(warnings("/Users/test/Desktop") == [.iCloudSynced("~/Desktop")])
        #expect(warnings("/Users/test/Documents/Private") == [.iCloudSynced("~/Documents")])
        #expect(warnings("/Users/test/DesktopStuff").isEmpty, "a prefix is not a parent")
    }

    @Test func agentConfigFoldersWarn() {
        #expect(warnings("/Users/test/.claude/private") == [.agentWorkspace(".claude")])
        #expect(warnings("/Users/test/work/.cursor") == [.agentWorkspace(".cursor")])
        #expect(warnings("/Users/test/.codex/x") == [.agentWorkspace(".codex")])
    }

    @Test func insideAGitRepositoryWarns() {
        #expect(warnings("/Users/test/code/app/private", gitRoots: ["/Users/test/code/app"]) == [.gitRepository("~/code/app")])
        #expect(warnings("/Users/test/code/app", gitRoots: ["/Users/test/code/app"]) == [.gitRepository("~/code/app")])
        #expect(warnings("/Users/test/Private", gitRoots: ["/Users/test/code/app"]).isEmpty)
    }

    @Test func severalWarningsCanApply() {
        let w = warnings("/Users/test/Documents/repo/.claude", gitRoots: ["/Users/test/Documents/repo"])
        #expect(w == [.iCloudSynced("~/Documents"), .agentWorkspace(".claude"), .gitRepository("~/Documents/repo")])
    }

    @Test func everyWarningHasAMessage() {
        for w in [FolderWarning.iCloudSynced("~/Desktop"), .agentWorkspace(".claude"), .gitRepository("~/code")] {
            #expect(!w.message.isEmpty)
        }
    }

    @Test func pathDisplayAbbreviatesHome() {
        #expect(PathDisplay.abbreviate(URL(fileURLWithPath: "/Users/test/Private"), home: home) == "~/Private")
        #expect(PathDisplay.abbreviate(URL(fileURLWithPath: "/Users/test"), home: home) == "~")
        #expect(PathDisplay.abbreviate(URL(fileURLWithPath: "/Users/tester/x"), home: home) == "/Users/tester/x")
        #expect(PathDisplay.expand("~/Private", home: home).path == "/Users/test/Private")
    }
}

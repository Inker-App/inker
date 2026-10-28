import Foundation
import Testing
@testable import InkerKit

/// "Copied" shows for 1.5 s on the button that was pressed.
struct CopyFeedbackTests {
    @Test func showsCopiedFor1_5SecondsOnTheRightButton() {
        var f = CopyFeedback()
        let t0 = Date(timeIntervalSince1970: 100)
        f.markCopied("g-1", at: t0)
        #expect(f.isShowingCopied("g-1", now: t0.addingTimeInterval(1.49)))
        #expect(!f.isShowingCopied("g-1", now: t0.addingTimeInterval(1.5)))
        #expect(!f.isShowingCopied("d-2", now: t0))
        #expect(CopyFeedback.duration == 1.5)
    }

    @Test func aNewCopyMovesTheFeedback() {
        var f = CopyFeedback()
        let t0 = Date(timeIntervalSince1970: 100)
        f.markCopied("a", at: t0)
        f.markCopied("b", at: t0.addingTimeInterval(0.5))
        #expect(!f.isShowingCopied("a", now: t0.addingTimeInterval(0.6)))
        #expect(f.isShowingCopied("b", now: t0.addingTimeInterval(1.9)))
    }

    @Test func labels() {
        var f = CopyFeedback()
        let t0 = Date(timeIntervalSince1970: 100)
        #expect(f.label(for: "a", now: t0) == "Copy")
        f.markCopied("a", at: t0)
        #expect(f.label(for: "a", now: t0) == "Copied")
    }
}

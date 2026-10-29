import InkerCore
import Testing
@testable import InkerKit

/// The "Move or copy?" question lives inside the popover now. It is asked
/// only when a drop needs the decision and nothing has been remembered.
struct DropDecisionTests {
    @Test func filesAlreadyInPlaceAreMovedWithoutAsking() {
        #expect(DropDecision.resolve(needsMoveDecision: false, remembered: nil) == .ingest(.move))
        #expect(DropDecision.resolve(needsMoveDecision: false, remembered: .copy) == .ingest(.move))
    }

    @Test func rememberedAnswerIsUsed() {
        #expect(DropDecision.resolve(needsMoveDecision: true, remembered: .move) == .ingest(.move))
        #expect(DropDecision.resolve(needsMoveDecision: true, remembered: .copy) == .ingest(.copy))
    }

    @Test func otherwiseThePopoverAsks() {
        #expect(DropDecision.resolve(needsMoveDecision: true, remembered: nil) == .ask)
    }
}

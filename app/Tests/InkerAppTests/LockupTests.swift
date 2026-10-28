import CoreGraphics
import Foundation
import Testing
@testable import InkerKit

/// `startLock`: the 100x20 lockup, mark at (0,1), word at (28,1), cell 2.6.
struct LockupTests {
    @Test func geometryMatchesTheBoard() {
        let l = Lockup()
        #expect(l.size == CGSize(width: 100, height: 20))
        #expect(l.mark.cell == 2.6 && l.word.cell == 2.6)
        #expect(l.mark.originX == 0 && l.mark.originY == 1)
        #expect(l.word.originX == 28 && l.word.originY == 1)
    }

    @Test func markBreathesBetweenPoint8AndOne() {
        var l = Lockup()
        l.update(time: 0.4)
        for i in 0..<l.mark.values.count {
            let row = Double(i / l.mark.width)
            let expected = l.mark.base[i] * (0.9 + 0.1 * sin(0.4 * 1.6 - row * 0.6))
            #expect(close(l.mark.values[i], expected))
        }
        // The word stays solid.
        #expect(l.word.values == l.word.base)
    }
}

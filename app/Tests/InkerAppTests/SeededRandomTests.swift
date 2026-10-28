import Foundation
import Testing
@testable import InkerKit

/// `rng` from the boards (mulberry32). Golden values from node.
struct SeededRandomTests {
    @Test func matchesTheBoardsForSeed31() {
        var r = SeededRandom(seed: 31)
        let expected = [0.6722561160568148, 0.8359460374340415, 0.7496604530606419, 0.04891713825054467, 0.41673933155834675]
        for e in expected {
            #expect(close(r.next(), e))
        }
    }

    @Test func matchesTheBoardsForSeed6() {
        var r = SeededRandom(seed: 6)
        for e in [0.5257466111797839, 0.002343546599149704, 0.6631714215036482] {
            #expect(close(r.next(), e))
        }
    }

    @Test func staysInTheUnitInterval() {
        var r = SeededRandom(seed: 12345)
        for _ in 0..<10_000 {
            let x = r.next()
            #expect(x >= 0 && x < 1)
        }
    }
}

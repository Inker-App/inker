import Foundation
import Testing
@testable import InkerKit

/// Gray-Scott from `makeRD` (f = 0.0545, k = 0.062). The board's version
/// diverges to NaN because `drop` sets V = 1 while U stays 1; ours clamps
/// U and V to [0, 1] after each update (see GrayScott.swift). Golden values
/// are from the board's JS with that one clamp added.
struct ReactionDiffusionTests {
    @Test func parameters() {
        #expect(GrayScott.feed == 0.0545 && GrayScott.kill == 0.062)
    }

    @Test func emptyFieldIsASteadyState() {
        var g = GrayScott(width: 6, height: 5, seedCount: 0, seed: 1)
        for _ in 0..<20 { g.step() }
        #expect(g.u.allSatisfy { $0 == 1 })
        #expect(g.v.allSatisfy { $0 == 0 })
    }

    @Test func dropWrapsAroundTheEdges() {
        var g = GrayScott(width: 4, height: 4, seedCount: 0, seed: 1)
        g.drop(x: 0, y: 0, size: 1)
        #expect(g.v.filter { $0 == 1 }.count == 9)
        #expect(g.v[3 * 4 + 3] == 1, "(-1, -1) wraps to (3, 3)")
    }

    @Test func aSeededDropGrows() {
        var g = GrayScott(width: 8, height: 8, seedCount: 0, seed: 1)
        g.drop(x: 4, y: 4, size: 1)
        var counts: [Int] = []
        for q in 0..<40 {
            g.step()
            if q % 10 == 9 { counts.append(g.v.filter { $0 > 0.1 }.count) }
        }
        #expect(counts == [47, 64, 64, 64])
        #expect(close(Double(g.v[36]), 0.3655588626861572, 1e-6))
    }

    @Test func emptyZoneFieldMatchesTheBoardAfter300Steps() {
        var g = GrayScott(width: 82, height: 96, seedCount: 10, seed: 6)
        #expect(g.v.reduce(0, +) == 90)
        for _ in 0..<300 { g.step() }
        let sum = g.v.reduce(0.0) { $0 + Double($1) }
        #expect(close(sum, 914.1865691284734, 1e-3))
        #expect(close(Double(g.v[4000]), 0.02039242349565029, 1e-6))
        let lit = g.v.filter { GrayScott.dotBrightness($0) >= 0.015 }.count
        #expect(lit == 2880)
    }

    @Test func valuesStayInTheUnitInterval() {
        var g = GrayScott(width: 20, height: 20, seedCount: 5, seed: 3)
        for _ in 0..<200 { g.step() }
        #expect(g.u.allSatisfy { $0 >= 0 && $0 <= 1 })
        #expect(g.v.allSatisfy { $0 >= 0 && $0 <= 1 })
    }

    @Test func dotBrightnessIsSmoothstepPoint1ToPoint28TimesPoint9() {
        #expect(GrayScott.dotBrightness(0.05) == 0)
        #expect(close(GrayScott.dotBrightness(0.28), 0.9))
    }

    @Test func emptyZoneGridIsTheBoardSizeOver4() {
        #expect(GrayScott.emptyZoneGrid == (width: 82, height: 96))
    }
}

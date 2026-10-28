import Foundation
import CoreGraphics
import Testing
@testable import InkerKit

/// `field`, `ctr`, `bridge` and `drawInk` from the boards, as shapes.
struct InkGridGeometryTests {
    private func field(_ rows: [String], cell: Double = 10) -> InkField {
        InkField(bitmap: InkBitmap(rows: rows), cell: cell, originX: 0, originY: 0)
    }

    @Test func dotRadiusAndGap() {
        let f = field(["#"], cell: 10)
        #expect(f.gap == 1)
        #expect(f.dotRadius == 4.5) // cell/2 - gap/2
    }

    @Test func cellRadiusScalesWithValueAndCapsAtOne() {
        #expect(InkGeometry.cellRadius(dotRadius: 4.5, value: 0.5) == 2.25)
        #expect(InkGeometry.cellRadius(dotRadius: 4.5, value: 1.7) == 4.5)
    }

    @Test func bridgeHalfWidthFollowsSmoothstepOfTheWeakerCell() {
        #expect(InkGeometry.bridgeHalfWidth(dotRadius: 4, 0.55, 1) == 0)
        #expect(InkGeometry.bridgeHalfWidth(dotRadius: 4, 1, 0.9) == 4)
        #expect(close(InkGeometry.bridgeHalfWidth(dotRadius: 4, 0.725, 1), 2))
    }

    @Test func fillsOnlyWhenAllFourExceedPoint88() {
        #expect(InkGeometry.fills(0.9, 0.9, 0.9, 0.89))
        #expect(!InkGeometry.fills(0.9, 0.9, 0.9, 0.88))
    }

    @Test func centersIncludeOriginAndOffsets() {
        var f = InkField(bitmap: InkBitmap(rows: ["##"]), cell: 10, originX: 3, originY: 4)
        #expect(f.center(1) == CGPoint(x: 18, y: 9))
        f.offsetsX[1] = 2
        #expect(f.center(1) == CGPoint(x: 20, y: 9))
    }

    @Test func faintCellsAreSkipped() {
        var f = field(["##"])
        f.values = [0.019, 1] // the board draws v >= 0.02
        let s = InkGeometry.shapes(for: f, diagonals: false)
        #expect(s.circles.count == 1)
        #expect(s.quads.isEmpty)
    }

    @Test func aFullTwoByTwoBlockGetsFourBridgesAndAFill() {
        let s = InkGeometry.shapes(for: field(["##", "##"]), diagonals: false)
        #expect(s.circles.count == 4)
        #expect(s.quads.count == 5) // right x2, down x2, one 2x2 fill
        for q in s.quads {
            #expect(q.signedArea > 0, "every quad is wound the same way so a nonzero fill unions them")
        }
    }

    @Test func lowValuesDisperseIntoSeparateDots() {
        var f = field(["##", "##"])
        f.values = [0.5, 0.5, 0.5, 0.5]
        let s = InkGeometry.shapes(for: f, diagonals: false)
        #expect(s.circles.count == 4)
        #expect(s.quads.isEmpty)
    }

    @Test func diagonalBridgesOnlyJoinCornerToCornerStrokes() {
        // "#." over ".#": a diagonal stroke.
        let diag = field(["#.", ".#"])
        #expect(InkGeometry.shapes(for: diag, diagonals: true).quads.count == 1)
        #expect(InkGeometry.shapes(for: diag, diagonals: false).quads.isEmpty)
        // Anti-diagonal.
        #expect(InkGeometry.shapes(for: field([".#", "#."]), diagonals: true).quads.count == 1)
        // A filled block never gets diagonals.
        #expect(InkGeometry.shapes(for: field(["##", "##"]), diagonals: true).quads.count == 5)
    }

    @Test func bridgeIsPerpendicularToTheLink() {
        let f = field(["##"])
        let q = InkGeometry.bridge(f, 0, 1, dotRadius: f.dotRadius)
        #expect(q != nil)
        let ys = q!.points.map(\.y).sorted()
        #expect(close(ys.first!, 5 - 4.5) && close(ys.last!, 5 + 4.5))
    }
}

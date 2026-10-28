import CoreGraphics
import Foundation

public struct InkCircle: Equatable, Sendable {
    public var center: CGPoint
    public var radius: Double
}

/// A convex quad, always wound with positive signed area (the boards' `poly`
/// reverses negative ones) so one nonzero-winding fill unions everything.
public struct InkQuad: Equatable, Sendable {
    public private(set) var points: [CGPoint]

    public init(_ points: [CGPoint]) {
        var p = points
        if InkQuad.signedArea(of: p) < 0 { p.reverse() }
        self.points = p
    }

    public var signedArea: Double { InkQuad.signedArea(of: points) }

    static func signedArea(of p: [CGPoint]) -> Double {
        var s = 0.0
        for i in 0..<p.count {
            let q = p[(i + 1) % p.count]
            s += Double(p[i].x * q.y - q.x * p[i].y)
        }
        return s / 2
    }
}

/// Everything one `drawInk` call fills, in drawing order.
public struct InkShapes: Equatable, Sendable {
    public var circles: [InkCircle] = []
    public var quads: [InkQuad] = []
}

/// `drawInk` and `bridge` from the boards, as geometry. The renderers turn
/// this into one path and fill it once.
public enum InkGeometry {
    /// Cells below this are not drawn (`if (v < 0.02) continue`).
    public static let visibleThreshold = 0.02
    /// All four of a 2x2 block above this fill solid.
    public static let fillThreshold = 0.88
    /// Bridges start at `min(va, vb) = 0.55` and are full width at 0.9.
    public static let bridgeStart = 0.55
    public static let bridgeFull = 0.9

    /// `r = R * min(1, v)`.
    public static func cellRadius(dotRadius r: Double, value v: Double) -> Double {
        r * min(1, v)
    }

    /// Half-width of the neck between two cells: `R * smoothstep(0.55, 0.9, min(va, vb))`.
    public static func bridgeHalfWidth(dotRadius r: Double, _ va: Double, _ vb: Double) -> Double {
        r * InkMath.smoothstep(bridgeStart, bridgeFull, min(va, vb))
    }

    public static func fills(_ a: Double, _ b: Double, _ c: Double, _ d: Double) -> Bool {
        min(a, b, c, d) > fillThreshold
    }

    /// `bridge(ctx, F, i, j, R)`: nil when the neck has no width or the cells are
    /// too far apart (moved apart by offsets).
    public static func bridge(_ f: InkField, _ i: Int, _ j: Int, dotRadius r: Double) -> InkQuad? {
        let rb = bridgeHalfWidth(dotRadius: r, f.values[i], f.values[j])
        if rb <= 0 { return nil }
        let a = f.center(i)
        let b = f.center(j)
        let ex = Double(b.x - a.x)
        let ey = Double(b.y - a.y)
        let len = (ex * ex + ey * ey).squareRoot()
        if len < 0.001 || len > f.cell * 1.6 { return nil }
        let nx = -ey / len * rb
        let ny = ex / len * rb
        return InkQuad([
            CGPoint(x: Double(a.x) + nx, y: Double(a.y) + ny),
            CGPoint(x: Double(b.x) + nx, y: Double(b.y) + ny),
            CGPoint(x: Double(b.x) - nx, y: Double(b.y) - ny),
            CGPoint(x: Double(a.x) - nx, y: Double(a.y) - ny),
        ])
    }

    /// `drawInk(ctx, F, col, diag)`.
    public static func shapes(for f: InkField, diagonals: Bool) -> InkShapes {
        var out = InkShapes()
        let w = f.width
        let h = f.height
        let r = f.dotRadius
        for y in 0..<h {
            for x in 0..<w {
                let i = y * w + x
                let v = f.values[i]
                if v < visibleThreshold { continue }
                out.circles.append(InkCircle(center: f.center(i), radius: cellRadius(dotRadius: r, value: v)))
                let joinRight = x < w - 1 && f.values[i + 1] > visibleThreshold
                let joinDown = y < h - 1 && f.values[i + w] > visibleThreshold
                if joinRight, let q = bridge(f, i, i + 1, dotRadius: r) { out.quads.append(q) }
                if joinDown, let q = bridge(f, i, i + w, dotRadius: r) { out.quads.append(q) }
                if diagonals && y < h - 1 {
                    if x < w - 1 && f.base[i + w + 1] > 0 && f.base[i + 1] == 0 && f.base[i + w] == 0,
                       let q = bridge(f, i, i + w + 1, dotRadius: r) {
                        out.quads.append(q)
                    }
                    if x > 0 && f.base[i + w - 1] > 0 && f.base[i - 1] == 0 && f.base[i + w] == 0,
                       let q = bridge(f, i, i + w - 1, dotRadius: r) {
                        out.quads.append(q)
                    }
                }
                if joinRight && joinDown && fills(v, f.values[i + 1], f.values[i + w], f.values[i + w + 1]) {
                    out.quads.append(InkQuad([f.center(i), f.center(i + 1), f.center(i + w + 1), f.center(i + w)]))
                }
            }
        }
        return out
    }
}

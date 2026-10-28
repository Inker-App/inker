import InkerKit
import SwiftUI

/// Ink geometry to SwiftUI paths.
enum InkPaths {
    /// One path for a `drawInk` call, filled once with the nonzero rule.
    /// Circles sweep with increasing angle and quads are wound with positive
    /// signed area (InkQuad normalises), so every piece has the same winding
    /// and overlaps union instead of punching holes.
    // VERIFY: if a Mac shows holes where bridges meet dots, switch to filling
    // circles and quads separately (two fills, still cheap).
    static func path(_ shapes: InkShapes) -> Path {
        var p = Path()
        for c in shapes.circles {
            p.move(to: CGPoint(x: c.center.x + c.radius, y: c.center.y))
            p.addRelativeArc(center: c.center, radius: c.radius, startAngle: .zero, delta: .radians(2 * Double.pi))
            p.closeSubpath()
        }
        for q in shapes.quads {
            p.addLines(q.points)
            p.closeSubpath()
        }
        return p
    }

    static func path(_ field: InkField, diagonals: Bool = false) -> Path {
        path(InkGeometry.shapes(for: field, diagonals: diagonals))
    }

    /// `put(path, x, y, b, C)`: a dot of radius `C/2 * min(1.05, sqrt(b))`.
    static func addDot(_ p: inout Path, x: Double, y: Double, brightness b: Double, cell: Double) {
        guard let r = EyeShading.dotRadius(cell: cell, brightness: b) else { return }
        p.addEllipse(in: CGRect(x: x - r, y: y - r, width: r * 2, height: r * 2))
    }

    /// The boards' dotted outlines (`dotRect`, the peek box): `n` dots of
    /// radius `r` walked evenly around a rectangle.
    static func dottedRect(_ rect: CGRect, count n: Int, radius r: Double) -> Path {
        var p = Path()
        let w = Double(rect.width), h = Double(rect.height)
        let x = Double(rect.minX), y = Double(rect.minY)
        let perimeter = w * 2 + h * 2
        for k in 0..<max(n, 0) {
            let u = Double(k) / Double(n) * perimeter
            var px: Double, py: Double
            if u < w {
                px = x + u; py = y
            } else if u < w + h {
                px = x + w; py = y + u - w
            } else if u < w * 2 + h {
                px = x + w - (u - w - h); py = y + h
            } else {
                px = x; py = y + h - (u - w * 2 - h)
            }
            p.addEllipse(in: CGRect(x: px - r, y: py - r, width: r * 2, height: r * 2))
        }
        return p
    }
}

import AppKit
import InkerKit

/// The 7-cell octopus mark as a menu bar template image, drawn from the
/// boards' MARK bitmap with the ink-grid geometry.
enum MarkIcon {
    /// 7 cells of 2.4 pt = 16.8 pt, centred in 18x18.
    static func make(cell: Double = 2.4, size: CGFloat = 18) -> NSImage {
        let field = InkField(bitmap: InkBitmap(rows: InkGlyphs.mark), cell: cell,
                             originX: (Double(size) - cell * 7) / 2, originY: (Double(size) - cell * 7) / 2)
        let shapes = InkGeometry.shapes(for: field, diagonals: false)
        let image = NSImage(size: NSSize(width: size, height: size), flipped: true) { _ in
            NSColor.black.setFill()
            // Each piece filled on its own: no winding-rule surprises, and it is
            // drawn once per launch.
            for c in shapes.circles {
                NSBezierPath(ovalIn: NSRect(x: c.center.x - c.radius, y: c.center.y - c.radius,
                                            width: c.radius * 2, height: c.radius * 2)).fill()
            }
            for q in shapes.quads {
                let p = NSBezierPath()
                p.move(to: q.points[0])
                for pt in q.points.dropFirst() { p.line(to: pt) }
                p.close()
                p.fill()
            }
            return true
        }
        image.isTemplate = true
        image.accessibilityDescription = "INKER"
        return image
    }
}

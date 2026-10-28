import CoreGraphics
import Foundation

/// `field(bits, cell, {ox, oy})` in the boards: a grid of values `v` in 0...1
/// (1 = pooled ink, low = dispersed dots), plus per-cell offsets for
/// animations that move dots.
public struct InkField: Sendable {
    public let width: Int
    public let height: Int
    /// The bitmap the field was made from; diagonal bridges read it.
    public let base: [Double]
    /// Current values, one per cell. Starts as `base`.
    public var values: [Double]
    public let cell: Double
    /// `cell * 0.1`.
    public let gap: Double
    public var originX: Double
    public var originY: Double
    public var offsetsX: [Double]
    public var offsetsY: [Double]

    public init(bitmap: InkBitmap, cell: Double, originX: Double = 0, originY: Double = 0) {
        width = bitmap.width
        height = bitmap.height
        base = bitmap.cells
        values = bitmap.cells
        self.cell = cell
        gap = cell * 0.1
        self.originX = originX
        self.originY = originY
        offsetsX = [Double](repeating: 0, count: bitmap.cells.count)
        offsetsY = [Double](repeating: 0, count: bitmap.cells.count)
    }

    /// Full-size dot radius `R = cell/2 - gap/2`.
    public var dotRadius: Double { cell / 2 - gap / 2 }

    /// `ctr(F, i)`.
    public func center(_ i: Int) -> CGPoint {
        let x = originX + Double(i % width) * cell + cell / 2 + offsetsX[i]
        let y = originY + Double(i / width) * cell + cell / 2 + offsetsY[i]
        return CGPoint(x: x, y: y)
    }
}

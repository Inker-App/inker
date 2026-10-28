import CoreGraphics
import Foundation

/// `startLock`: the 100x20 header lockup. The mark breathes, the word is still.
public struct Lockup: Sendable {
    public let size = CGSize(width: 100, height: 20)
    public static let cell = 2.6
    public var mark: InkField
    public var word: InkField

    public init() {
        mark = InkField(bitmap: InkBitmap(rows: InkGlyphs.mark), cell: Lockup.cell, originX: 0, originY: 1)
        word = InkField(bitmap: InkGlyphs.word("INKER"), cell: Lockup.cell, originX: 28, originY: 1)
    }

    /// `m.v[i] = m.base[i] * (0.9 + 0.1 * sin(t * 1.6 - y * 0.6))`.
    public mutating func update(time t: Double) {
        for i in 0..<mark.values.count {
            let row = Double(i / mark.width)
            mark.values[i] = mark.base[i] * (0.9 + 0.1 * sin(t * 1.6 - row * 0.6))
        }
    }
}

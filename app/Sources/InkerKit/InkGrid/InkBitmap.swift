import Foundation

/// `bits(rows)` in the boards: '#' is ink, anything else is empty. Ragged rows
/// are padded with empty cells to the widest row.
public struct InkBitmap: Equatable, Sendable {
    public let width: Int
    public let height: Int
    /// Row-major, 0 or 1.
    public let cells: [Double]

    public init(rows: [String]) {
        let w = rows.map(\.count).max() ?? 0
        var a = [Double](repeating: 0, count: w * rows.count)
        for (y, row) in rows.enumerated() {
            for (x, ch) in row.enumerated() where ch == "#" {
                a[y * w + x] = 1
            }
        }
        width = w
        height = rows.count
        cells = a
    }

    /// A solid block (`ones(w, h)` in AppOnboard3).
    public init(width: Int, height: Int, filled: Bool = true) {
        self.width = width
        self.height = height
        cells = [Double](repeating: filled ? 1 : 0, count: width * height)
    }

    public subscript(x x: Int, y y: Int) -> Double {
        cells[y * width + x]
    }

    /// Back to '#' / '.' rows.
    public var rows: [String] {
        (0..<height).map { y in
            String((0..<width).map { x in self[x: x, y: y] > 0 ? "#" : "." })
        }
    }
}

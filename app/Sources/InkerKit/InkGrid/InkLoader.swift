import CoreGraphics
import Foundation

/// The three loader modes in AppPopProcessing.
public enum InkLoaderMode: Equatable, Sendable {
    case read
    case ocr
    case queued
}

/// `startLoaders`: a one-row ink field, 8 px cells, values per mode.
public enum InkLoader {
    public static let cell = 8.0
    /// The board's loaders are 284x8.
    public static let size = CGSize(width: 284, height: 8)

    public static func cellCount(width: Double) -> Int {
        Int((width / cell).rounded(.down))
    }

    public static func value(_ mode: InkLoaderMode, cell j: Int, count w: Int, time t: Double) -> Double {
        switch mode {
        case .read:
            let s = 0.5 + 0.5 * sin(t * 6 - Double(j) * 0.45)
            return 0.18 + 0.45 * s * s * s
        case .ocr:
            let head = InkMath.jsMod(t * 14, Double(w + 10)) - 5
            let d = (Double(j) - head) / 3
            return 0.16 + 0.5 * exp(-(d * d))
        case .queued:
            return 0.1
        }
    }

    /// A loader field with every value set for time `t`.
    public static func field(_ mode: InkLoaderMode, width: Double = 284, time t: Double) -> InkField {
        let n = cellCount(width: width)
        var f = InkField(bitmap: InkBitmap(width: n, height: 1), cell: cell)
        for j in 0..<n {
            f.values[j] = value(mode, cell: j, count: n, time: t)
        }
        return f
    }
}

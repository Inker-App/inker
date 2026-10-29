import CoreGraphics
import Foundation

/// The ink pulse behind a located row: a ring of dots rippling out from the
/// centre and fading over 1.5 s, on the 8 pt ink grid.
public enum InkPulse {
    public static let duration = 1.5
    public static let cell = 8.0

    /// Cell values for `time` seconds after the pulse started. All zero once
    /// it is over.
    public static func field(size: CGSize, time t: Double) -> InkField {
        let w = max(1, Int((Double(size.width) / cell).rounded(.down)))
        let h = max(1, Int((Double(size.height) / cell).rounded(.down)))
        var f = InkField(bitmap: InkBitmap(width: w, height: h), cell: cell)
        guard t >= 0, t < duration else {
            f.values = [Double](repeating: 0, count: w * h)
            return f
        }
        let k = t / duration
        let cx = Double(w) / 2
        let cy = Double(h) / 2
        let maxR = (cx * cx + cy * cy).squareRoot()
        let ring = k * maxR
        let fade = (1 - k) * (1 - k)
        for y in 0..<h {
            for x in 0..<w {
                let dx = Double(x) + 0.5 - cx
                let dy = (Double(y) + 0.5 - cy) * 1.6
                let d = (dx * dx + dy * dy).squareRoot()
                let s = (d - ring) / 2.2
                f.values[y * w + x] = min(1, 0.9 * exp(-s * s) * fade)
            }
        }
        return f
    }
}

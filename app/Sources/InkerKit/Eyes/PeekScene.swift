import CoreGraphics
import Foundation

/// `startPeek` in AppPopAttention: one eye squinting at the dotted outline
/// where the held value sits. The value itself is never drawn.
public struct PeekScene: EyeScene {
    public let size = CGSize(width: 302, height: 84)
    public let cell = 4.0
    /// Left edge and baseline of the value on the board.
    public static let valueOrigin = CGPoint(x: 96, y: 56)
    /// Baseline of the context text ("…wire to").
    public static let contextOrigin = CGPoint(x: 96, y: 30)
    public let valueWidth: Double
    private var yaw = 0.0
    private var pitch = 0.0

    public init(valueLength: Int, characterWidth: Double) {
        valueWidth = Double(valueLength) * characterWidth
    }

    /// The dotted outline: `bx = dx0 - 6, by = dy0 - 15, bw = sw + 12, bh = 22`.
    public var outline: CGRect {
        CGRect(x: PeekScene.valueOrigin.x - 6, y: PeekScene.valueOrigin.y - 15, width: valueWidth + 12, height: 22)
    }

    /// Outline dot alpha: `0.35 + 0.35 * (0.5 + 0.5 sin(3t))`.
    public static func outlineAlpha(time t: Double) -> Double {
        0.35 + 0.35 * (0.5 + 0.5 * sin(t * 3))
    }

    public mutating func frames(time t: Double, dt: Double, pointer: CGPoint?) -> [EyeFrame] {
        let cx = 44 + sin(t * 0.9) * 1.5
        let cy = 42.0
        let o = PeekScene.valueOrigin
        let tx = pointer.map { Double($0.x) } ?? Double(o.x) + valueWidth * (0.5 + 0.45 * sin(t * 1.3))
        let ty = pointer.map { Double($0.y) } ?? Double(o.y) - 5
        let g = Vec3.normalized(tx - cx, ty - cy, 60)
        let k = min(1, dt * 14)
        yaw += (atan2(g.x, g.z) - yaw) * k
        pitch += (asin(g.y) - pitch) * k
        var lid = 0.3 + 0.1 * sin(t * 2.3)
        let bl = InkMath.jsMod(t, 4.1)
        if bl < 0.2 { lid = max(lid, sin(Double.pi * bl / 0.2)) }
        return [EyeFrame(centerX: cx, centerY: cy, radius: 32, gaze: EyeFrame.gaze(yaw: yaw, pitch: pitch), lid: lid, spin: 2.1)]
    }
}

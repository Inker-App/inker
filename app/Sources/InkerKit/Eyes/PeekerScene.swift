import CoreGraphics
import Foundation

/// `startPeeker` in AppPopReady: the 24x24 eye on the last-read line. It looks
/// sideways at the line, blinks, and glances away. Ignores the cursor.
public struct PeekerScene: EyeScene {
    public let size = CGSize(width: 24, height: 24)
    public let cell = 2.0

    public init() {}

    public mutating func frames(time t: Double, dt: Double, pointer: CGPoint?) -> [EyeFrame] {
        let u = InkMath.jsMod(t, 6)
        var yaw = 0.6 + 0.25 * sin(t * 0.7)
        let pitch = 0.05
        var lid = 0.45 + 0.1 * sin(t * 2)
        if u > 4.5 && u < 4.7 { lid = max(lid, sin(Double.pi * (u - 4.5) / 0.2)) }
        if u > 5 && u < 5.6 { yaw = -0.7 }
        return [EyeFrame(centerX: 12, centerY: 12, radius: 11, gaze: EyeFrame.gaze(yaw: yaw, pitch: pitch), lid: lid, spin: 0.7)]
    }
}

import CoreGraphics
import Foundation

/// Where the octopus's slit pupils point: eased towards the cursor every
/// frame, even between the 12 fps renders, exactly as the boards' loop does.
public struct OctopusLook: Sendable {
    public private(set) var x = 0.0
    public private(set) var y = 0.0
    public let center: CGPoint
    public let yOffset: Double
    public let scale: Double

    /// Onboarding: center (240, 58), yOffset 30, scale 160. World board: 120 and 400.
    public init(center: CGPoint, yOffset: Double, scale: Double) {
        self.center = center
        self.yOffset = yOffset
        self.scale = scale
    }

    public mutating func update(pointer: CGPoint?, dt: Double) {
        var lx = 0.0, ly = 0.0
        if let p = pointer {
            lx = InkMath.clamp((Double(p.x) - Double(center.x)) / scale, -1, 1)
            ly = InkMath.clamp((Double(p.y) - Double(center.y) + yOffset) / scale, -1, 1)
        }
        let k = min(1, dt * 6)
        x += (lx - x) * k
        y += (ly - y) * k
    }
}

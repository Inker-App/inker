import Foundation

/// One eye, ready to shade: what the Metal shader receives per eye.
public struct EyeFrame: Equatable, Sendable {
    public var centerX: Double
    public var centerY: Double
    public var radius: Double
    /// Unit gaze vector (x right, y down, z towards the viewer).
    public var gaze: Vec3
    /// 0 open, 1 shut.
    public var lid: Double
    /// Iris spoke phase.
    public var spin: Double

    public init(centerX: Double, centerY: Double, radius: Double, gaze: Vec3, lid: Double, spin: Double) {
        self.centerX = centerX
        self.centerY = centerY
        self.radius = radius
        self.gaze = gaze
        self.lid = lid
        self.spin = spin
    }

    /// `gx = sin(yaw) cos(pitch), gy = sin(pitch), gz = cos(yaw) cos(pitch)`.
    public static func gaze(yaw: Double, pitch: Double) -> Vec3 {
        let cp = cos(pitch)
        return Vec3(sin(yaw) * cp, sin(pitch), cos(yaw) * cp)
    }
}

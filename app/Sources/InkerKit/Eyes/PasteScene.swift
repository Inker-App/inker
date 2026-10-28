import CoreGraphics
import Foundation

/// Panel 3 of AppOnboard3: an eye peering at an inked SSN line. Coordinates
/// are in the 480x150 canvas; the panel starts at x = 328.
public struct PasteScene: EyeScene {
    public let size = CGSize(width: 480, height: 150)
    public let cell = 4.0
    public static let panelX = 328.0
    /// The inked pill: `sx = lx + 30`, `ly = 112`, width 86.
    public static let lineX = panelX + 20
    public static let lineY = 112.0
    public static let pillX = lineX + 30
    public static let pillWidth = 86.0

    public init() {}

    public mutating func frames(time t: Double, dt: Double, pointer: CGPoint?) -> [EyeFrame] {
        let ecx = PasteScene.panelX + 76 + sin(t * 0.8) * 3
        let ecy = 48.0
        let tx = PasteScene.pillX + PasteScene.pillWidth / 2 + sin(t * 1.7) * 30
        let g = Vec3.normalized(tx - ecx, PasteScene.lineY - ecy, 70)
        var lid = 0.38 + 0.08 * sin(t * 3)
        let bl = InkMath.jsMod(t, 3.7)
        if bl < 0.2 { lid = max(lid, sin(Double.pi * bl / 0.2)) }
        return [EyeFrame(centerX: ecx, centerY: ecy, radius: 28, gaze: g, lid: lid, spin: 1.3)]
    }
}

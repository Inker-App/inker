import CoreGraphics
import Foundation

/// AppOnboard1 and AppDropDialog: eyes crowding round ~/Downloads. They stare
/// at the folder unless the cursor is in the canvas.
public struct StaringEyesScene: EyeScene {
    public let size: CGSize
    public let cell: Double
    public let tuning: EyeTuning
    /// What they look at when the cursor is elsewhere.
    public let focus: CGPoint
    public private(set) var eyes: [EyeState]
    private var random: SeededRandom

    public init(size: CGSize, cell: Double, seed: UInt32, focus: CGPoint, eyes specs: [(Double, Double, Double)], tuning: EyeTuning) {
        self.size = size
        self.cell = cell
        self.focus = focus
        self.tuning = tuning
        var r = SeededRandom(seed: seed)
        eyes = specs.map { EyeState(x: $0.0, y: $0.1, radius: $0.2, random: &r) }
        random = r
    }

    /// AppOnboard1: 480x120, 4 px cells, `rng(12)`, folder at (240, 62).
    public static func onboarding() -> StaringEyesScene {
        StaringEyesScene(size: CGSize(width: 480, height: 120), cell: 4, seed: 12, focus: CGPoint(x: 240, y: 62),
                         eyes: [(64, 44, 24), (124, 86, 18), (176, 32, 16), (312, 34, 18), (356, 84, 22), (424, 48, 26), (22, 94, 12), (460, 98, 10)],
                         tuning: .popover)
    }

    /// AppDropDialog: 392x76, 3 px cells, `rng(44)`, files at (196, 36).
    public static func dropDialog() -> StaringEyesScene {
        StaringEyesScene(size: CGSize(width: 392, height: 76), cell: 3, seed: 44, focus: CGPoint(x: 196, y: 36),
                         eyes: [(40, 30, 18), (92, 54, 13), (128, 22, 11), (268, 24, 12), (306, 50, 16), (356, 30, 19)],
                         tuning: .dialog)
    }

    public mutating func frames(time t: Double, dt: Double, pointer: CGPoint?) -> [EyeFrame] {
        let target = pointer ?? focus
        var out: [EyeFrame] = []
        for e in 0..<eyes.count {
            eyes[e].step(dt: dt, time: t, target: target, tuning: tuning, random: &random)
            out.append(eyes[e].frame(time: t))
        }
        return out
    }
}

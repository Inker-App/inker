import CoreGraphics
import Foundation

/// `startZone` in AppPopEmpty: a few eyes drifting slowly over the drop zone,
/// turning to the cursor.
public struct DriftingEyesScene: EyeScene {
    public let size: CGSize
    public let cell: Double
    public let tuning: EyeTuning
    public private(set) var eyes: [EyeState]
    private var random: SeededRandom

    public init(size: CGSize, cell: Double, seed: UInt32, eyes specs: [(Double, Double, Double)], tuning: EyeTuning) {
        self.size = size
        self.cell = cell
        self.tuning = tuning
        var r = SeededRandom(seed: seed)
        eyes = specs.map { EyeState(x: $0.0, y: $0.1, radius: $0.2, random: &r) }
        random = r
    }

    /// The board: 328x384, 3 px cells, `rng(31)`.
    public static func emptyZone() -> DriftingEyesScene {
        DriftingEyesScene(size: CGSize(width: 328, height: 384), cell: 3, seed: 31,
                          eyes: [(62, 74, 20), (268, 300, 24), (250, 64, 14), (58, 320, 12)], tuning: .popover)
    }

    public mutating func frames(time t: Double, dt: Double, pointer: CGPoint?) -> [EyeFrame] {
        var out: [EyeFrame] = []
        for e in 0..<eyes.count {
            eyes[e].x = eyes[e].homeX + sin(t * 0.21 + eyes[e].phase) * 10
            eyes[e].y = eyes[e].homeY + cos(t * 0.17 + eyes[e].phase) * 8
            eyes[e].step(dt: dt, time: t, target: pointer, tuning: tuning, random: &random)
            out.append(eyes[e].frame(time: t))
        }
        return out
    }
}

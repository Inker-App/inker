import Foundation

/// The per-board constants in `eyeStep`.
public struct EyeTuning: Equatable, Sendable {
    /// Chance a saccade starts a "dart" that ignores the cursor for a while.
    public var dartChance: Double
    /// Dart length is `0.3 + r * dartSpan` seconds.
    public var dartSpan: Double
    /// The z of the gaze vector towards the cursor, in points.
    public var gazeDepth: Double

    public init(dartChance: Double, dartSpan: Double, gazeDepth: Double) {
        self.dartChance = dartChance
        self.dartSpan = dartSpan
        self.gazeDepth = gazeDepth
    }

    /// AppPopEmpty, AppOnboard1.
    public static let popover = EyeTuning(dartChance: 0.45, dartSpan: 0.8, gazeDepth: 160)
    /// AppDropDialog.
    public static let dialog = EyeTuning(dartChance: 0.35, dartSpan: 0.6, gazeDepth: 120)
}

import CoreGraphics
import Foundation

/// A surface full of eyes: the per-board start functions. `pointer` is in the
/// surface's points, top-left origin, nil when the cursor is elsewhere.
public protocol EyeScene {
    var size: CGSize { get }
    var cell: Double { get }
    mutating func frames(time t: Double, dt: Double, pointer: CGPoint?) -> [EyeFrame]
}

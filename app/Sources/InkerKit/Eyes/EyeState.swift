import CoreGraphics
import Foundation

/// `mkEye` / `eyeStep` from the boards: a Nosy One's gaze and blink state.
///
/// The random source is passed in, not owned, because on the boards every eye
/// in a scene draws from one shared `rng`; keeping the call order identical
/// keeps the motion identical to the design canvas.
public struct EyeState: Sendable {
    /// Current position (the drifting scenes move it each frame).
    public var x: Double
    public var y: Double
    public let homeX: Double
    public let homeY: Double
    public let radius: Double

    public var yaw = 0.0
    public var pitch = 0.0
    public var targetYaw = 0.0
    public var targetPitch = 0.0
    /// The random gaze picked at the last saccade (`ry`, `rp`).
    public var restYaw = 0.0
    public var restPitch = 0.0
    /// `next`: time of the next saccade.
    public var nextSaccade: Double
    /// `dart`: ignore the cursor until this time (0 = not darting).
    public var dartUntil = 0.0
    /// `bt`: blink start, -1 when not blinking.
    public var blinkStart = -1.0
    /// `nb`: time of the next blink.
    public var nextBlink: Double
    public let phase: Double
    public let spin: Double
    public var lid = 0.0

    public static let blinkDuration = 0.2

    /// `mkEye(x, y, R, r)`: consumes four randoms in the board's order.
    public init(x: Double, y: Double, radius: Double, random r: inout SeededRandom) {
        self.x = x
        self.y = y
        homeX = x
        homeY = y
        self.radius = radius
        nextSaccade = r.next() * 2
        nextBlink = 1 + r.next() * 4
        phase = r.next() * 6.283
        spin = r.next() * 6.283
    }

    /// `eyeStep(E, dt, t, tx, ty)`.
    public mutating func step(dt: Double, time t: Double, target: CGPoint?, tuning: EyeTuning, random r: inout SeededRandom) {
        if t > nextSaccade {
            nextSaccade = t + 0.4 + r.next() * 2.4
            dartUntil = r.next() < tuning.dartChance ? t + 0.3 + r.next() * tuning.dartSpan : 0
            restYaw = (r.next() - 0.5) * 1.9
            restPitch = (r.next() - 0.5) * 1.3
        }
        if let target, t > dartUntil {
            let g = EyeState.gaze(from: CGPoint(x: x, y: y), to: target, depth: tuning.gazeDepth)
            targetYaw = g.yaw
            targetPitch = g.pitch
        } else {
            targetYaw = restYaw
            targetPitch = restPitch
        }
        let k = min(1, dt * 18)
        yaw += (targetYaw - yaw) * k
        pitch += (targetPitch - pitch) * k

        if t > nextBlink && blinkStart < 0 { blinkStart = t }
        lid = 0
        if blinkStart >= 0 {
            let u = (t - blinkStart) / EyeState.blinkDuration
            if u >= 1 {
                blinkStart = -1
                nextBlink = t + 2 + r.next() * 5
            } else {
                lid = EyeState.lid(blinkProgress: u)
            }
        }
    }

    /// `sin(pi * u)` for u in 0...1.
    public static func lid(blinkProgress u: Double) -> Double {
        sin(Double.pi * u)
    }

    /// Yaw and pitch that look from `from` towards `to`, with the target
    /// `depth` points in front of the screen: `g = n3(tx - x, ty - y, depth)`.
    public static func gaze(from: CGPoint, to: CGPoint, depth: Double) -> (yaw: Double, pitch: Double) {
        let g = Vec3.normalized(Double(to.x - from.x), Double(to.y - from.y), depth)
        return (atan2(g.x, g.z), asin(g.y))
    }

    /// What `eyeDraw` shades this frame: the small wobble plus the gaze vector.
    public func frame(time t: Double, squint: Double = 0) -> EyeFrame {
        EyeFrame(centerX: x + sin(t * 0.7 + phase) * 2,
                 centerY: y + cos(t * 0.9 + phase) * 1.5,
                 radius: radius,
                 gaze: EyeFrame.gaze(yaw: yaw, pitch: pitch),
                 lid: max(lid, squint),
                 spin: spin)
    }
}

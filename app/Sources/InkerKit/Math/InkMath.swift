import Foundation

/// Small numeric helpers shared by every renderer. Names follow the boards' JS.
public enum InkMath {
    /// `ss(a, b, x)` in the boards: clamped Hermite smoothstep.
    public static func smoothstep(_ a: Double, _ b: Double, _ x: Double) -> Double {
        var t = (x - a) / (b - a)
        t = t < 0 ? 0 : (t > 1 ? 1 : t)
        return t * t * (3 - 2 * t)
    }

    /// `cl(x)` in the boards, generalised.
    public static func clamp(_ x: Double, _ lo: Double, _ hi: Double) -> Double {
        x < lo ? lo : (x > hi ? hi : x)
    }

    /// JS `%` for positive divisors and any sign of `x` (truncating, like JS).
    public static func jsMod(_ x: Double, _ m: Double) -> Double {
        x.truncatingRemainder(dividingBy: m)
    }
}

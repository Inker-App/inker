import Foundation

/// A plain 3-vector in Double. The GPU side converts to `SIMD4<Float>`.
public struct Vec3: Equatable, Sendable {
    public var x: Double
    public var y: Double
    public var z: Double

    public init(_ x: Double, _ y: Double, _ z: Double) {
        self.x = x
        self.y = y
        self.z = z
    }

    /// `n3(x, y, z)` in the boards: normalise, with a zero vector left at zero.
    public static func normalized(_ x: Double, _ y: Double, _ z: Double) -> Vec3 {
        var l = (x * x + y * y + z * z).squareRoot()
        if l == 0 { l = 1 }
        return Vec3(x / l, y / l, z / l)
    }

    public var length: Double { (x * x + y * y + z * z).squareRoot() }

    public func dot(_ o: Vec3) -> Double { x * o.x + y * o.y + z * o.z }

    public static func + (a: Vec3, b: Vec3) -> Vec3 { Vec3(a.x + b.x, a.y + b.y, a.z + b.z) }

    /// Row-major 3x3 times vector (`mul` in the octopus board).
    public static func multiply(_ m: [Double], _ v: Vec3) -> Vec3 {
        Vec3(m[0] * v.x + m[1] * v.y + m[2] * v.z,
             m[3] * v.x + m[4] * v.y + m[5] * v.z,
             m[6] * v.x + m[7] * v.y + m[8] * v.z)
    }
}

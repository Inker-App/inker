import Foundation

/// One tentacle: 9 control points (8 tapered capsules) and a bounding sphere.
public struct Tentacle: Equatable, Sendable {
    public var points: [Vec3]
    public var radii: [Double]
    public var boundsCenter: Vec3
    public var boundsRadius: Double
}

/// The CPU half of the octopus (AppOnboard2 / WorldOctopus): `build(T)` and the
/// rotation. The SDF, raymarch and shading live in OctopusShader.metal.
public enum OctopusRig {
    public static let tentacleCount = 8
    public static let segments = 8
    public static let marchSteps = 56
    public static let boundingRadius = 2.0
    public static let framesPerSecond = 12
    /// `EYE`: the two eye spheres (radius 0.12).
    public static let eyes = [Vec3(-0.25, -0.40, -0.5), Vec3(0.25, -0.40, -0.5)]

    /// `build(T)`.
    public static func build(time t: Double) -> [Tentacle] {
        var out: [Tentacle] = []
        for i in 0..<tentacleCount {
            let fi = Double(i)
            let a = fi / Double(tentacleCount) * 6.2832 + 0.2 + 0.12 * sin(t * 0.7 + fi * 1.3)
            let ux = cos(a)
            let uz = sin(a)
            var th = 1.25
            var px = ux * 0.32
            var py = 0.0
            var pz = uz * 0.32
            var points: [Vec3] = []
            var radii: [Double] = []
            for k in 0...segments {
                let fk = Double(k)
                // The board stores points and radii in Float32Arrays.
                points.append(Vec3(Double(Float(px)), Double(Float(py)), Double(Float(pz))))
                radii.append(Double(Float(0.16 * (1 - fk / (Double(segments) + 1.5)) + 0.02)))
                th += -(0.06 + 0.42 * fk / Double(segments)) * (1 + 0.3 * sin(t * 1.4 + fi * 1.9))
                let cs = cos(th)
                let sn = sin(th)
                px += ux * cs * 0.21
                pz += uz * cs * 0.21
                py += sn * 0.21
            }
            var mx = 0.0, my = 0.0, mz = 0.0
            for p in points {
                mx += p.x
                my += p.y
                mz += p.z
            }
            let n = Double(segments + 1)
            mx /= n
            my /= n
            mz /= n
            var br = 0.0
            for (p, r) in zip(points, radii) {
                br = max(br, Vec3(p.x - mx, p.y - my, p.z - mz).length + r)
            }
            out.append(Tentacle(points: points, radii: radii, boundsCenter: Vec3(mx, my, mz), boundsRadius: br))
        }
        return out
    }

    /// `M = tr(mm(rx(0.45), ry(0.35 sin(0.25 T))))`, row-major.
    public static func rotation(time t: Double) -> [Double] {
        let rx = rotationX(0.45)
        let ry = rotationY(0.35 * sin(t * 0.25))
        return transpose(multiply(rx, ry))
    }

    /// Light, view, half and ray direction in model space for rotation `m`:
    /// `L = mul(n3(-0.5, -0.65, -0.6))`, `V = mul(0, 0, -1)`, `H = n3(L + V)`, `D = mul(0, 0, 1)`.
    public static func frameVectors(_ m: [Double]) -> (light: Vec3, view: Vec3, half: Vec3, direction: Vec3) {
        let l = Vec3.multiply(m, Vec3.normalized(-0.5, -0.65, -0.6))
        let v = Vec3.multiply(m, Vec3(0, 0, -1))
        let h = Vec3.normalized(l.x + v.x, l.y + v.y, l.z + v.z)
        let d = Vec3.multiply(m, Vec3(0, 0, 1))
        return (l, v, h, d)
    }

    static func rotationX(_ a: Double) -> [Double] {
        let c = cos(a), s = sin(a)
        return [1, 0, 0, 0, c, -s, 0, s, c]
    }

    static func rotationY(_ a: Double) -> [Double] {
        let c = cos(a), s = sin(a)
        return [c, 0, s, 0, 1, 0, -s, 0, c]
    }

    static func multiply(_ a: [Double], _ b: [Double]) -> [Double] {
        var r = [Double](repeating: 0, count: 9)
        for i in 0..<3 {
            for j in 0..<3 {
                r[i * 3 + j] = a[i * 3] * b[j] + a[i * 3 + 1] * b[3 + j] + a[i * 3 + 2] * b[6 + j]
            }
        }
        return r
    }

    static func transpose(_ a: [Double]) -> [Double] {
        [a[0], a[3], a[6], a[1], a[4], a[7], a[2], a[5], a[8]]
    }
}

import Foundation

/// CPU reference for `eyeDraw`'s per-cell shading. `EyeShader.metal` is a
/// line-by-line port of `shade`; keep the two in step. The app does not call
/// this per frame; tests do.
public enum EyeShading {
    /// `LGT = n3(-0.45, -0.55, 0.7)`.
    public static let light = Vec3.normalized(-0.45, -0.55, 0.7)
    /// `HV = n3(LGT.x, LGT.y, LGT.z + 1)`.
    public static let halfVector = Vec3.normalized(light.x, light.y, light.z + 1)

    public static let irisCos = 0.83
    public static let pupilCos = 0.955
    public static let spokes = 14.0
    public static let specularPower = 48.0
    public static let minBrightness = 0.015

    /// Brightness and iris flag for the sphere point at (dx, dy) in units of
    /// the radius; nil outside the disk.
    public static func shade(dx: Double, dy: Double, gaze g: Vec3, lid: Double, spin: Double) -> (brightness: Double, isIris: Bool)? {
        let d2 = dx * dx + dy * dy
        if d2 > 1 { return nil }
        let z = (1 - d2).squareRoot()
        let lam = max(0, dx * light.x + dy * light.y + z * light.z)
        let sp = pow(max(0, dx * halfVector.x + dy * halfVector.y + z * halfVector.z), specularPower)
        if abs(dy) > 1 - lid {
            return (0.05 + 0.3 * lam, false)
        }
        let ca = dx * g.x + dy * g.y + z * g.z
        if ca > pupilCos {
            return (sp * 0.35, false)
        }
        if ca > irisCos {
            let ang = atan2(dy - g.y * ca, dx - g.x * ca)
            let b = (0.32 + 0.48 * lam) * (0.72 + 0.28 * sin(ang * spokes + spin)) * InkMath.smoothstep(0.83, 0.85, ca) + sp * 0.3
            return (b, true)
        }
        return ((0.08 + 0.72 * lam) * (0.55 + 0.45 * z.squareRoot()) + sp * 0.3, false)
    }

    /// `put`: dot radius `cell/2 * min(1.05, sqrt(b))`, nil below 0.015.
    public static func dotRadius(cell: Double, brightness b: Double) -> Double? {
        if b < minBrightness { return nil }
        return cell / 2 * min(1.05, b.squareRoot())
    }
}

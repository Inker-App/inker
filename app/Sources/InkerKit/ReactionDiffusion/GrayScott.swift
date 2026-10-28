import Foundation

/// `makeRD` from AppPopEmpty / BrandMaterial: Gray-Scott reaction-diffusion on
/// a wrapping CPU grid, f = 0.0545, k = 0.062, Du = 1, Dv = 0.5, dt = 1, with
/// the boards' 9-point Laplacian (0.2 edges, 0.05 corners).
///
/// One deliberate change from the board: U and V are clamped to [0, 1] after
/// each update. The board's `drop` sets V = 1 while U stays 1, so the first
/// steps overshoot and the field diverges to NaN within ~300 steps; in a
/// browser canvas silently ignores NaN arcs, so the board shows no pattern.
/// With the clamp the pattern grows as the board's aria-label describes.
/// Storage is Float (the board uses Float32Array); arithmetic is Double, like JS.
public struct GrayScott: Sendable {
    public static let feed = 0.0545
    public static let kill = 0.062
    /// The empty drop zone grid: 328/4 by 384/4.
    public static let emptyZoneGrid = (width: 82, height: 96)

    public let width: Int
    public let height: Int
    public private(set) var u: [Float]
    public private(set) var v: [Float]
    private var u2: [Float]
    private var v2: [Float]

    /// `makeRD(gw, gh, seedCount, seed)`: `seedCount` 3x3 drops at random cells.
    public init(width: Int, height: Int, seedCount: Int, seed: UInt32) {
        self.width = width
        self.height = height
        let n = width * height
        u = [Float](repeating: 1, count: n)
        v = [Float](repeating: 0, count: n)
        u2 = [Float](repeating: 0, count: n)
        v2 = [Float](repeating: 0, count: n)
        var r = SeededRandom(seed: seed)
        for _ in 0..<seedCount {
            let x = Int(r.next() * Double(width))
            let y = Int(r.next() * Double(height))
            drop(x: x, y: y, size: 1)
        }
    }

    /// `drop(cx, cy, s)`: set V = 1 on a (2s+1)^2 square, wrapping.
    public mutating func drop(x cx: Int, y cy: Int, size s: Int) {
        if s < 0 { return }
        for dy in -s...s {
            for dx in -s...s {
                let x = ((cx + dx) % width + width) % width
                let y = ((cy + dy) % height + height) % height
                v[y * width + x] = 1
            }
        }
    }

    public mutating func step() {
        let f = GrayScott.feed
        let k = GrayScott.kill
        let w = width
        let h = height
        for y in 0..<h {
            let ym = ((y - 1 + h) % h) * w
            let y0 = y * w
            let yp = ((y + 1) % h) * w
            for x in 0..<w {
                let xm = (x - 1 + w) % w
                let xp = (x + 1) % w
                let i = y0 + x
                // Split up so the type checker does not time out.
                let edgesU: Double = Double(u[y0 + xm]) + Double(u[y0 + xp]) + Double(u[ym + x]) + Double(u[yp + x])
                let cornersU: Double = Double(u[ym + xm]) + Double(u[ym + xp]) + Double(u[yp + xm]) + Double(u[yp + xp])
                let edgesV: Double = Double(v[y0 + xm]) + Double(v[y0 + xp]) + Double(v[ym + x]) + Double(v[yp + x])
                let cornersV: Double = Double(v[ym + xm]) + Double(v[ym + xp]) + Double(v[yp + xm]) + Double(v[yp + xp])
                let lu: Double = -Double(u[i]) + 0.2 * edgesU + 0.05 * cornersU
                let lv: Double = -Double(v[i]) + 0.2 * edgesV + 0.05 * cornersV
                let uu = Double(u[i])
                let vv = Double(v[i])
                let uvv = uu * vv * vv
                let nu = uu + (lu - uvv + f * (1 - uu))
                let nv = vv + (0.5 * lv + uvv - (k + f) * vv)
                u2[i] = Float(InkMath.clamp(nu, 0, 1))
                v2[i] = Float(InkMath.clamp(nv, 0, 1))
            }
        }
        swap(&u, &u2)
        swap(&v, &v2)
    }

    /// The zone draws each cell as a dot of brightness `ss(0.1, 0.28, V) * 0.9`.
    public static func dotBrightness(_ value: Float) -> Double {
        InkMath.smoothstep(0.1, 0.28, Double(value)) * 0.9
    }
}

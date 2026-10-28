import InkerKit
import SwiftUI

/// The three looping panels of AppOnboard3 (`start` in its script), minus the
/// eye, which EyeLayer draws on top (PasteScene).
struct HowItWorksCanvas: View {
    private static let panelX: [Double] = [0, 164, 328]
    private static let panelWidth = 152.0
    /// `hr = rng(5)`: 20 rim-dot sizes.
    private static let rimSizes: [Double] = {
        var r = SeededRandom(seed: 5)
        return (0..<20).map { _ in r.next() }
    }()

    var body: some View {
        TimedCanvas { context, size, t in
            for x in HowItWorksCanvas.panelX {
                context.fill(Path(roundedRect: CGRect(x: x, y: 0, width: HowItWorksCanvas.panelWidth, height: Double(size.height)), cornerRadius: 8),
                             with: .color(Palette.panel))
            }
            drawDrop(&context, t)
            drawCopy(&context, t)
            drawPaste(&context)
        }
    }

    /// Panel 1: a page of ink dissolves into dots that fly down and pool into a handle.
    private func drawDrop(_ context: inout GraphicsContext, _ t: Double) {
        let ss = InkMath.smoothstep
        var src = InkField(bitmap: InkBitmap(width: 3, height: 4), cell: 10, originX: HowItWorksCanvas.panelX[0] + 61, originY: 18)
        var tgt = InkField(bitmap: InkBitmap(width: 8, height: 1), cell: 10, originX: HowItWorksCanvas.panelX[0] + 36, originY: 104)
        let u = InkMath.jsMod(t, 4.6)
        let r = 5 - 0.5
        if u < 1.2 {
            for j in 0..<12 {
                src.values[j] = ss(0, 0.35, u - Double(j) * 0.02) * (1 - 0.4 * ss(0.85, 1.15, u))
            }
            context.fill(InkPaths.path(src), with: .color(Palette.paper))
        }
        if u >= 1.15 && u < 2.5 {
            var p = Path()
            for j in 0..<8 {
                let pr = ss(0, 1, (u - 1.15 - Double(j) * 0.04) / 0.85)
                let a = src.center(j), c = tgt.center(j)
                let px = Double(a.x) + Double(c.x - a.x) * pr + sin(Double.pi * pr) * (j % 2 == 1 ? 10 : -10)
                let py = Double(a.y) + Double(c.y - a.y) * pr
                let rr = r * 0.6
                p.addEllipse(in: CGRect(x: px - rr, y: py - rr, width: rr * 2, height: rr * 2))
            }
            context.fill(p, with: .color(Palette.paper))
        }
        if u >= 2.5 {
            for j in 0..<8 {
                tgt.values[j] = (0.6 + 0.4 * ss(0, 0.35, u - 2.5 - Double(j) * 0.02)) * (1 - ss(4.0, 4.5, u))
            }
            context.fill(InkPaths.path(tgt), with: .color(Palette.paper))
            var c = context
            c.opacity = ss(2.8, 3.0, u) * (1 - ss(4.0, 4.3, u))
            c.draw(Text("🐙/w2-acme").font(Typography.mono(10, weight: 600)).foregroundColor(Palette.paper),
                   at: CGPoint(x: HowItWorksCanvas.panelX[0] + 76, y: 134 + 3), anchor: .bottom)
        }
    }

    /// Panel 2: the handle pill, "copied", and a burst of dots.
    private func drawCopy(_ context: inout GraphicsContext, _ t: Double) {
        let ss = InkMath.smoothstep
        let u2 = InkMath.jsMod(t, 3.2)
        let cx = HowItWorksCanvas.panelX[1] + 76, cy = 75.0
        context.stroke(Path(roundedRect: CGRect(x: cx - 58, y: cy - 14, width: 116, height: 28), cornerRadius: 14),
                       with: .color(Palette.paper.opacity(0.35)), lineWidth: 1)
        context.draw(Text("🐙/w2-acme-2025").font(Typography.mono(11, weight: 600)).foregroundColor(Palette.paper),
                     at: CGPoint(x: cx, y: cy), anchor: .center)
        var c = context
        c.opacity = ss(0.4, 0.55, u2) * (1 - ss(1.6, 1.9, u2))
        c.draw(Text("copied").font(Typography.archivo(10, weight: 500)).foregroundColor(Palette.muted),
               at: CGPoint(x: cx, y: cy + 34 + 3), anchor: .bottom)
        if u2 > 0.4 && u2 < 1.4 {
            let e2 = u2 - 0.4
            let dd = ss(0, 0.8, e2) * 26
            let br = 2.2 * (1 - ss(0.2, 1, e2))
            if br > 0.2 {
                var p = Path()
                for k in 0..<18 {
                    let an = Double(k) / 18 * 6.2832
                    let bx = cx + cos(an) * (58 + dd)
                    let by = cy + sin(an) * (14 + dd * 0.5)
                    p.addEllipse(in: CGRect(x: bx - br, y: by - br, width: br * 2, height: br * 2))
                }
                context.fill(p, with: .color(Palette.paper))
            }
        }
    }

    /// Panel 3: "SSN" and an inked pill with a ragged rim (the eye is EyeLayer).
    private func drawPaste(_ context: inout GraphicsContext) {
        let lx = PasteScene.lineX, ly = PasteScene.lineY, sx = PasteScene.pillX, sw = PasteScene.pillWidth
        context.draw(Text("SSN").font(Typography.mono(11, weight: 500)).foregroundColor(Palette.paper),
                     at: CGPoint(x: lx, y: ly + 4 + 3), anchor: .bottomLeading)
        context.fill(Path(roundedRect: CGRect(x: sx, y: ly - 8, width: sw, height: 16), cornerRadius: 8), with: .color(.black))
        var rim = Path()
        for m in 0..<20 {
            let mx = sx + 4 + Double(m % 10) * (sw - 8) / 9
            let my = m < 10 ? ly - 10 : ly + 10
            let r = 0.8 + HowItWorksCanvas.rimSizes[m] * 1.2
            rim.addEllipse(in: CGRect(x: mx - r, y: my - r, width: r * 2, height: r * 2))
        }
        context.fill(rim, with: .color(Palette.rim))
    }
}

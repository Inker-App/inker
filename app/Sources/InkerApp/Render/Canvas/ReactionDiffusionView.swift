import InkerKit
import SwiftUI

/// Gray-Scott state for the empty drop zone. Stepped on the CPU, 4 steps per
/// drawn frame as in `startZone`; the cursor drips ink into it.
final class ReactionDiffusionModel: ObservableObject {
    static let cell = 4.0
    private(set) var field: GrayScott
    private var lastTime: Double = -1

    init() {
        let grid = GrayScott.emptyZoneGrid
        field = GrayScott(width: grid.width, height: grid.height, seedCount: 10, seed: 6)
        for _ in 0..<300 { field.step() }
    }

    /// Advance once per distinct frame time. (Drawn frames are capped at 30 fps
    /// where the board ran at the display rate, so the pattern grows at about
    /// half the canvas speed; raise `stepsPerFrame` to match if wanted.)
    static let stepsPerFrame = 4

    func advance(to t: Double, pointer: CGPoint?) {
        guard t != lastTime else { return }
        lastTime = t
        if let p = pointer {
            field.drop(x: Int(Double(p.x) / ReactionDiffusionModel.cell), y: Int(Double(p.y) / ReactionDiffusionModel.cell), size: 0)
        }
        for _ in 0..<ReactionDiffusionModel.stepsPerFrame { field.step() }
    }
}

/// The empty zone's background: panel, the faint ink field at 9% alpha and
/// the dashed inset border (`startZone`).
struct ReactionDiffusionView: View {
    @StateObject private var model = ReactionDiffusionModel()
    let pointer: PointerBox
    var highlighted = false

    var body: some View {
        TimedCanvas { context, size, t in
            model.advance(to: t, pointer: pointer.point)
            let rect = CGRect(origin: .zero, size: size)
            context.fill(Path(roundedRect: rect, cornerRadius: 10), with: .color(Palette.panel))

            let c = ReactionDiffusionModel.cell
            let f = model.field
            var dots = Path()
            for j in 0..<f.height {
                for i in 0..<f.width {
                    let b = GrayScott.dotBrightness(f.v[j * f.width + i])
                    InkPaths.addDot(&dots, x: (Double(i) + 0.5) * c, y: (Double(j) + 0.5) * c, brightness: b, cell: c)
                }
            }
            context.fill(dots, with: .color(Palette.paper.opacity(0.09)))

            let inset = Path(roundedRect: rect.insetBy(dx: 6, dy: 6), cornerRadius: 8)
            // The board's border is paper at 0.35; brighter while a drag hovers
            // (not on the boards; Ben to confirm).
            let alpha = highlighted ? 0.7 : 0.35
            context.stroke(inset, with: .color(Palette.paper.opacity(alpha)),
                           style: StrokeStyle(lineWidth: 1.25, dash: [5, 6]))
        }
    }
}

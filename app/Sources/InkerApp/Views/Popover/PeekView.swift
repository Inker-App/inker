import InkerKit
import SwiftUI

/// The 302x84 "peek": an eye squinting at a dotted outline where the held
/// value sits. The value is never shown (it never leaves the core).
struct PeekView: View {
    let contextBefore: String
    let valueLength: Int
    /// JetBrains Mono advance is 600/1000 em: 7.2 pt at 12 pt.
    static let characterWidth = 7.2

    var body: some View {
        ZStack {
            TimedCanvas { context, _, t in
                let scene = PeekScene(valueLength: valueLength, characterWidth: PeekView.characterWidth)
                if !contextBefore.isEmpty {
                    // VERIFY: anchor .bottomLeading puts the text's bottom (not
                    // its baseline) at the point; +3 approximates the descent so
                    // the baseline lands on the board's y = 30.
                    let origin = PeekScene.contextOrigin
                    context.draw(Text(contextBefore).font(Typography.mono(12, weight: 500)).foregroundColor(Palette.muted),
                                 at: CGPoint(x: origin.x, y: origin.y + 3), anchor: .bottomLeading)
                }
                let outline = scene.outline
                let count = Int((Double(outline.width + outline.height) * 2 / 6).rounded(.down))
                context.fill(InkPaths.dottedRect(outline, count: count, radius: 1.1),
                             with: .color(Palette.paper.opacity(PeekScene.outlineAlpha(time: t))))
            }
            EyeLayer(makeScene: { PeekScene(valueLength: valueLength, characterWidth: PeekView.characterWidth) })
        }
        .frame(width: 302, height: 84)
        .background(RoundedRectangle(cornerRadius: 6).fill(Palette.panel))
        .accessibilityElement()
        .accessibilityLabel("An eyeball squints at a value the scanner wasn't sure about")
    }
}

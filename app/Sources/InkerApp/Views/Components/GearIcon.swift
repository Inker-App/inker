import SwiftUI

/// The settings gear from the boards' 16x16 SVG: a r=2.2 circle and eight spokes.
struct GearIcon: Shape {
    func path(in rect: CGRect) -> Path {
        let s = min(rect.width, rect.height) / 16
        func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: rect.minX + x * s, y: rect.minY + y * s) }
        var p = Path()
        p.addEllipse(in: CGRect(x: rect.minX + (8 - 2.2) * s, y: rect.minY + (8 - 2.2) * s, width: 4.4 * s, height: 4.4 * s))
        let spokes: [(CGFloat, CGFloat, CGFloat, CGFloat)] = [
            (8, 1.5, 8, 3.5), (8, 12.5, 8, 14.5), (1.5, 8, 3.5, 8), (12.5, 8, 14.5, 8),
            (3.4, 3.4, 4.8, 4.8), (11.2, 11.2, 12.6, 12.6), (3.4, 12.6, 4.8, 11.2), (11.2, 4.8, 12.6, 3.4),
        ]
        for (x1, y1, x2, y2) in spokes {
            p.move(to: pt(x1, y1))
            p.addLine(to: pt(x2, y2))
        }
        return p
    }
}

struct SettingsButton: View {
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            GearIcon()
                .stroke(Palette.muted, lineWidth: 1.5)
                .frame(width: 16, height: 16)
                .frame(width: 32, height: 32)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Settings")
    }
}

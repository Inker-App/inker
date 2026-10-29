import SwiftUI

/// The header's folder outline (16x16 SVG from the find handoff).
struct FolderIcon: Shape {
    func path(in rect: CGRect) -> Path {
        let s = min(rect.width, rect.height) / 16
        func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: rect.minX + x * s, y: rect.minY + y * s) }
        var p = Path()
        // M1.5 4.5 A1.5 1.5 0 0 1 3 3 h3.2 l1.6 1.8 H13 a1.5 1.5 0 0 1 1.5 1.5 v6
        // a1.5 1.5 0 0 1 -1.5 1.5 H3 a1.5 1.5 0 0 1 -1.5 -1.5 z
        p.move(to: pt(1.5, 4.5))
        p.addArc(center: pt(3, 4.5), radius: 1.5 * s, startAngle: .degrees(180), endAngle: .degrees(270), clockwise: false)
        p.addLine(to: pt(6.2, 3))
        p.addLine(to: pt(7.8, 4.8))
        p.addLine(to: pt(13, 4.8))
        p.addArc(center: pt(13, 6.3), radius: 1.5 * s, startAngle: .degrees(270), endAngle: .degrees(360), clockwise: false)
        p.addLine(to: pt(14.5, 12.3))
        p.addArc(center: pt(13, 12.3), radius: 1.5 * s, startAngle: .degrees(0), endAngle: .degrees(90), clockwise: false)
        p.addLine(to: pt(3, 13.8))
        p.addArc(center: pt(3, 12.3), radius: 1.5 * s, startAngle: .degrees(90), endAngle: .degrees(180), clockwise: false)
        p.closeSubpath()
        return p
    }
}

/// Two sliders: the settings icon that replaced the gear (which read as a
/// light-mode sun). Lines at y 4.5 and 11.5, knobs at (6, 4.5) and (10.5, 11.5).
struct SlidersIcon: View {
    var color: Color = Palette.muted

    var body: some View {
        Canvas { context, size in
            let s = min(size.width, size.height) / 16
            func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: x * s, y: y * s) }
            var lines = Path()
            lines.move(to: pt(2, 4.5)); lines.addLine(to: pt(14, 4.5))
            lines.move(to: pt(2, 11.5)); lines.addLine(to: pt(14, 11.5))
            context.stroke(lines, with: .color(color), style: StrokeStyle(lineWidth: 1.5 * s, lineCap: .round))
            for (x, y) in [(6.0, 4.5), (10.5, 11.5)] {
                let knob = Path(ellipseIn: CGRect(x: (x - 1.8) * s, y: (y - 1.8) * s, width: 3.6 * s, height: 3.6 * s))
                context.fill(knob, with: .color(Palette.background))
                context.stroke(knob, with: .color(color), lineWidth: 1.5 * s)
            }
        }
    }
}

/// A 32x32 hit area around a 16x16 icon.
struct HeaderIconButton<Icon: View>: View {
    let label: String
    var tooltip: String?
    let action: () -> Void
    @ViewBuilder let icon: () -> Icon

    var body: some View {
        Button(action: action) {
            icon()
                .frame(width: 16, height: 16)
                .frame(width: 32, height: 32)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .help(tooltip ?? label)
    }
}

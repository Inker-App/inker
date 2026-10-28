import SwiftUI

/// "Drop more": 44 tall, dashed paper 0.3 border, radius 8.
struct DropMoreStrip: View {
    var highlighted = false

    var body: some View {
        Text("Drop more")
            .font(Typography.small)
            .foregroundColor(Palette.muted)
            .frame(maxWidth: .infinity)
            .frame(height: 44)
            .overlay(
                RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(Palette.paper.opacity(highlighted ? 0.6 : 0.3), style: StrokeStyle(lineWidth: 1, dash: [3, 3]))
            )
    }
}

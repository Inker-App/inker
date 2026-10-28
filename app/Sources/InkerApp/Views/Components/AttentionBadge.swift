import SwiftUI

/// The paper pill with the needs-you count (Attention board header).
struct AttentionBadge: View {
    let count: Int

    var body: some View {
        Text("\(count)")
            .font(Typography.archivo(11, weight: 700))
            .foregroundColor(Palette.background)
            .padding(.horizontal, 5)
            .frame(minWidth: 18, minHeight: 18, maxHeight: 18)
            .background(Capsule().fill(Palette.paper))
            .accessibilityLabel(count == 1 ? "1 item needs you" : "\(count) items need you")
    }
}

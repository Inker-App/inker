import InkerKit
import SwiftUI

/// 36 tall (40 with the peeker on the Ready board), 11 pt muted. The
/// last-read handle is a button that locates its row (find handoff, 6).
struct PopoverFooter: View {
    let model: FooterModel
    var onLocate: ((String) -> Void)?
    /// "Claude not connected": opens Settings at Chat apps.
    var onConnect: (() -> Void)?

    var body: some View {
        HStack(spacing: 8) {
            switch model.left {
            case .text(let text):
                Text(text)
                Spacer(minLength: 8)
                right
            case .lastRead(let agent, let handle, let ago):
                if model.showsPeeker {
                    EyeLayer(makeScene: { PeekerScene() })
                        .frame(width: 24, height: 24)
                        .accessibilityHidden(true)
                }
                Text("\(agent) read")
                Button { onLocate?(handle) } label: {
                    Text(handle)
                        .font(Typography.mono(11))
                        .foregroundColor(Palette.paper)
                        .padding(.horizontal, 6)
                        .frame(height: 22)
                        .background(RoundedRectangle(cornerRadius: 4).fill(Palette.hairlineFaint))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Find \(handle) in the list")
                Text("· \(ago)")
                Spacer(minLength: 8)
                right
            }
        }
        .font(Typography.caption)
        .foregroundColor(Palette.muted)
        .padding(.horizontal, 16)
        .frame(height: model.showsPeeker ? 40 : 36)
    }

    @ViewBuilder private var right: some View {
        if model.rightIsConnectLink {
            Button { onConnect?() } label: {
                Text(model.right).underline().lineLimit(1)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Claude is not connected. Open Settings to connect it")
        } else {
            Text(model.right).lineLimit(1)
        }
    }
}

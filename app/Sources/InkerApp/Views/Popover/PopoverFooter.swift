import InkerKit
import SwiftUI

/// 36 tall (40 with the peeker on the Ready board), 11 pt muted.
struct PopoverFooter: View {
    let model: FooterModel

    var body: some View {
        HStack(spacing: 8) {
            switch model.left {
            case .text(let text):
                Text(text)
                Spacer(minLength: 8)
                Text(model.right)
            case .lastRead(let agent, let handle, let ago):
                if model.showsPeeker {
                    EyeLayer(makeScene: { PeekerScene() })
                        .frame(width: 24, height: 24)
                        .accessibilityHidden(true)
                }
                (Text("\(agent) read ")
                    + Text(handle).font(Typography.mono(11)).foregroundColor(Palette.paper)
                    + Text(", saw only ink · \(ago)"))
                    .lineLimit(1)
                Spacer(minLength: 0)
            }
        }
        .font(Typography.caption)
        .foregroundColor(Palette.muted)
        .padding(.horizontal, 16)
        .frame(height: model.showsPeeker ? 40 : 36)
    }
}

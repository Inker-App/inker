import SwiftUI

/// The boards' three button looks.
struct InkButtonStyle: ButtonStyle {
    enum Kind {
        /// Paper fill, background-colour text, weight 600 ("Ink it", "Move", "Continue", "Copied").
        case primary
        /// Transparent with a paper outline ("Copy", "Leave it", "Copy instead").
        case outline(opacity: Double)
        /// Muted, underlined ("Open file", "Copy config").
        case link
    }

    var kind: Kind
    var height: CGFloat
    var horizontalPadding: CGFloat
    var fontSize: CGFloat = 12
    var radius: CGFloat = 6
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        let pressed = configuration.isPressed
        return label(configuration.label)
            .frame(height: height)
            .padding(.horizontal, horizontalPadding)
            .background(background)
            .overlay(border)
            .contentShape(Rectangle())
            .opacity(pressed ? 0.75 : 1)
    }

    @ViewBuilder
    private func label(_ l: Configuration.Label) -> some View {
        switch kind {
        case .primary:
            l.font(Typography.archivo(fontSize, weight: 600)).foregroundColor(Palette.background)
        case .outline:
            l.font(Typography.archivo(fontSize)).foregroundColor(isEnabled ? Palette.paper : Palette.disabled)
        case .link:
            l.font(Typography.archivo(fontSize)).foregroundColor(Palette.muted).underline()
        }
    }

    @ViewBuilder
    private var background: some View {
        switch kind {
        case .primary: RoundedRectangle(cornerRadius: radius).fill(Palette.paper)
        case .outline, .link: Color.clear
        }
    }

    @ViewBuilder
    private var border: some View {
        switch kind {
        case .outline(let opacity):
            // Disabled outline buttons use the hairline (Processing board).
            RoundedRectangle(cornerRadius: radius)
                .strokeBorder(Palette.paper.opacity(isEnabled ? opacity : 0.14), lineWidth: 1)
        case .primary, .link:
            EmptyView()
        }
    }
}

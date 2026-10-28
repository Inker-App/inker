import InkerKit
import SwiftUI
import UniformTypeIdentifiers

/// The 360x500 popover: header, one of the four boards, footer.
struct PopoverRootView: View {
    @EnvironmentObject private var store: CoreStore
    var onSettings: () -> Void = {}

    var body: some View {
        VStack(spacing: 0) {
            PopoverHeader(badgeCount: store.model.mode == .attention ? store.model.badgeCount : 0, onSettings: onSettings)
            Hairline()
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            Hairline()
            PopoverFooter(model: store.model.footer)
        }
        .frame(width: 360, height: 500)
        .background(Palette.background)
        .foregroundColor(Palette.paper)
        .font(Typography.body)
        .onDrop(of: [UTType.fileURL], isTargeted: $store.isDropTargeted) { providers in
            store.acceptDrop(providers)
        }
    }

    @ViewBuilder
    private var content: some View {
        switch store.model.mode {
        case .empty:
            EmptyBoard()
        case .processing, .ready:
            QueueBoard()
        case .attention:
            AttentionBoard()
        }
    }
}

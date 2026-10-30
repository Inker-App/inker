import InkerKit
import SwiftUI
import UniformTypeIdentifiers

/// The 360x500 popover, and the whole app: header, the current screen (a
/// board, an onboarding step or settings), footer. A pending drop puts the
/// move-or-copy card in front of the screen; the screen stays underneath so
/// an onboarding step keeps what was typed into it.
struct PopoverRootView: View {
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        VStack(spacing: 0) {
            PopoverHeader(badgeCount: badgeCount, folderTooltip: "Open \(store.privateFolderDisplay)",
                          onOpenFolder: { store.openPrivateFolder() }, onSettings: { store.toggleSettings() })
            Hairline()
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                .clipped()
            Hairline()
            PopoverFooter(model: store.model.footer, onLocate: { handle in
                store.show(.board)
                store.locate(handle: handle)
            }, onConnect: { store.openSettings() })
        }
        .frame(width: PopoverLayout.width, height: PopoverLayout.height)
        .background(Palette.background)
        .foregroundColor(Palette.paper)
        .font(Typography.body)
        .onDrop(of: [UTType.fileURL], isTargeted: $store.isDropTargeted) { providers in
            store.acceptDrop(providers)
        }
    }

    private var badgeCount: Int {
        store.screen == .board && store.model.mode == .attention ? store.model.badgeCount : 0
    }

    private var cardShowing: Bool { store.surface == .dropCard }

    private var content: some View {
        ZStack(alignment: .top) {
            screen
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                // Disabled, not just covered: its Return shortcut must not
                // compete with the card's.
                .disabled(cardShowing)
                .accessibilityHidden(cardShowing)
            if let batch = store.pendingDrop {
                DropDecisionCard(batch: batch) { mode, always in store.answerDrop(mode, always: always) }
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    .background(Palette.background)
            }
        }
    }

    @ViewBuilder
    private var screen: some View {
        switch store.screen {
        case .board:
            board
        case .onboarding(let step):
            OnboardingScreen(step: step)
        case .settings:
            SettingsView()
        }
    }

    @ViewBuilder
    private var board: some View {
        switch store.model.mode {
        case .empty:
            EmptyBoard()
        case .processing:
            QueueBoard()
        case .ready:
            FindBoard()
        case .attention:
            AttentionBoard()
        }
    }
}

/// The popover's fixed size and the content area between header and footer.
enum PopoverLayout {
    static let width: CGFloat = 360
    static let height: CGFloat = 500
    /// 16 pt either side.
    static let gutter: CGFloat = 16
    /// 328: what the boards' 480-wide onboarding scenes are scaled to.
    static let innerWidth: CGFloat = width - gutter * 2
}

/// A scene drawn at its board size (480x120, 480x150, 392x76) and scaled
/// down to `width`. The Metal eye and octopus renderers work in the scene's
/// own points, so the whole scene is scaled rather than laid out narrower.
struct ScaledScene<Content: View>: View {
    let size: CGSize
    var width: CGFloat = PopoverLayout.innerWidth
    let content: Content

    init(size: CGSize, width: CGFloat = PopoverLayout.innerWidth, @ViewBuilder content: () -> Content) {
        self.size = size
        self.width = width
        self.content = content()
    }

    var body: some View {
        let k = width / size.width
        // VERIFY: scaleEffect reaches the MTKViews inside (as it does for the
        // drop card's DialogScene); if it does not, the eyes draw unscaled
        // and the right of the scene is cut off.
        content
            .frame(width: size.width, height: size.height)
            .scaleEffect(k, anchor: .topLeading)
            .frame(width: width, height: size.height * k, alignment: .topLeading)
    }
}

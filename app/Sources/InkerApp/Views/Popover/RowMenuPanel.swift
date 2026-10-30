import InkerKit
import SwiftUI

/// Where each row sits on the board, so the open menu can hang off its row.
struct RowAnchorKey: PreferenceKey {
    static let defaultValue: [String: Anchor<CGRect>] = [:]

    static func reduce(value: inout [String: Anchor<CGRect>], nextValue: () -> [String: Anchor<CGRect>]) {
        value.merge(nextValue()) { $1 }
    }
}

/// Draws the open row menu over the board, right-aligned under its row
/// (above it when it would not fit). A click anywhere else closes it, as
/// a menu would, and the click goes no further.
struct RowMenuLayer: View {
    let anchors: [String: Anchor<CGRect>]
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        GeometryReader { geo in
            if let id = store.openMenuID, let anchor = anchors[id], let target = store.menuTarget(for: id) {
                let rect = geo[anchor]
                let placement = RowMenu.placement(rowMaxY: rect.maxY, panelHeight: RowMenu.estimatedHeight,
                                                  boardHeight: geo.size.height)
                ZStack {
                    Color.clear
                        .contentShape(Rectangle())
                        .onTapGesture { store.closeMenu() }
                    RowMenuPanel(id: id, target: target)
                        .frame(width: RowMenu.width)
                        .frame(maxWidth: .infinity, maxHeight: .infinity,
                               alignment: placement == .below ? .topTrailing : .bottomTrailing)
                        .offset(x: rect.maxX - geo.size.width,
                                y: placement == .below ? rect.maxY + RowMenu.gap : rect.minY - RowMenu.gap - geo.size.height)
                }
            }
        }
    }
}

/// The panel as the artboard draws it: 200 wide, 6 of padding, the panel
/// black, a 0.2 paper border, items 7 by 10 with a 5-point hover fill.
struct RowMenuPanel: View {
    let id: String
    let target: RowMenuTarget
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        let items = RowMenu.items(target.menu, confirmingForget: store.confirmingForgetID == id)
        VStack(spacing: 0) {
            ForEach(items) { item in
                if item.separatorAbove {
                    Hairline(color: Palette.hairlineFaint).padding(.vertical, 4)
                }
                RowMenuItemView(item: item) {
                    store.perform(item.action, id: id, target: target)
                }
            }
        }
        .padding(6)
        .background(RoundedRectangle(cornerRadius: 8).fill(Palette.panel))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.field, lineWidth: 1))
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Row menu")
    }
}

/// One item. "Are you sure?" is red on a red tint so it reads as a
/// different item, not the same one with new text.
struct RowMenuItemView: View {
    let item: RowMenuItem
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(item.title)
                .font(Typography.small)
                .foregroundColor(item.isDestructive ? Palette.red : Palette.checkLabel)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.vertical, 7)
                .padding(.horizontal, 10)
                .background(RoundedRectangle(cornerRadius: 5).fill(fill))
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }

    private var fill: Color {
        if item.isDestructive { return Palette.red.opacity(hovering ? 0.22 : 0.12) }
        return hovering ? Palette.paper.opacity(0.08) : .clear
    }
}

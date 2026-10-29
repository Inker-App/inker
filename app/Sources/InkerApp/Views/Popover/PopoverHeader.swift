import SwiftUI

/// 48 tall: lockup (+ badge on the Attention board), then two icon buttons
/// that are always there: open the private folder in Finder, and settings.
struct PopoverHeader: View {
    let badgeCount: Int
    /// "Open ~/Private", with the real path.
    let folderTooltip: String
    let onOpenFolder: () -> Void
    let onSettings: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            HStack(spacing: 10) {
                LockupView()
                if badgeCount > 0 {
                    AttentionBadge(count: badgeCount)
                }
            }
            Spacer(minLength: 0)
            HStack(spacing: 2) {
                HeaderIconButton(label: "Open your private folder in Finder", tooltip: folderTooltip, action: onOpenFolder) {
                    FolderIcon().stroke(Palette.muted, lineWidth: 1.5)
                }
                HeaderIconButton(label: "Settings", action: onSettings) {
                    SlidersIcon()
                }
            }
        }
        .padding(.leading, 16)
        .padding(.trailing, 12)
        .frame(height: 48)
    }
}

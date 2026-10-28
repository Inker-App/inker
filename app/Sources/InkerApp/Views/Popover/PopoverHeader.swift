import SwiftUI

/// 48 tall: lockup (+ badge on the Attention board) and the settings gear.
struct PopoverHeader: View {
    let badgeCount: Int
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
            SettingsButton(action: onSettings)
        }
        .padding(.leading, 16)
        .padding(.trailing, 12)
        .frame(height: 48)
    }
}

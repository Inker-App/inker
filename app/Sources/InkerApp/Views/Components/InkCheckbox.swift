import SwiftUI

/// A checkbox toggle in the boards' style.
struct InkCheckbox: View {
    @Binding var isOn: Bool
    let label: Text
    var accessibilityLabel: String
    @Environment(\.snapshotMode) private var snapshotMode

    var body: some View {
        if snapshotMode {
            // ImageRenderer cannot draw the AppKit checkbox; a box of the same size.
            HStack(spacing: 6) {
                RoundedRectangle(cornerRadius: 3)
                    .strokeBorder(Palette.field, lineWidth: 1)
                    .frame(width: 14, height: 14)
                label
            }
        } else {
            // VERIFY: `.toggleStyle(.checkbox)` is macOS-only and renders the system
            // checkbox; the boards use the browser default, so this is a match in
            // spirit. Replace with a custom box if Ben wants it drawn.
            Toggle(isOn: $isOn) { label }
                .toggleStyle(.checkbox)
                .accessibilityLabel(accessibilityLabel)
        }
    }
}

import SwiftUI

/// A checkbox toggle in the boards' style.
struct InkCheckbox: View {
    @Binding var isOn: Bool
    let label: Text
    var accessibilityLabel: String

    var body: some View {
        // VERIFY: `.toggleStyle(.checkbox)` is macOS-only and renders the system
        // checkbox; the boards use the browser default, so this is a match in
        // spirit. Replace with a custom box if Ben wants it drawn.
        Toggle(isOn: $isOn) { label }
            .toggleStyle(.checkbox)
            .accessibilityLabel(accessibilityLabel)
    }
}

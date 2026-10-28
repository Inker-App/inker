import SwiftUI

/// "STEP 1 OF 3", "KEEP THESE AGENTS OUT": width 125, weight 600, 0.2em, caps.
struct LabelText: View {
    let text: String
    var color: Color = Palette.paper

    init(_ text: String, color: Color = Palette.paper) {
        self.text = text
        self.color = color
    }

    var body: some View {
        Text(text.uppercased())
            .font(Typography.label)
            .tracking(Typography.labelTracking)
            .foregroundColor(color)
    }
}

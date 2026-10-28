import InkerKit
import SwiftUI

/// AppOnboard3: drop it on the octopus, not in the chat.
struct OnboardStep3: View {
    let onBack: () -> Void
    let onDone: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            OnboardHeading(step: 3, title: "Drop it on the octopus, not in the chat",
                           text: "Instead of dragging a file into Claude or Cursor, drag it onto Inker in your menu bar.")
            ZStack {
                HowItWorksCanvas()
                EyeLayer(makeScene: { PasteScene() })
            }
            .frame(width: 480, height: 150)
            .accessibilityElement()
            .accessibilityLabel("Three looping panels: a file dissolving into a handle, the handle being copied, and an eye squinting at an inked-out line")
            HStack(alignment: .top, spacing: 12) {
                stepColumn("1  Drop", Text("It moves into ~/Private and gets inked."))
                stepColumn("2  Copy", Text("A ") + Text("🐙/").font(Typography.mono(12)).foregroundColor(Palette.paper) + Text(" handle lands on your clipboard."))
                stepColumn("3  Paste", Text("Your agent reads it. The eyes only see ink."))
            }
            Spacer(minLength: 0)
            OnboardNav(onBack: onBack, primary: "Done", onPrimary: onDone)
        }
    }

    private func stepColumn(_ label: String, _ text: Text) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            LabelText(label)
            text.font(Typography.small).foregroundColor(Palette.muted).fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

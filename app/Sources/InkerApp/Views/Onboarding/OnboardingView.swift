import SwiftUI

/// AppOnboard1-3 in one 560x540 window.
struct OnboardingView: View {
    let onDone: () -> Void
    @State private var step = 1

    var body: some View {
        Group {
            switch step {
            case 1: OnboardStep1(onContinue: { step = 2 })
            case 2: OnboardStep2(onBack: { step = 1 }, onContinue: { step = 3 })
            default: OnboardStep3(onBack: { step = 2 }, onDone: onDone)
            }
        }
        .padding(.top, 32)
        .padding(.horizontal, 40)
        .padding(.bottom, 28)
        .frame(width: 560, height: 540, alignment: .top)
        .background(Palette.background)
        .foregroundColor(Palette.paper)
        .font(Typography.body)
    }
}

/// "STEP n OF 3", the heading and the muted paragraph.
struct OnboardHeading: View {
    let step: Int
    let title: String
    let text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            LabelText("Step \(step) of 3", color: Palette.muted)
            Text(title)
                .font(Typography.heading)
                .tracking(22 * 0.01)
            Text(text)
                .foregroundColor(Palette.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// "Back" (muted text) and the primary button, pinned to the bottom.
struct OnboardNav: View {
    var onBack: (() -> Void)?
    let primary: String
    let onPrimary: () -> Void

    var body: some View {
        HStack {
            if let onBack {
                Button("Back", action: onBack)
                    .buttonStyle(.plain)
                    .font(Typography.body)
                    .foregroundColor(Palette.muted)
                    .frame(height: 36)
                    .padding(.horizontal, 12)
            }
            Spacer()
            Button(primary, action: onPrimary)
                .buttonStyle(InkButtonStyle(kind: .primary, height: 36, horizontalPadding: 18, fontSize: 13))
                .keyboardShortcut(.defaultAction)
        }
    }
}

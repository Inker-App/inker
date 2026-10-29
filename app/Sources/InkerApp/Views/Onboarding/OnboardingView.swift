import SwiftUI

/// AppOnboard1-3 as screens of the popover. CoreStore.flow holds the step;
/// each step lays itself out with OnboardPage.
struct OnboardingScreen: View {
    let step: Int

    var body: some View {
        switch step {
        case 1: OnboardStep1()
        case 2: OnboardStep2()
        default: OnboardStep3()
        }
    }
}

/// One onboarding step in the popover's 360x414 content area: the scene,
/// heading and controls scroll if they must, and the nav row is pinned to
/// the bottom so Back and the primary button are always on screen.
struct OnboardPage<Content: View>: View {
    let nav: OnboardNav
    let content: Content

    init(nav: OnboardNav, @ViewBuilder content: () -> Content) {
        self.nav = nav
        self.content = content()
    }

    var body: some View {
        VStack(spacing: 0) {
            PopoverScroll {
                VStack(alignment: .leading, spacing: 12) {
                    content
                }
                .padding(.horizontal, PopoverLayout.gutter)
                .padding(.top, 12)
                .padding(.bottom, 8)
                .frame(width: PopoverLayout.width, alignment: .topLeading)
            }
            .frame(maxHeight: .infinity)
            nav
                .padding(.horizontal, PopoverLayout.gutter)
                .padding(.top, 8)
                .padding(.bottom, 12)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }
}

/// "STEP n OF 3", the heading and the muted paragraph. The boards' 22 pt
/// heading is set at 18 pt to fit the popover's 328-point column.
struct OnboardHeading: View {
    let step: Int
    let title: String
    let text: String

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            LabelText("Step \(step) of 3", color: Palette.muted)
            Text(title)
                .font(Typography.popoverHeading)
                .tracking(18 * 0.01)
                .fixedSize(horizontal: false, vertical: true)
            Text(text)
                .foregroundColor(Palette.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// "Back" (muted text) and the primary button.
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
                    .padding(.trailing, 12)
            }
            Spacer()
            Button(primary, action: onPrimary)
                .buttonStyle(InkButtonStyle(kind: .primary, height: 36, horizontalPadding: 18, fontSize: 13))
                .keyboardShortcut(.defaultAction)
        }
        .frame(height: 36)
    }
}

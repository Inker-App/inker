import AppKit
import SwiftUI

/// The 560x540 onboarding window (AppOnboard1-3).
@MainActor
final class OnboardingWindowController {
    private var panel: InkPanel?

    /// `onFinish` runs when Done is pressed; `onClose` whenever the window
    /// goes away, Done or the close button.
    init(store: CoreStore, clock: RenderClock, onFinish: @escaping () -> Void, onClose: @escaping () -> Void) {
        var finish: () -> Void = {}
        let view = OnboardingView(onDone: { finish() })
            .environmentObject(store)
            .environmentObject(clock)
        let panel = InkPanel(size: NSSize(width: 560, height: 540), clock: clock, floating: false, content: AnyView(view))
        panel.window.title = "Welcome to INKER"
        panel.onClose = onClose
        self.panel = panel
        // Close first (windowWillClose releases the RenderClock and fires
        // onClose), then mark onboarding complete.
        finish = { [weak panel] in
            panel?.close()
            onFinish()
        }
    }

    func show() {
        panel?.show()
    }
}

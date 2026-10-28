import AppKit
import SwiftUI

/// The 560x540 onboarding window (AppOnboard1-3).
@MainActor
final class OnboardingWindowController {
    private var panel: InkPanel?

    init(store: CoreStore, clock: RenderClock, onFinish: @escaping () -> Void) {
        var finish: () -> Void = {}
        let view = OnboardingView(onDone: { finish() })
            .environmentObject(store)
            .environmentObject(clock)
        let panel = InkPanel(size: NSSize(width: 560, height: 540), clock: clock, floating: false, content: AnyView(view))
        panel.window.title = "Welcome to INKER"
        self.panel = panel
        // Close first (windowWillClose releases the RenderClock), then let the
        // owner drop us; the owner defers that release past this call stack.
        finish = { [weak panel] in
            panel?.close()
            onFinish()
        }
    }

    func show() {
        panel?.show()
    }
}

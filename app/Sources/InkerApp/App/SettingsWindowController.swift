import AppKit
import SwiftUI

/// The settings window: handle style and the coding-agent section. There is
/// no board for it yet; it borrows the onboarding window's styling.
@MainActor
final class SettingsWindowController {
    private var panel: InkPanel?

    init(store: CoreStore, clock: RenderClock, onClose: @escaping () -> Void) {
        let view = SettingsView()
            .environmentObject(store)
            .environmentObject(clock)
        let panel = InkPanel(size: NSSize(width: 560, height: 400), clock: clock, floating: false, content: AnyView(view))
        panel.window.title = "INKER Settings"
        panel.onClose = onClose
        self.panel = panel
    }

    func show() {
        panel?.show()
    }
}

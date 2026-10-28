import AppKit
import InkerCore
import InkerKit
import SwiftUI

/// The first-drop "Move or copy?" dialog (AppDropDialog), 440x280. A SwiftUI
/// view in a floating panel, not an NSAlert.
@MainActor
final class DropDialogWindowController {
    private var panel: InkPanel?
    private var answered = false

    init(batch: DropBatch, clock: RenderClock, completion: @escaping (IngestMode?, Bool) -> Void) {
        var answer: (IngestMode?, Bool) -> Void = { _, _ in }
        let view = DropDialogView(batch: batch) { mode, always in answer(mode, always) }
            .environmentObject(clock)
        let panel = InkPanel(size: NSSize(width: 440, height: 280), clock: clock, floating: true, content: AnyView(view))
        self.panel = panel
        answer = { [weak self, weak panel] mode, always in
            guard let self, !self.answered else { return }
            self.answered = true
            completion(mode, always)
            panel?.close()
        }
        // Closing the window without choosing cancels the drop.
        panel.onClose = { [weak self] in
            guard let self, !self.answered else { return }
            self.answered = true
            completion(nil, false)
        }
    }

    func show() {
        panel?.show()
    }

    func close() {
        panel?.close()
    }
}

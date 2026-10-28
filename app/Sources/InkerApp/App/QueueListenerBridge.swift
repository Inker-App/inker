import Foundation
import InkerCore

/// The Swift side of the UniFFI `QueueListener` callback. Rust calls it from a
/// background thread; `onChange` must hop to the main actor itself.
final class QueueListenerBridge: QueueListener, @unchecked Sendable {
    private let onChange: @Sendable () -> Void

    init(onChange: @escaping @Sendable () -> Void) {
        self.onChange = onChange
    }

    func queueChanged() {
        onChange()
    }
}

import InkerKit
import SwiftUI

/// "Copy" that turns into a filled "Copied" for 1.5 s.
struct CopyButton: View {
    enum Size {
        /// Folder header: 28 tall, 12 padding.
        case large
        /// Rows: 26 tall, 10 padding.
        case small
    }

    let id: String
    let handle: String
    let enabled: Bool
    var size: Size = .small
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        let copied = store.copyFeedback.isShowingCopied(id, now: Date())
        let height: CGFloat = size == .large ? 28 : 26
        let padding: CGFloat = size == .large ? 12 : 10
        Button(copied ? "Copied" : "Copy") {
            store.copy(handle: handle, id: id)
        }
        .buttonStyle(InkButtonStyle(kind: copied ? .primary : .outline(opacity: 0.3), height: height, horizontalPadding: padding))
        .disabled(!enabled)
        .accessibilityLabel(copied ? "Copied \(handle)" : "Copy \(handle)")
    }
}

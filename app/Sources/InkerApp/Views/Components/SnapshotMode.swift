import SwiftUI

/// True while `INKER --snapshots` renders screens with ImageRenderer, which
/// only draws views SwiftUI renders itself. AppKit-backed views (scroll
/// views, text fields, the checkbox, the Metal eyes and octopus) show a
/// placeholder there, so under this flag they swap for plain SwiftUI
/// stand-ins of the same size.
private struct SnapshotModeKey: EnvironmentKey {
    static let defaultValue = false
}

extension EnvironmentValues {
    var snapshotMode: Bool {
        get { self[SnapshotModeKey.self] }
        set { self[SnapshotModeKey.self] = newValue }
    }
}

/// A vertical scroll view without indicators. In snapshot mode the content
/// is drawn from the top and clipped, as the scroll view shows it at rest.
struct PopoverScroll<Content: View>: View {
    @Environment(\.snapshotMode) private var snapshotMode
    let content: Content

    init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    var body: some View {
        if snapshotMode {
            content
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                .clipped()
        } else {
            ScrollView(.vertical, showsIndicators: false) {
                content
            }
        }
    }
}

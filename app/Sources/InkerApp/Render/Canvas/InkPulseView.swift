import InkerKit
import SwiftUI

/// The ripple of dots behind a located row: plays once from `startedAt`
/// over 1.5 s, then draws nothing.
struct InkPulseView: View {
    let startedAt: Date
    @Environment(\.snapshotMode) private var snapshotMode

    var body: some View {
        TimelineView(.animation(minimumInterval: 1.0 / 30, paused: snapshotMode)) { timeline in
            Canvas { context, size in
                let t = snapshotMode ? 0.25 : timeline.date.timeIntervalSince(startedAt)
                let field = InkPulse.field(size: size, time: t)
                context.fill(InkPaths.path(InkGeometry.shapes(for: field, diagonals: false)),
                             with: .color(Palette.paper.opacity(0.35)))
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

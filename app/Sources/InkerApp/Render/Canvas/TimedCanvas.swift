import SwiftUI

/// A Canvas redrawn by the shared RenderClock: up to 30 fps while running,
/// frozen otherwise (at the still frame under Reduce Motion).
struct TimedCanvas: View {
    var fps: Double = 30
    let draw: (inout GraphicsContext, CGSize, Double) -> Void
    @EnvironmentObject private var clock: RenderClock

    var body: some View {
        // VERIFY: `.animation(minimumInterval:paused:)` is the macOS 12+
        // TimelineView schedule; check CPU in Instruments (stack-decision 3.7).
        TimelineView(.animation(minimumInterval: 1.0 / fps, paused: !clock.isRunning)) { timeline in
            Canvas { context, size in
                draw(&context, size, clock.time(at: timeline.date))
            }
        }
    }
}

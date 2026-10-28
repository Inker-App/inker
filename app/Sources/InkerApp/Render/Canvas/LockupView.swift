import InkerKit
import SwiftUI

/// The animated header lockup: ink-grid mark plus INKER, 100x20.
struct LockupView: View {
    var body: some View {
        TimedCanvas { context, _, t in
            var lockup = Lockup()
            lockup.update(time: t)
            context.fill(InkPaths.path(lockup.mark), with: .color(Palette.paper))
            context.fill(InkPaths.path(lockup.word, diagonals: true), with: .color(Palette.paper))
        }
        .frame(width: 100, height: 20)
        .accessibilityElement()
        .accessibilityLabel("Inker")
    }
}

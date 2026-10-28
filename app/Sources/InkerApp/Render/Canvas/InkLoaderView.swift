import InkerKit
import SwiftUI

/// The 284x8 ink-grid loaders on the Processing board.
struct InkLoaderView: View {
    let mode: InkLoaderMode

    var body: some View {
        TimedCanvas { context, _, t in
            let field = InkLoader.field(mode, width: Double(InkLoader.size.width), time: t)
            context.fill(InkPaths.path(field), with: .color(Palette.paper))
        }
        .frame(width: InkLoader.size.width, height: InkLoader.size.height)
        .accessibilityHidden(true)
    }
}

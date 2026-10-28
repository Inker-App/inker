import InkerKit
import SwiftUI

/// AppPopEmpty: the 328x384 drop zone.
struct EmptyBoard: View {
    @EnvironmentObject private var store: CoreStore
    @State private var pointer = PointerBox()

    var body: some View {
        ZStack(alignment: .top) {
            ReactionDiffusionView(pointer: pointer, highlighted: store.isDropTargeted)
            EyeLayer(makeScene: { DriftingEyesScene.emptyZone() }, pointerBox: pointer)
            VStack(spacing: 6) {
                Text("Drop files or folders")
                    .font(Typography.title)
                    .foregroundColor(Palette.paper)
                (Text(store.model.emptySubtitlePrefix)
                    + Text(store.model.handlePrefix).font(Typography.mono(12)).foregroundColor(Palette.paper)
                    + Text(" handle to paste."))
                    .font(Typography.small)
                    .foregroundColor(Palette.muted)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 220)
            }
            .padding(.top, 160)
            .allowsHitTesting(false)
        }
        .frame(width: 328, height: 384)
        // 16 all round would need 416 pt; the board's content area is 414.
        .padding(.horizontal, 16)
        .padding(.top, 16)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Drop zone. Drop files or folders.")
    }
}

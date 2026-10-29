import InkerCore
import InkerKit
import SwiftUI

/// "Move these 4 files out of ~/Downloads?" as a card inside the popover,
/// replacing the AppDropDialog window (it opened behind the popover).
struct DropDecisionCard: View {
    let batch: DropBatch
    let onAnswer: (IngestMode?, Bool) -> Void
    @State private var alwaysMove = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            // The 392-wide dialog scene, scaled to the popover's inner width.
            DialogScene(fileCount: batch.count, source: batch.sourceDisplay ?? "~/Downloads")
                .scaleEffect(328.0 / 392.0, anchor: .topLeading)
                .frame(width: 328, height: 76 * 328.0 / 392.0, alignment: .topLeading)
            VStack(alignment: .leading, spacing: 4) {
                Text(DropDialogCopy.title(for: batch)).font(Typography.title)
                Text(DropDialogCopy.body).foregroundColor(Palette.muted).fixedSize(horizontal: false, vertical: true)
            }
            InkCheckbox(isOn: $alwaysMove, label: Text(DropDialogCopy.alwaysMove), accessibilityLabel: DropDialogCopy.alwaysMove)
                .font(Typography.small)
                .foregroundColor(Palette.checkLabel)
            HStack(spacing: 8) {
                Button(DropDialogCopy.cancel) { onAnswer(nil, false) }
                    .buttonStyle(.plain)
                    .font(Typography.body)
                    .foregroundColor(Palette.muted)
                    .keyboardShortcut(.cancelAction)
                Spacer()
                Button(DropDialogCopy.copyInstead) { onAnswer(.copy, false) }
                    .buttonStyle(InkButtonStyle(kind: .outline(opacity: 0.3), height: 32, horizontalPadding: 14, fontSize: 13))
                Button(DropDialogCopy.move) { onAnswer(.move, alwaysMove) }
                    .buttonStyle(InkButtonStyle(kind: .primary, height: 32, horizontalPadding: 18, fontSize: 13))
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding(16)
        .frame(width: 360, alignment: .topLeading)
    }
}

/// Eyeballs peering at the dropped files sitting in Downloads (392x76).
struct DialogScene: View {
    let fileCount: Int
    let source: String

    var body: some View {
        ZStack {
            TimedCanvas(fps: 1) { context, _, _ in
                let fx = 196.0, fy = 36.0
                var files = Path()
                for k in 0..<min(max(fileCount, 1), 4) {
                    let rect = CGRect(x: fx - 34 + Double(k) * 14, y: fy - 18 + Double(k) * 3, width: 26, height: 32)
                    let n = Int((Double(rect.width + rect.height) * 2 / 5).rounded(.down))
                    files.addPath(InkPaths.dottedRect(rect, count: n, radius: 1.1))
                }
                context.fill(files, with: .color(Palette.muted))
                context.draw(Text(source).font(Typography.mono(10, weight: 500)).foregroundColor(Palette.muted),
                             at: CGPoint(x: fx, y: fy + 34 + 3), anchor: .bottom)
            }
            EyeLayer(makeScene: { StaringEyesScene.dropDialog() })
        }
        .frame(width: 392, height: 76)
        .background(RoundedRectangle(cornerRadius: 8).fill(Palette.panel))
        .accessibilityElement()
        .accessibilityLabel("Eyeballs peering at files sitting in \(source)")
    }
}

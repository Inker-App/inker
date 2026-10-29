import AppKit
import InkerCore
import InkerKit
import SwiftUI

/// AppOnboard1: pick a folder the chat can't see. (The coding-agent rows
/// moved to Settings; this screen is for chat users.)
struct OnboardStep1: View {
    @EnvironmentObject private var store: CoreStore
    @State private var folderText = ""
    @State private var warnings: [FolderWarning] = []
    @Environment(\.snapshotMode) private var snapshotMode
    private let home = FileManager.default.homeDirectoryForCurrentUser

    var body: some View {
        OnboardPage(nav: OnboardNav(primary: "Continue", onPrimary: finish)) {
            ScaledScene(size: DownloadsScene.size) { DownloadsScene() }
            // Copy is a placeholder for Ben.
            OnboardHeading(step: 1, title: "Pick a folder your chat can't see",
                           text: "Anything you drag into a chat window gets uploaded. Files you drop on INKER move into a private folder on this Mac instead.")
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    folderField
                        .font(Typography.mono(13))
                        .padding(.horizontal, 12)
                        .frame(height: 36)
                        .background(RoundedRectangle(cornerRadius: 6).fill(Palette.panel))
                        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Palette.field, lineWidth: 1))
                        .accessibilityLabel("Private folder")
                    Button("Choose…", action: choose)
                        .buttonStyle(InkButtonStyle(kind: .outline(opacity: 0.2), height: 36, horizontalPadding: 14, fontSize: 13))
                }
                ForEach(warnings, id: \.message) { w in
                    // Not on the board; copy and placement are Ben's call.
                    Text(w.message).font(Typography.small).foregroundColor(Palette.paper)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .onAppear {
            folderText = PathDisplay.abbreviate(store.privateFolder, home: home)
            check()
        }
    }

    /// ImageRenderer cannot draw a TextField; snapshots show the folder as text.
    @ViewBuilder
    private var folderField: some View {
        if snapshotMode {
            Text(PathDisplay.abbreviate(store.privateFolder, home: home))
                .lineLimit(1)
                .truncationMode(.middle)
                .frame(maxWidth: .infinity, alignment: .leading)
        } else {
            TextField("Private folder", text: $folderText, onCommit: { check() })
                .textFieldStyle(.plain)
        }
    }

    private var folderURL: URL {
        PathDisplay.expand(folderText.trimmingCharacters(in: .whitespaces), home: home)
    }

    private func check() {
        warnings = FolderPickerWarnings.warnings(for: folderURL, home: home, containsGitDirectory: FolderPickerWarnings.gitCheck)
    }

    private func choose() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.prompt = "Choose"
        let folder = folderURL
        // Default to ~/Private (or its parent if it does not exist yet).
        panel.directoryURL = FileManager.default.fileExists(atPath: folder.path) ? folder : folder.deletingLastPathComponent()
        // The panel is a system window; pin the popover so it is still
        // there, on this step, when the panel closes.
        let response = store.withPopoverPinned { panel.runModal() }
        if response == .OK, let url = panel.url {
            folderText = PathDisplay.abbreviate(url, home: home)
            check()
        }
    }

    private func finish() {
        check()
        let url = folderURL
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        store.setPrivateFolder(url)
        store.nextStep()
    }
}

/// Eyes crowding round a dotted ~/Downloads folder (480x120; ScaledScene
/// fits it to the popover).
struct DownloadsScene: View {
    static let size = CGSize(width: 480, height: 120)

    var body: some View {
        ZStack {
            TimedCanvas(fps: 1) { context, _, _ in
                let fx = 240.0, fy = 62.0
                var dots = Path()
                for k in 0..<64 {
                    let u = Double(k) / 64
                    var px: Double, py: Double
                    if u < 0.25 {
                        px = fx - 34 + u / 0.25 * 68; py = fy - 20
                    } else if u < 0.5 {
                        px = fx + 34; py = fy - 20 + (u - 0.25) / 0.25 * 44
                    } else if u < 0.75 {
                        px = fx + 34 - (u - 0.5) / 0.25 * 68; py = fy + 24
                    } else {
                        px = fx - 34; py = fy + 24 - (u - 0.75) / 0.25 * 44
                    }
                    dots.addEllipse(in: CGRect(x: px - 1.3, y: py - 1.3, width: 2.6, height: 2.6))
                }
                for q in 0..<8 {
                    let px = fx - 34 + Double(q) * 3.4
                    dots.addEllipse(in: CGRect(x: px - 1.3, y: fy - 24 - 1.3, width: 2.6, height: 2.6))
                }
                context.fill(dots, with: .color(Palette.muted))
                context.draw(Text("~/Downloads").font(Typography.mono(10, weight: 500)).foregroundColor(Palette.muted),
                             at: CGPoint(x: fx, y: fy + 40 + 3), anchor: .bottom)
            }
            EyeLayer(makeScene: { StaringEyesScene.onboarding() })
        }
        .frame(width: DownloadsScene.size.width, height: DownloadsScene.size.height)
        .background(RoundedRectangle(cornerRadius: 8).fill(Palette.panel))
        .accessibilityElement()
        .accessibilityLabel("Halftone eyeballs crowding around a Downloads folder, staring at it")
    }
}

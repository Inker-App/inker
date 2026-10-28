import AppKit
import InkerCore
import InkerKit
import SwiftUI

/// AppOnboard1: pick a folder they can't see; keep these agents out.
struct OnboardStep1: View {
    let onContinue: () -> Void
    @EnvironmentObject private var store: CoreStore
    @State private var folderText = ""
    @State private var warnings: [FolderWarning] = []
    @State private var enabled: Set<String> = Set(AgentRows.keepOut.map(\.id))
    private let home = FileManager.default.homeDirectoryForCurrentUser

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            DownloadsScene()
            OnboardHeading(step: 1, title: "Pick a folder they can't see",
                           text: "Agents can read anything in their workspace, Downloads included. Your private folder lives outside it.")
            VStack(alignment: .leading, spacing: 8) {
                HStack(spacing: 8) {
                    TextField("Private folder", text: $folderText, onCommit: { check() })
                        .textFieldStyle(.plain)
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
                }
            }
            VStack(alignment: .leading, spacing: 8) {
                LabelText("Keep these agents out")
                VStack(spacing: 0) {
                    Hairline()
                    ForEach(AgentRows.keepOut) { row in
                        HStack {
                            (Text(row.title) + Text(" · \(row.detail)").font(Typography.caption).foregroundColor(Palette.muted))
                            Spacer()
                            Toggle("", isOn: binding(for: row.id))
                                .toggleStyle(.checkbox)
                                .labelsHidden()
                                .accessibilityLabel(row.accessibilityLabel)
                        }
                        .frame(height: 40)
                        Hairline()
                    }
                }
                Text(AgentRows.footnote).font(Typography.small).foregroundColor(Palette.muted)
            }
            Spacer(minLength: 0)
            OnboardNav(primary: "Continue", onPrimary: finish)
        }
        .onAppear {
            folderText = PathDisplay.abbreviate(store.privateFolder, home: home)
            check()
        }
    }

    private func binding(for id: String) -> Binding<Bool> {
        Binding(get: { enabled.contains(id) },
                set: { on in if on { enabled.insert(id) } else { enabled.remove(id) } })
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
        if panel.runModal() == .OK, let url = panel.url {
            folderText = PathDisplay.abbreviate(url, home: home)
            check()
        }
    }

    private func finish() {
        check()
        let url = folderURL
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        store.setPrivateFolder(url)
        // Dry runs only until the writers in inker-agents are wired.
        let agents = AgentRows.keepOut.filter { enabled.contains($0.id) }.map(\.agent)
        _ = store.planAgents(agents)
        onContinue()
    }
}

/// Eyes crowding round a dotted ~/Downloads folder (480x120).
struct DownloadsScene: View {
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
        .frame(width: 480, height: 120)
        .background(RoundedRectangle(cornerRadius: 8).fill(Palette.panel))
        .accessibilityElement()
        .accessibilityLabel("Halftone eyeballs crowding around a Downloads folder, staring at it")
    }
}

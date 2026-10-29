import AppKit
import InkerCore
import SwiftUI

/// AppOnboard2: agents ask the octopus.
struct OnboardStep2: View {
    let onBack: () -> Void
    let onContinue: () -> Void
    @EnvironmentObject private var store: CoreStore
    @State private var teach = true
    @State private var planned: Set<String> = []

    var body: some View {
        VStack(alignment: .leading, spacing: 20) {
            OctopusLayer()
                .frame(width: 480, height: 120)
                .background(RoundedRectangle(cornerRadius: 8).fill(Palette.panel))
                .accessibilityLabel("The halftone octopus, swaying, its eyes following the cursor")
            OnboardHeading(step: 2, title: "Agents ask the octopus",
                           text: "They can't open the folder. They ask Inker, and get a copy with the sensitive parts inked out.")
            VStack(spacing: 0) {
                Hairline()
                connectRow("Claude Code", agent: .claudeCode)
                Hairline()
                connectRow("Cursor", agent: .cursor)
                Hairline()
                HStack {
                    Text("Something else")
                    Spacer()
                    Button("Copy config", action: copyConfig)
                        .buttonStyle(InkButtonStyle(kind: .link, height: 28, horizontalPadding: 12))
                }
                .frame(height: 40)
                Hairline()
            }
            // Decision 5.4: the line goes in CLAUDE.md and AGENTS.md (not .cursorrules).
            InkCheckbox(isOn: $teach,
                        label: Text("Teach agents what ") + Text("🐙/").font(Typography.mono(12)).foregroundColor(Palette.paper)
                            + Text(" means, with one line in CLAUDE.md and AGENTS.md"),
                        accessibilityLabel: "Teach agents what the octopus handle means")
                .font(Typography.small)
                .foregroundColor(Palette.checkLabel)
            Spacer(minLength: 0)
            OnboardNav(onBack: onBack, primary: "Continue", onPrimary: onContinue)
        }
    }

    /// The board shows "Connected" for an agent already set up. The config
    /// writers are stubs, so "Connect" only runs a dry run and says so.
    private func connectRow(_ title: String, agent: AgentKind) -> some View {
        HStack {
            Text(title)
            Spacer()
            if planned.contains(title) {
                // Placeholder until the writers exist; Ben to confirm the copy.
                Text("Not wired yet").font(Typography.small).foregroundColor(Palette.muted)
            } else {
                Button("Connect") {
                    _ = store.planAgents([agent])
                    planned.insert(title)
                }
                .buttonStyle(InkButtonStyle(kind: .outline(opacity: 0.3), height: 28, horizontalPadding: 12))
            }
        }
        .frame(height: 40)
    }

    /// A generic stdio MCP entry for any client.
    private func copyConfig() {
        let json = """
        {
          "mcpServers": {
            "inker": { "command": "/Applications/INKER.app/Contents/MacOS/inker-cli", "args": ["mcp"] }
          }
        }
        """
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(json, forType: .string)
    }
}

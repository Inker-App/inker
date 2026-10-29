import AppKit
import InkerCore
import InkerKit
import os
import SwiftUI

/// AppOnboard2: your chat asks INKER. Chat apps only; coding agents are in
/// Settings. Copy is a placeholder for Ben.
struct OnboardStep2: View {
    @EnvironmentObject private var store: CoreStore
    @State private var results: [String: String] = [:]
    @State private var copied = false
    private let log = Logger(subsystem: "sh.inker.app", category: "onboarding")

    var body: some View {
        OnboardPage(nav: OnboardNav(onBack: { store.previousStep() }, primary: "Continue",
                                    onPrimary: { store.nextStep() })) {
            ScaledScene(size: CGSize(width: 480, height: 120)) {
                OctopusLayer()
                    .background(RoundedRectangle(cornerRadius: 8).fill(Palette.panel))
            }
            .accessibilityLabel("The halftone octopus, swaying, its eyes following the cursor")
            OnboardHeading(step: 2, title: "Your chat asks INKER",
                           text: "Your chat can't open the folder. It asks INKER and gets a copy with the sensitive parts inked out.")
            VStack(spacing: 0) {
                Hairline()
                ForEach(ChatRows.connect) { row in
                    chatRow(row)
                    Hairline()
                }
            }
        }
    }

    /// Title over detail: "title · detail" on one line does not fit 328 points.
    private func chatRow(_ row: ChatRow) -> some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(row.title)
                Text(row.detail).font(Typography.caption).foregroundColor(Palette.muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 8)
            trailing(for: row)
        }
        .padding(.vertical, 6)
        .frame(minHeight: 40)
    }

    @ViewBuilder
    private func trailing(for row: ChatRow) -> some View {
        switch row.action {
        case .connect(let agent):
            if let result = results[row.id] {
                Text(result).font(Typography.small).foregroundColor(Palette.muted)
            } else {
                Button("Connect") { connect(agent, row: row) }
                    .buttonStyle(InkButtonStyle(kind: .outline(opacity: 0.3), height: 28, horizontalPadding: 12))
            }
        case .comingSoon:
            Text(ChatRows.comingSoonLabel).font(Typography.small).foregroundColor(Palette.muted)
        case .copyConfig:
            Button(copied ? "Copied" : "Copy config", action: copyConfig)
                .buttonStyle(InkButtonStyle(kind: .link, height: 28, horizontalPadding: 12))
        }
    }

    private func connect(_ agent: AgentKind, row: ChatRow) {
        do {
            _ = try store.setupAgent(agent)
            results[row.id] = ChatRows.connectedLabel(for: agent)
        } catch {
            results[row.id] = ChatRows.failedLabel
            log.error("connect failed: \(String(describing: error), privacy: .public)")
        }
    }

    /// A generic stdio MCP entry for any client.
    private func copyConfig() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(AgentConfigSnippet.mcpServers(binary: store.cliPath), forType: .string)
        copied = true
    }
}

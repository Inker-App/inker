import InkerCore
import InkerKit
import os
import SwiftUI

/// Settings: handle style, and "Also use a coding agent?" (moved here from
/// onboarding step 1, which is for chat users). Placeholder layout for Ben.
struct SettingsView: View {
    @EnvironmentObject private var store: CoreStore
    @State private var asciiHandles = false
    @State private var results: [String: String] = [:]
    private let log = Logger(subsystem: "sh.inker.app", category: "settings")

    var body: some View {
        VStack(alignment: .leading, spacing: 24) {
            VStack(alignment: .leading, spacing: 10) {
                LabelText("Handles")
                InkCheckbox(isOn: $asciiHandles,
                            label: Text("Copy handles as ") + Text("ink/").font(Typography.mono(12)).foregroundColor(Palette.paper)
                                + Text(" instead of ") + Text("🐙/").font(Typography.mono(12)).foregroundColor(Palette.paper),
                            accessibilityLabel: "Copy handles as ink slash instead of the octopus emoji")
                    .font(Typography.small)
                    .foregroundColor(Palette.checkLabel)
            }
            VStack(alignment: .leading, spacing: 8) {
                LabelText(CodingAgentRows.sectionTitle)
                VStack(spacing: 0) {
                    Hairline()
                    ForEach(CodingAgentRows.keepOut) { row in
                        HStack {
                            (Text(row.title) + Text(" · \(row.detail)").font(Typography.caption).foregroundColor(Palette.muted))
                            Spacer()
                            if let result = results[row.id] {
                                Text(result).font(Typography.small).foregroundColor(Palette.muted)
                            } else {
                                Button("Keep out") { keepOut(row) }
                                    .buttonStyle(InkButtonStyle(kind: .outline(opacity: 0.3), height: 28, horizontalPadding: 12))
                                    .accessibilityLabel(row.accessibilityLabel)
                            }
                        }
                        .frame(height: 40)
                        Hairline()
                    }
                }
                Text(CodingAgentRows.footnote).font(Typography.small).foregroundColor(Palette.muted)
            }
            Spacer(minLength: 0)
        }
        .padding(.top, 32)
        .padding(.horizontal, 40)
        .padding(.bottom, 28)
        .frame(width: 560, height: 400, alignment: .top)
        .background(Palette.background)
        .foregroundColor(Palette.paper)
        .font(Typography.body)
        .onAppear { asciiHandles = store.settings.handleStyle == .ascii }
        .onChange(of: asciiHandles) { _, on in store.setHandleStyle(on ? .ascii : .octopus) }
    }

    private func keepOut(_ row: CodingAgentRow) {
        do {
            let plan = try store.setupAgent(row.agent)
            results[row.id] = CodingAgentRows.doneLabel(changed: !plan.steps.isEmpty)
        } catch {
            results[row.id] = ChatRows.failedLabel
            log.error("agent setup failed: \(String(describing: error), privacy: .public)")
        }
    }
}

import AppKit
import InkerCore
import InkerKit
import os
import SwiftUI

/// Settings, a screen of the popover: handle style, "Also use a coding
/// agent?" (moved here from onboarding step 1, which is for chat users), and
/// running setup again. Done goes back to the board; Quit INKER lives here
/// now that the gear opens this screen instead of a menu. Placeholder layout
/// for Ben: there is no settings board yet.
struct SettingsView: View {
    @EnvironmentObject private var store: CoreStore
    @State private var asciiHandles = false
    @State private var showsPayer = true
    @State private var clipboardChip = true
    @State private var results: [String: String] = [:]
    private let log = Logger(subsystem: "sh.inker.app", category: "settings")

    var body: some View {
        VStack(spacing: 0) {
            PopoverScroll {
                VStack(alignment: .leading, spacing: 20) {
                    handles
                    privateFolder
                    codingAgents
                    setup
                }
                .padding(.horizontal, PopoverLayout.gutter)
                .padding(.top, 14)
                .padding(.bottom, 8)
                .frame(width: PopoverLayout.width, alignment: .topLeading)
            }
            .frame(maxHeight: .infinity)
            HStack {
                Button("Quit INKER") { NSApp.terminate(nil) }
                    .buttonStyle(.plain)
                    .font(Typography.small)
                    .foregroundColor(Palette.muted)
                    .frame(height: 36)
                    .keyboardShortcut("q")
                Spacer()
                Button("Done") { store.closeSettings() }
                    .buttonStyle(InkButtonStyle(kind: .primary, height: 36, horizontalPadding: 18, fontSize: 13))
                    .keyboardShortcut(.defaultAction)
            }
            .frame(height: 36)
            .padding(.horizontal, PopoverLayout.gutter)
            .padding(.top, 8)
            .padding(.bottom, 12)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .onAppear {
            asciiHandles = store.settings.handleStyle == .ascii
            showsPayer = store.settings.showsPayer
            clipboardChip = store.settings.offersClipboardHandle
        }
        .onChange(of: asciiHandles) { _, on in store.setHandleStyle(on ? .ascii : .octopus) }
        .onChange(of: showsPayer) { _, on in store.setShowsPayer(on) }
        .onChange(of: clipboardChip) { _, on in store.setOffersClipboardHandle(on) }
    }

    private var handles: some View {
        VStack(alignment: .leading, spacing: 10) {
            LabelText("Handles")
            InkCheckbox(isOn: $asciiHandles,
                        label: Text("Copy handles as ") + Text("ink/").font(Typography.mono(12)).foregroundColor(Palette.paper)
                            + Text(" instead of ") + Text("🐙/").font(Typography.mono(12)).foregroundColor(Palette.paper),
                        accessibilityLabel: "Copy handles as ink slash instead of the octopus emoji")
                .font(Typography.small)
                .foregroundColor(Palette.checkLabel)
            InkCheckbox(isOn: $showsPayer, label: Text(AppSettings.showPayerLabel),
                        accessibilityLabel: AppSettings.showPayerLabel)
                .font(Typography.small)
                .foregroundColor(Palette.checkLabel)
            InkCheckbox(isOn: $clipboardChip, label: Text(AppSettings.clipboardChipLabel),
                        accessibilityLabel: AppSettings.clipboardChipLabel)
                .font(Typography.small)
                .foregroundColor(Palette.checkLabel)
        }
    }

    /// Where the files went, and a way there. Placement is a placeholder for Ben.
    private var privateFolder: some View {
        VStack(alignment: .leading, spacing: 8) {
            LabelText(PrivateFolderSection.title)
            HStack(spacing: 8) {
                Text(store.privateFolderDisplay)
                    .font(Typography.mono(12))
                    .foregroundColor(Palette.paper)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Spacer(minLength: 8)
                Button(PrivateFolderSection.openButton) { store.openPrivateFolder() }
                    .buttonStyle(InkButtonStyle(kind: .outline(opacity: 0.3), height: 28, horizontalPadding: 12))
                    .accessibilityLabel("Open the private folder in Finder")
            }
        }
    }

    /// Title over detail, as on onboarding step 2: one line does not fit 328 points.
    private var codingAgents: some View {
        VStack(alignment: .leading, spacing: 8) {
            LabelText(CodingAgentRows.sectionTitle)
            VStack(spacing: 0) {
                Hairline()
                ForEach(CodingAgentRows.keepOut) { row in
                    HStack(spacing: 8) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(row.title)
                            Text(row.detail).font(Typography.caption).foregroundColor(Palette.muted)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        Spacer(minLength: 8)
                        if let result = results[row.id] {
                            Text(result).font(Typography.small).foregroundColor(Palette.muted)
                        } else {
                            Button("Keep out") { keepOut(row) }
                                .buttonStyle(InkButtonStyle(kind: .outline(opacity: 0.3), height: 28, horizontalPadding: 12))
                                .accessibilityLabel(row.accessibilityLabel)
                        }
                    }
                    .padding(.vertical, 6)
                    .frame(minHeight: 40)
                    Hairline()
                }
            }
            Text(CodingAgentRows.footnote).font(Typography.small).foregroundColor(Palette.muted)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    /// Back to onboarding step 1, inside the popover. Copy is a placeholder for Ben.
    private var setup: some View {
        VStack(alignment: .leading, spacing: 8) {
            LabelText("Setup")
            Button("Run setup again") { store.runSetupAgain() }
                .buttonStyle(InkButtonStyle(kind: .outline(opacity: 0.3), height: 28, horizontalPadding: 12))
        }
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

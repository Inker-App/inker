import Foundation
import ImageIO
import InkerCore
import InkerKit
import SwiftUI
import UniformTypeIdentifiers

/// `INKER --snapshots <dir>`: renders every popover screen with SwiftUI's
/// ImageRenderer at 2x into `<dir>/<name>.png` and exits, without starting
/// NSApplication or opening a window. The release workflow runs it so each
/// build carries pictures of what the popover looks like.
///
/// The stores are frozen on the Rust PreviewBackend (as `INKER_PREVIEW`
/// does) and the screen is set on the store directly. `snapshotMode` swaps
/// the AppKit-backed views ImageRenderer cannot draw (scroll views, the
/// folder field, the checkbox) for SwiftUI stand-ins, and leaves the Metal
/// eyes and octopus blank. Canvas scenes draw the frame at
/// `RenderClock.stillTime`.
@MainActor
enum Snapshots {
    struct Shot {
        let name: String
        let scenario: PreviewScenario
        let screen: PopoverScreen
        var dropCard = false
        /// Typed into Find before the shot.
        var query: String?
        /// Located (expanded, pulsing) before the shot.
        var locate: String?
        /// The row menu open on the row this handle names.
        var menu: String?
        /// Forget clicked once, so the menu asks "Are you sure?".
        var confirmForget = false
    }

    static let shots: [Shot] = [
        Shot(name: "popover-empty", scenario: .empty, screen: .board),
        Shot(name: "popover-processing", scenario: .processing, screen: .board),
        Shot(name: "popover-attention", scenario: .attention, screen: .board),
        Shot(name: "popover-ready", scenario: .ready, screen: .board),
        Shot(name: "popover-scale", scenario: .scale, screen: .board),
        Shot(name: "popover-scale-search", scenario: .scale, screen: .board, query: "acme"),
        Shot(name: "popover-scale-handle", scenario: .scale, screen: .board, locate: "🐙/w2-2025-2"),
        Shot(name: "popover-scale-menu", scenario: .scale, screen: .board, menu: "🐙/w2-2025-2"),
        Shot(name: "popover-scale-menu-forget", scenario: .scale, screen: .board, menu: "🐙/w2-2025-2", confirmForget: true),
        Shot(name: "onboarding-1", scenario: .empty, screen: .onboarding(step: 1)),
        Shot(name: "onboarding-2", scenario: .empty, screen: .onboarding(step: 2)),
        Shot(name: "onboarding-3", scenario: .empty, screen: .onboarding(step: 3)),
        Shot(name: "settings", scenario: .ready, screen: .settings),
        Shot(name: "drop-card", scenario: .ready, screen: .board, dropCard: true),
    ]

    /// Four PDFs dropped from ~/Downloads, for the drop card.
    static func sampleBatch() -> DropBatch {
        DropCollector.collect(
            (1...4).map { URL(fileURLWithPath: "/Users/preview/Downloads/file-\($0).pdf") },
            home: URL(fileURLWithPath: "/Users/preview"),
            privateFolder: URL(fileURLWithPath: "/Users/preview/Private"),
            exists: { _ in true })
    }

    /// Returns the process exit code: 0 when every PNG was written.
    static func run(directory: URL) -> Int32 {
        FontRegistry.registerBundledFonts()
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        } catch {
            fail("cannot create \(directory.path): \(error)")
            return 1
        }
        // Eyes open, mid-breath; the onboarding loops part-way through.
        let clock = RenderClock(alwaysRunning: false, frozenAt: RenderClock.stillTime)
        var failures = 0
        for shot in shots {
            let store = CoreStore.preview(shot.scenario, screen: shot.screen)
            if shot.dropCard { store.presentDropCard(sampleBatch()) }
            if let query = shot.query { store.setQuery(query) }
            if let handle = shot.locate { store.locate(handle: handle) }
            if let handle = shot.menu {
                store.openMenu(handle: handle)
                if shot.confirmForget, let id = store.openMenuID { store.forget(id: id) }
            }
            let view = PopoverRootView()
                .environmentObject(store)
                .environmentObject(clock)
                .environment(\.snapshotMode, true)
                .environment(\.colorScheme, .dark)
            let url = directory.appendingPathComponent("\(shot.name).png")
            if write(view, to: url) {
                print("wrote \(url.path)")
            } else {
                fail("could not render \(shot.name)")
                failures += 1
            }
        }
        return failures == 0 ? 0 : 1
    }

    private static func write<V: View>(_ view: V, to url: URL) -> Bool {
        let renderer = ImageRenderer(content: view)
        renderer.scale = 2
        guard let image = renderer.cgImage,
              let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil)
        else { return false }
        CGImageDestinationAddImage(destination, image, nil)
        return CGImageDestinationFinalize(destination)
    }

    private static func fail(_ message: String) {
        FileHandle.standardError.write(Data("snapshots: \(message)\n".utf8))
    }
}

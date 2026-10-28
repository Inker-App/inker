import InkerCore
import InkerKit
import SwiftUI

// SwiftUI previews for every board, driven by the Rust PreviewBackend through
// CoreHandle.preview(scenario:). They need the xcframework built
// (scripts/build-rust.sh) like the app does.
//
// VERIFY: `#Preview` is a macro from Xcode's toolchain; it builds under
// `swift build` with Xcode 26 selected (xcode-select). If the command-line
// build complains, wrap this file's body in `#if canImport(DeveloperToolsSupport)`.

#if DEBUG
@MainActor
private func popover(_ scenario: PreviewScenario) -> some View {
    FontRegistry.registerBundledFonts()
    return PopoverRootView()
        .environmentObject(CoreStore.preview(scenario))
        .environmentObject(RenderClock(alwaysRunning: true))
}

#Preview("Popover · Empty") {
    popover(.empty)
}

#Preview("Popover · Processing") {
    popover(.processing)
}

#Preview("Popover · Attention") {
    popover(.attention)
}

#Preview("Popover · Ready") {
    popover(.ready)
}

#Preview("Drop dialog") {
    FontRegistry.registerBundledFonts()
    let batch = DropCollector.collect(
        (1...4).map { URL(fileURLWithPath: "/Users/preview/Downloads/file-\($0).pdf") },
        home: URL(fileURLWithPath: "/Users/preview"),
        privateFolder: URL(fileURLWithPath: "/Users/preview/Private"),
        exists: { _ in true })
    return DropDialogView(batch: batch) { _, _ in }
        .environmentObject(RenderClock(alwaysRunning: true))
}

#Preview("Onboarding") {
    FontRegistry.registerBundledFonts()
    return OnboardingView(onDone: {})
        .environmentObject(CoreStore.preview(.empty))
        .environmentObject(RenderClock(alwaysRunning: true))
}
#endif

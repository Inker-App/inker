import InkerCore
import InkerKit
import SwiftUI

// SwiftUI previews for every popover screen (boards, drop card, onboarding
// steps, settings), driven by the Rust PreviewBackend through
// CoreHandle.preview(scenario:). They need the xcframework built
// (scripts/build-rust.sh) like the app does.
//
// VERIFY: `#Preview` is a macro from Xcode's toolchain; it builds under
// `swift build` with Xcode 26 selected (xcode-select). If the command-line
// build complains, wrap this file's body in `#if canImport(DeveloperToolsSupport)`.

#if DEBUG
@MainActor
private func popover(_ scenario: PreviewScenario, _ screen: PopoverScreen = .board) -> some View {
    FontRegistry.registerBundledFonts()
    return PopoverRootView()
        .environmentObject(CoreStore.preview(scenario, screen: screen))
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

#Preview("Popover · Drop card") {
    FontRegistry.registerBundledFonts()
    let store = CoreStore.preview(.ready)
    store.presentDropCard(Snapshots.sampleBatch())
    return PopoverRootView()
        .environmentObject(store)
        .environmentObject(RenderClock(alwaysRunning: true))
}

#Preview("Onboarding · Step 1") {
    popover(.empty, .onboarding(step: 1))
}

#Preview("Onboarding · Step 2") {
    popover(.empty, .onboarding(step: 2))
}

#Preview("Onboarding · Step 3") {
    popover(.empty, .onboarding(step: 3))
}

#Preview("Settings") {
    popover(.ready, .settings)
}
#endif

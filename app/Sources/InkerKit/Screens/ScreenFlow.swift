import Foundation

/// What fills the popover between its header and footer. Everything INKER
/// shows lives here; there are no other windows.
public enum PopoverScreen: Equatable, Sendable {
    /// One of the four boards (empty, processing, attention, ready), picked by the queue.
    case board
    /// Onboarding, steps 1 to `ScreenFlow.stepCount`.
    case onboarding(step: Int)
    case settings
}

/// What the popover draws right now.
public enum PopoverSurface: Equatable, Sendable {
    case screen(PopoverScreen)
    /// The move-or-copy card, in front of the current screen.
    case dropCard
}

/// Moves between the popover's screens. Onboarding shows at first launch (or
/// when forced); Done marks it complete and lands on the board. The gear opens
/// settings, which go back to the board or start setup again at step 1.
public struct ScreenFlow {
    public static let stepCount = 3

    public private(set) var screen: PopoverScreen
    private var onboarding: OnboardingState

    /// `forceOnboarding` is the `INKER_ONBOARDING=1` override.
    public init(onboarding: OnboardingState, forceOnboarding: Bool = false) {
        self.onboarding = onboarding
        screen = onboarding.needsOnboarding || forceOnboarding ? .onboarding(step: 1) : .board
    }

    /// True when the popover should open by itself at launch.
    public var needsAttentionAtLaunch: Bool {
        if case .onboarding = screen { return true }
        return false
    }

    /// A pending drop puts the card in front; the screen underneath is kept
    /// and comes back once the drop is answered.
    public func visible(dropPending: Bool) -> PopoverSurface {
        dropPending ? .dropCard : .screen(screen)
    }

    // MARK: Onboarding

    public mutating func next() {
        guard case .onboarding(let step) = screen else { return }
        screen = .onboarding(step: min(step + 1, ScreenFlow.stepCount))
    }

    public mutating func back() {
        guard case .onboarding(let step) = screen else { return }
        screen = .onboarding(step: max(step - 1, 1))
    }

    /// Done on the last step.
    public mutating func finishOnboarding() {
        onboarding.markComplete()
        screen = .board
    }

    // MARK: Settings

    /// The gear: open settings, or close them when they are already open.
    public mutating func toggleSettings() {
        if screen == .settings {
            closeSettings()
        } else {
            openSettings()
        }
    }

    public mutating func openSettings() {
        screen = .settings
    }

    public mutating func closeSettings() {
        guard screen == .settings else { return }
        screen = .board
    }

    /// "Run setup again" in settings.
    public mutating func runSetupAgain() {
        screen = .onboarding(step: 1)
    }

    /// Any screen directly (snapshots, previews). Steps are clamped to 1...stepCount.
    public mutating func show(_ target: PopoverScreen) {
        if case .onboarding(let step) = target {
            screen = .onboarding(step: min(max(step, 1), ScreenFlow.stepCount))
        } else {
            screen = target
        }
    }
}

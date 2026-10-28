import Foundation

/// Onboarding shows on first launch only.
public struct OnboardingState {
    public static let key = "sh.inker.onboarding.complete"
    private let store: KeyValueStore

    public init(store: KeyValueStore) {
        self.store = store
    }

    public var needsOnboarding: Bool { !store.bool(forKey: OnboardingState.key) }

    public mutating func markComplete() {
        store.set(true, forKey: OnboardingState.key)
    }
}

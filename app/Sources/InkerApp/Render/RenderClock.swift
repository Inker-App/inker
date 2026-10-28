import AppKit
import Combine
import Foundation

/// One clock for every renderer. Runs only while something animated is on
/// screen (the popover, onboarding, the drop dialog); Reduce Motion freezes
/// everything at a still frame (`stillTime`).
@MainActor
final class RenderClock: ObservableObject {
    static let shared = RenderClock()

    /// True while at least one surface is visible and Reduce Motion is off.
    @Published private(set) var isRunning = false
    @Published private(set) var reduceMotion: Bool

    /// The time every renderer shows under Reduce Motion: eyes open, mid-breath.
    static let stillTime: Double = 1.3
    let epoch = Date()

    private var visibleSurfaces = 0
    private let alwaysRunning: Bool
    private var observers: [NSObjectProtocol] = []

    /// `alwaysRunning` is for SwiftUI previews, which have no popover delegate.
    init(alwaysRunning: Bool = false) {
        self.alwaysRunning = alwaysRunning
        reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        // VERIFY: Apple documents this notification on NSWorkspace's own
        // notification center; observing the default center as well is harmless.
        let name = NSWorkspace.accessibilityDisplayOptionsDidChangeNotification
        for center in [NSWorkspace.shared.notificationCenter, NotificationCenter.default] {
            let token = center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                Task { @MainActor in self?.refreshReduceMotion() }
            }
            observers.append(token)
        }
        update()
    }

    func surfaceAppeared() {
        visibleSurfaces += 1
        update()
    }

    func surfaceDisappeared() {
        visibleSurfaces = max(0, visibleSurfaces - 1)
        update()
    }

    /// Seconds since launch, or the still time under Reduce Motion.
    func time(at date: Date) -> Double {
        reduceMotion ? RenderClock.stillTime : date.timeIntervalSince(epoch)
    }

    var now: Double { time(at: Date()) }

    private func refreshReduceMotion() {
        reduceMotion = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion
        update()
    }

    private func update() {
        let running = (alwaysRunning || visibleSurfaces > 0) && !reduceMotion
        if running != isRunning { isRunning = running }
    }
}

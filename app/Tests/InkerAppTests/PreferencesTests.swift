import Foundation
import InkerCore
import Testing
@testable import InkerKit

final class MemoryStore: KeyValueStore {
    var values: [String: Any] = [:]
    func bool(forKey key: String) -> Bool { values[key] as? Bool ?? false }
    func set(_ value: Bool, forKey key: String) { values[key] = value }
    func string(forKey key: String) -> String? { values[key] as? String }
    func setString(_ value: String?, forKey key: String) { values[key] = value }
}

/// The first-drop dialog is remembered (AppDropDialog), onboarding runs once.
struct PreferencesTests {
    @Test func asksUntilAlwaysMoveIsChecked() {
        let store = MemoryStore()
        var p = DropPreferences(store: store)
        #expect(p.rememberedMode == nil, "first drop asks")
        p.record(.move, always: false)
        #expect(p.rememberedMode == nil, "Move without the checkbox asks again")
        p.record(.copy, always: false)
        #expect(p.rememberedMode == nil)
        p.record(.move, always: true)
        #expect(p.rememberedMode == .move)
        #expect(DropPreferences(store: store).rememberedMode == .move, "persists")
    }

    @Test func alwaysOnlyAppliesToMove() {
        // The board's checkbox reads "Always move, don't ask again".
        var p = DropPreferences(store: MemoryStore())
        p.record(.copy, always: true)
        #expect(p.rememberedMode == nil)
    }

    @Test func onboardingRunsOnce() {
        let store = MemoryStore()
        var o = OnboardingState(store: store)
        #expect(o.needsOnboarding)
        o.markComplete()
        #expect(!OnboardingState(store: store).needsOnboarding)
    }

    @Test func privateFolderAndHandleStyleAreStored() {
        let store = MemoryStore()
        var s = AppSettings(store: store)
        #expect(s.handleStyle == .octopus)
        #expect(s.privateFolderPath == nil)
        s.handleStyle = .ascii
        s.privateFolderPath = "/Users/test/Private"
        let again = AppSettings(store: store)
        #expect(again.handleStyle == .ascii)
        #expect(again.privateFolderPath == "/Users/test/Private")
    }
}

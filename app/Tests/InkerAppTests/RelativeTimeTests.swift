import Foundation
import Testing
@testable import InkerKit

/// The "· 2m" on the last-read line.
struct RelativeTimeTests {
    private let now = Date(timeIntervalSince1970: 10_000_000)

    @Test func shortForms() {
        #expect(RelativeTime.short(from: now.addingTimeInterval(-5), to: now) == "now")
        #expect(RelativeTime.short(from: now.addingTimeInterval(-120), to: now) == "2m")
        #expect(RelativeTime.short(from: now.addingTimeInterval(-3 * 3600 - 59), to: now) == "3h")
        #expect(RelativeTime.short(from: now.addingTimeInterval(-2 * 86400), to: now) == "2d")
        #expect(RelativeTime.short(from: now.addingTimeInterval(30), to: now) == "now", "clock skew")
    }
}

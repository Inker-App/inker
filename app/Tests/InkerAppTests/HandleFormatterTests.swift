import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// Codename to clipboard handle, both styles (decision 5.13).
struct HandleFormatterTests {
    @Test func rendersBothStyles() {
        #expect(HandleFormatter.render(codename: "w2-acme-2025", style: .octopus) == "🐙/w2-acme-2025")
        #expect(HandleFormatter.render(codename: "w2-acme-2025", style: .ascii) == "ink/w2-acme-2025")
    }

    @Test func rejectsWhatTheCoreRejects() {
        for bad in ["", "W2", "a--b", "-a", "a-", "a b", "a_b", String(repeating: "a", count: 65)] {
            #expect(HandleFormatter.render(codename: bad, style: .octopus) == nil, "\(bad)")
        }
        #expect(HandleFormatter.render(codename: String(repeating: "a", count: 64), style: .ascii) != nil)
    }

    @Test func styleRoundTripsThroughItsStoredName() {
        #expect(HandleFormatter.style(named: "ascii") == .ascii)
        #expect(HandleFormatter.style(named: "octopus") == .octopus)
        #expect(HandleFormatter.style(named: nil) == .octopus, "octopus is the default")
        #expect(HandleFormatter.name(of: .ascii) == "ascii")
    }
}

import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// The clipboard chip's parse, the remembered year groups, the expanded
/// row's lines, the two new settings and the ink pulse.
struct FindSupportTests {
    @Test func clipboardParseAcceptsOnlyHandlesWithAPrefix() {
        #expect(ClipboardHandle.parse("🐙/w2-2025-2") == "🐙/w2-2025-2")
        #expect(ClipboardHandle.parse("  ink/w2-2025-2\n") == "ink/w2-2025-2")
        #expect(ClipboardHandle.parse("w2-2025-2") == nil, "a bare codename is not a handle")
        #expect(ClipboardHandle.parse("🐙/Not A Codename") == nil)
        #expect(ClipboardHandle.parse("🐙/") == nil)
        #expect(ClipboardHandle.parse("Please read 🐙/w2-2025-2") == nil, "only a handle on its own")
        #expect(ClipboardHandle.parse(nil) == nil)
        #expect(ClipboardHandle.parse("") == nil)
        #expect(ClipboardHandle.chipText == "Your clipboard holds ")
        #expect(ClipboardHandle.chipButton == "Find it")
    }

    @Test func yearGroupsRememberWhatTheUserOpenedAndDefaultToTheNewest() {
        let store = MemoryStore()
        var memory = YearGroupMemory(store: store)
        #expect(memory.isOpen("2025", newest: "2025"))
        #expect(!memory.isOpen("2024", newest: "2025"))
        #expect(!memory.isOpen("undated", newest: "2025"))
        memory.set("2024", open: true)
        memory.set("2025", open: false)
        #expect(memory.isOpen("2024", newest: "2025"))
        #expect(!memory.isOpen("2025", newest: "2025"))
        let again = YearGroupMemory(store: store)
        #expect(again.isOpen("2024", newest: "2025"), "persists")
        #expect(!again.isOpen("2025", newest: "2025"))
        #expect(again.isOpen("2026", newest: "2026"), "a new newest year opens by default")
    }

    @Test func detailsLinesReadLikeTheBoard() {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let read = ReadEvent(agent: "Claude", codename: "w2-2025-2", at: now.addingTimeInterval(-120))
        let d = DocDetails(location: "Taxes 2025/", droppedAt: now.addingTimeInterval(-86_400 * 40), droppedWith: 8, lastRead: read)
        let lines = DocDetailsModel.lines(d, privateFolderDisplay: "~/Private", now: now,
                                          calendar: Calendar(identifier: .gregorian), timeZone: TimeZone(identifier: "UTC")!)
        #expect(lines.map(\.label) == ["In", "Dropped", "Last read"])
        #expect(lines[0].value == "~/Private/Taxes 2025/")
        #expect(lines[1].value.hasSuffix(", with 8 others"), "\(lines[1].value)")
        #expect(lines[2].value == "Claude, 2m ago · saw only ink")
        let loose = DocDetails(location: "", droppedAt: now, droppedWith: 0, lastRead: nil)
        let l2 = DocDetailsModel.lines(loose, privateFolderDisplay: "~/Private", now: now,
                                       calendar: Calendar(identifier: .gregorian), timeZone: TimeZone(identifier: "UTC")!)
        #expect(l2.map(\.label) == ["In", "Dropped"], "no last-read line until an agent reads it")
        #expect(l2[0].value == "~/Private/")
        #expect(!l2[1].value.contains("with"))
        #expect(DocDetailsModel.showInFinder == "Show in Finder")
    }

    @Test func theTwoNewSettingsDefaultOn() {
        let store = MemoryStore()
        var s = AppSettings(store: store)
        #expect(s.showsPayer)
        #expect(s.offersClipboardHandle)
        s.showsPayer = false
        s.offersClipboardHandle = false
        let again = AppSettings(store: store)
        #expect(!again.showsPayer)
        #expect(!again.offersClipboardHandle)
        s.showsPayer = true
        #expect(AppSettings(store: store).showsPayer)
        #expect(AppSettings.showPayerLabel == "Show payer names in the list")
        #expect(AppSettings.clipboardChipLabel == "Offer to find a handle from the clipboard")
    }

    @Test func thePulseIsARingThatFadesOutInOneAndAHalfSeconds() {
        #expect(InkPulse.duration == 1.5)
        let size = CGSize(width: 316, height: 64)
        let start = InkPulse.field(size: size, time: 0)
        #expect(start.width == 39 && start.height == 8)
        // At the start the ring sits at the centre; by the end everything is gone.
        let mid = InkPulse.field(size: size, time: 0.4)
        let center = mid.values[mid.width * (mid.height / 2) + mid.width / 2]
        let edge = mid.values[0]
        #expect(center < edge || mid.values.max()! > 0.3, "the ring has moved out from the centre")
        let end = InkPulse.field(size: size, time: 1.5)
        #expect(end.values.allSatisfy { $0 < 0.02 })
        #expect(InkPulse.field(size: size, time: 9).values.allSatisfy { $0 == 0 })
        #expect(InkPulse.field(size: size, time: 0.2).values.max()! > 0.5)
    }
}

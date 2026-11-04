import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// What Copy puts on the clipboard: the handle, and by default a short note
/// for the chat, because Claude Desktop may not have INKER's tools loaded
/// when a bare handle arrives (stack-decision.md section 10).
struct HandlePasteTests {
    @Test func theNoteFollowsTheHandleOnOneLine() {
        let text = HandlePaste.text(handle: "🐙/folder-4392", withNote: true)
        #expect(text == "🐙/folder-4392 (INKER handle: read it with the INKER tool, not from disk)")
        #expect(!text.contains("\n"), "one line, no trailing newline")
        #expect(text.hasPrefix("🐙/folder-4392"), "the handle leads")
        #expect(HandlePaste.text(handle: "ink/w2-acme-2025", withNote: true)
            == "ink/w2-acme-2025 (INKER handle: read it with the INKER tool, not from disk)")
    }

    @Test func withTheNoteOffCopyIsTheBareHandle() {
        #expect(HandlePaste.text(handle: "🐙/folder-4392", withNote: false) == "🐙/folder-4392")
    }

    @Test func theExampleShowsWhatGetsPastedInTheChosenStyle() {
        #expect(HandlePaste.example(style: .octopus, withNote: true)
            == "🐙/w2-acme-2025 (INKER handle: read it with the INKER tool, not from disk)")
        #expect(HandlePaste.example(style: .ascii, withNote: false) == "ink/w2-acme-2025")
    }

    @Test func theNoteSettingDefaultsOnAndPersists() {
        let store = MemoryStore()
        let s = AppSettings(store: store)
        #expect(s.copiesHandleNote, "on by default")
        s.copiesHandleNote = false
        #expect(!AppSettings(store: store).copiesHandleNote)
        s.copiesHandleNote = true
        #expect(AppSettings(store: store).copiesHandleNote)
        #expect(AppSettings.copyHandleNoteLabel == "Add a note for the chat when copying a handle")
    }

    @Test func theClipboardChipStillFindsAHandleThatCarriesTheNote() {
        let noted = HandlePaste.text(handle: "🐙/w2-2025-2", withNote: true)
        #expect(ClipboardHandle.parse(noted) == "🐙/w2-2025-2")
        #expect(ClipboardHandle.parse(noted + "\n") == "🐙/w2-2025-2")
        #expect(ClipboardHandle.parse(HandlePaste.text(handle: "ink/w2-2025-2", withNote: true)) == "ink/w2-2025-2")
        #expect(ClipboardHandle.parse("🐙/w2-2025-2") == "🐙/w2-2025-2", "and without it")
        #expect(ClipboardHandle.parse("🐙/w2-2025-2 (something else)") == nil, "only INKER's own note")
        #expect(ClipboardHandle.parse("🐙/Not A Codename " + HandlePaste.note) == nil)
    }
}

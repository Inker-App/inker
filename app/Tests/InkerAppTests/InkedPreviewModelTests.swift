import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// "Preview inked": the document as the agent reads it. A header (handle,
/// then "W-2 · 2025 · 6 inked"), each page under a "page 1 of 3" divider as
/// runs of plain text and tokens cut at the core's byte spans, form fields
/// last, and the empty states.
struct InkedPreviewModelTests {
    /// A token at its byte offsets in `text`, found the way the core reports
    /// it (UTF-8 bytes, not characters).
    private func token(_ t: String, in text: String, kind: String, role: String? = nil, occurrence: Int = 0) -> InkedToken {
        var from = text.startIndex
        var range = text.range(of: t)!
        for _ in 0..<occurrence {
            from = range.upperBound
            range = text.range(of: t, range: from..<text.endIndex)!
        }
        let start = text.utf8.distance(from: text.utf8.startIndex, to: range.lowerBound)
        let end = text.utf8.distance(from: text.utf8.startIndex, to: range.upperBound)
        return InkedToken(span: TextSpan(start: UInt32(start), end: UInt32(end)), token: t, kindLabel: kind, role: role)
    }

    private func page(_ n: UInt32, _ text: String, _ tokens: [InkedToken]) -> InkedPage {
        InkedPage(page: n, text: text, tokens: tokens)
    }

    private func view(inked: UInt32 = 6, year: UInt16? = 2025, pages: [InkedPage], fields: [InkedField] = []) -> InkedView {
        InkedView(handle: "🐙/w2-2025", kindLabel: "W-2", year: year, inked: inked, pages: pages, fields: fields)
    }

    // MARK: Runs

    @Test func plainTextAndTokensAlternate() {
        let text = "SSN [SSN_1] for [PERSON_1]."
        let runs = InkedPreviewModel.runs(text: text, tokens: [
            token("[SSN_1]", in: text, kind: "SSN"),
            token("[PERSON_1]", in: text, kind: "PERSON"),
        ])
        #expect(runs == [.text("SSN "), .token("[SSN_1]"), .text(" for "), .token("[PERSON_1]"), .text(".")])
    }

    @Test func spansAreBytesSoMultiByteTextCutsCleanly() {
        let text = "Café · 🐙 [ORG_1] — Zoë [PERSON_2]"
        let runs = InkedPreviewModel.runs(text: text, tokens: [
            token("[ORG_1]", in: text, kind: "ORG"),
            token("[PERSON_2]", in: text, kind: "PERSON"),
        ])
        #expect(runs == [.text("Café · 🐙 "), .token("[ORG_1]"), .text(" — Zoë "), .token("[PERSON_2]")])
    }

    @Test func adjacentTokensHaveNoTextBetweenThem() {
        let text = "[PERSON_1][SSN_1]"
        let runs = InkedPreviewModel.runs(text: text, tokens: [
            token("[PERSON_1]", in: text, kind: "PERSON"),
            token("[SSN_1]", in: text, kind: "SSN"),
        ])
        #expect(runs == [.token("[PERSON_1]"), .token("[SSN_1]")])
    }

    @Test func tokensAtTheStartAndTheEnd() {
        let text = "[SSN_1] and [SSN_1]"
        let runs = InkedPreviewModel.runs(text: text, tokens: [
            token("[SSN_1]", in: text, kind: "SSN"),
            token("[SSN_1]", in: text, kind: "SSN", occurrence: 1),
        ])
        #expect(runs == [.token("[SSN_1]"), .text(" and "), .token("[SSN_1]")])
    }

    @Test func anEmptyPageHasNoRuns() {
        #expect(InkedPreviewModel.runs(text: "", tokens: []).isEmpty)
    }

    @Test func aPageWithNoTokensIsOneTextRun() {
        #expect(InkedPreviewModel.runs(text: "Line one\nLine two", tokens: []) == [.text("Line one\nLine two")])
    }

    @Test func tokensArrivingOutOfOrderAreSorted() {
        let text = "a [SSN_1] b [EIN_1]"
        let runs = InkedPreviewModel.runs(text: text, tokens: [
            token("[EIN_1]", in: text, kind: "EIN"),
            token("[SSN_1]", in: text, kind: "SSN"),
        ])
        #expect(runs == [.text("a "), .token("[SSN_1]"), .text(" b "), .token("[EIN_1]")])
    }

    @Test func aSpanThatDoesNotSpellItsTokenStaysText() {
        // Out of range, mid-character and mismatched spans are never drawn
        // as chips: the page text is shown as it is.
        let text = "é [SSN_1]"
        let bad = [
            InkedToken(span: TextSpan(start: 3, end: 99), token: "[SSN_1]", kindLabel: "SSN", role: nil),
            InkedToken(span: TextSpan(start: 1, end: 2), token: "[SSN_1]", kindLabel: "SSN", role: nil),
            InkedToken(span: TextSpan(start: 0, end: 3), token: "[SSN_1]", kindLabel: "SSN", role: nil),
        ]
        #expect(InkedPreviewModel.runs(text: text, tokens: bad) == [.text(text)])
    }

    @Test func overlappingSpansKeepTheFirst() {
        let text = "[SSN_1]"
        let t = token("[SSN_1]", in: text, kind: "SSN")
        #expect(InkedPreviewModel.runs(text: text, tokens: [t, t]) == [.token("[SSN_1]")])
    }

    // MARK: The screen

    @Test func theHeaderIsTheHandleThenKindYearAndCount() {
        let m = InkedPreviewModel.build(.view(view(pages: [page(1, "x", [])])))
        #expect(m.handle == "🐙/w2-2025")
        #expect(m.detail == "W-2 · 2025 · 6 inked")
        #expect(m.emptyState == nil)
        #expect(m.notice == nil)
    }

    @Test func noYearLeavesItOut() {
        let m = InkedPreviewModel.build(.view(view(year: nil, pages: [page(1, "x", [])])))
        #expect(m.detail == "W-2 · 6 inked")
    }

    @Test func eachPageGetsADividerOfThePageCount() {
        let text = "Employee [PERSON_1]"
        let m = InkedPreviewModel.build(.view(view(pages: [
            page(1, text, [token("[PERSON_1]", in: text, kind: "PERSON", role: "employee")]),
            page(2, "", []),
            page(3, "End", []),
        ])))
        #expect(m.pages.map(\.divider) == ["page 1 of 3", "page 2 of 3", "page 3 of 3"])
        #expect(m.pages[0].runs == [.text("Employee "), .token("[PERSON_1]")])
        #expect(m.pages[1].runs.isEmpty)
        #expect(Set(m.pages.map(\.id)).count == 3)
    }

    @Test func formFieldsFollowThePages() {
        let m = InkedPreviewModel.build(.view(view(pages: [page(1, "x", [])], fields: [
            InkedField(name: "Employee SSN", value: "[SSN_1]"),
            InkedField(name: "Employer EIN", value: "[EIN_1]"),
        ])))
        #expect(InkedPreviewModel.fieldsLabel == "Form fields")
        #expect(m.fields.map(\.name) == ["Employee SSN", "Employer EIN"])
        #expect(m.fields.map(\.value) == ["[SSN_1]", "[EIN_1]"])
        #expect(InkedPreviewModel.build(.view(view(pages: []))).fields.isEmpty)
    }

    @Test func nothingInkedSaysSoAboveTheText() {
        // The text is still shown: it is what the agent reads, and seeing it
        // is how the user catches a value INKER missed.
        let m = InkedPreviewModel.build(.view(view(inked: 0, pages: [page(1, "A note.", [])])))
        #expect(m.notice == "Nothing inked in this document")
        #expect(m.detail == "W-2 · 2025 · 0 inked")
        #expect(m.pages.count == 1)
        #expect(m.emptyState == nil)
    }

    @Test func aHeldDocumentAsksForTheCardFirst() {
        let m = InkedPreviewModel.build(.unavailable(handle: "🐙/w2-2025", reason: "pending review"))
        #expect(m.handle == "🐙/w2-2025")
        #expect(m.detail == nil)
        #expect(m.emptyState == "Answer the card first")
        #expect(m.pages.isEmpty)
        #expect(m.fields.isEmpty)
    }

    @Test func aDocumentInFlightIsStillInking() {
        let m = InkedPreviewModel.build(.unavailable(handle: "🐙/w2-2025", reason: "not ready"))
        #expect(m.emptyState == "Still inking")
    }

    @Test func anyOtherReasonGetsTheFallback() {
        let m = InkedPreviewModel.build(.unavailable(handle: "🐙/x", reason: "io error"))
        #expect(m.emptyState == InkedPreviewModel.couldNotLoad)
    }

    @Test func theReasonIsTakenFromEveryFfiErrorCase() {
        #expect(InkedPreviewModel.reason(of: FfiError.Backend(reason: "pending review")) == "pending review")
        #expect(InkedPreviewModel.reason(of: FfiError.InvalidArgument(reason: "a folder has no inked view"))
            == "a folder has no inked view")
        #expect(InkedPreviewModel.reason(of: FfiError.NotFound(reason: "no document with that id"))
            == "no document with that id")
        struct Other: Error {}
        #expect(InkedPreviewModel.reason(of: Other()) == "")
    }
}

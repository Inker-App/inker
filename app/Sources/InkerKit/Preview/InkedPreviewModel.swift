import Foundation
import InkerCore

/// "Preview inked": a document exactly as an agent reads it, so the user can
/// check what INKER inked. The core sends `resolve`'s pages with each token's
/// byte span; this cuts them into runs the view draws, plain text and token
/// chips. Pure: no AppKit, no FFI calls. Tokenized text only: nothing here
/// ever holds a real value.

/// What the preview screen was opened with: the core's inked view, or the
/// reason it has none (a fixed, value-free string from the core).
public enum InkedPreviewContent: Equatable, Sendable {
    case view(InkedView)
    /// `handle` is the row's, for the header.
    case unavailable(handle: String, reason: String)
}

/// A stretch of page text, or one token drawn as a chip.
public enum InkedRun: Equatable, Sendable {
    case text(String)
    /// `[SSN_1]`
    case token(String)
}

/// One page: its divider and its runs. An empty page has no runs.
public struct InkedPageModel: Equatable, Sendable, Identifiable {
    /// Position in the document, from 0.
    public var id: Int
    /// "page 1 of 3"
    public var divider: String
    public var runs: [InkedRun]
}

/// A form field: its name, and its value as the agent reads it.
public struct InkedFieldLine: Equatable, Sendable, Identifiable {
    public var id: Int
    public var name: String
    public var value: String
}

/// The screen: header, then pages, then form fields, or an empty state.
public struct InkedPreviewModel: Equatable, Sendable {
    /// `🐙/w2-2025`, in mono.
    public var handle: String
    /// "W-2 · 2025 · 6 inked", in the rows' detail style. `nil` when there
    /// is no view to describe.
    public var detail: String?
    /// "Nothing inked in this document", above the text, which still shows.
    public var notice: String?
    /// Instead of the pages: "Answer the card first", "Still inking".
    public var emptyState: String?
    public var pages: [InkedPageModel]
    public var fields: [InkedFieldLine]

    public static let back = "Back"
    public static let fieldsLabel = "Form fields"
    public static let nothingInked = "Nothing inked in this document"
    public static let answerTheCard = "Answer the card first"
    public static let stillInking = "Still inking"
    /// Any other reason. Placeholder copy: the designs do not show it.
    public static let couldNotLoad = "Couldn't show this document"

    /// The core's fixed reasons (`inker-ffi/src/inked.rs`).
    public static let pendingReviewReason = "pending review"
    public static let notReadyReason = "not ready"

    public static func build(_ content: InkedPreviewContent) -> InkedPreviewModel {
        switch content {
        case .unavailable(let handle, let reason):
            return InkedPreviewModel(handle: handle, detail: nil, notice: nil, emptyState: emptyState(for: reason),
                                     pages: [], fields: [])
        case .view(let v):
            var parts = [v.kindLabel]
            if let year = v.year { parts.append(String(year)) }
            parts.append("\(v.inked) inked")
            let count = v.pages.count
            let pages = v.pages.enumerated().map { i, p in
                InkedPageModel(id: i, divider: "page \(p.page) of \(count)", runs: runs(text: p.text, tokens: p.tokens))
            }
            let fields = v.fields.enumerated().map { i, f in InkedFieldLine(id: i, name: f.name, value: f.value) }
            return InkedPreviewModel(handle: v.handle, detail: parts.joined(separator: " · "),
                                     notice: v.inked == 0 ? nothingInked : nil, emptyState: nil,
                                     pages: pages, fields: fields)
        }
    }

    static func emptyState(for reason: String) -> String {
        switch reason {
        case pendingReviewReason: return answerTheCard
        case notReadyReason: return stillInking
        default: return couldNotLoad
        }
    }

    /// The reason an `FfiError` carries, whatever its case; empty otherwise.
    public static func reason(of error: Error) -> String {
        guard let e = error as? FfiError else { return "" }
        switch e {
        case .InvalidArgument(let reason), .NotFound(let reason), .Backend(let reason):
            return reason
        }
    }

    /// Cut `text` at its tokens' byte spans. A token becomes a chip only when
    /// its span lies inside the text, on character boundaries, does not
    /// overlap the one before, and spells its token; otherwise the text is
    /// shown as it is.
    public static func runs(text: String, tokens: [InkedToken]) -> [InkedRun] {
        let bytes = Array(text.utf8)
        var out: [InkedRun] = []
        var cursor = 0
        for t in tokens.sorted(by: { $0.span.start < $1.span.start }) {
            let start = Int(t.span.start)
            let end = Int(t.span.end)
            guard start >= cursor, start < end, end <= bytes.count,
                  let spelled = String(bytes: bytes[start..<end], encoding: .utf8), spelled == t.token,
                  let before = String(bytes: bytes[cursor..<start], encoding: .utf8)
            else { continue }
            if !before.isEmpty { out.append(.text(before)) }
            out.append(.token(t.token))
            cursor = end
        }
        if cursor < bytes.count, let rest = String(bytes: bytes[cursor...], encoding: .utf8) {
            out.append(.text(rest))
        }
        return out
    }
}

import Foundation
import InkerCore

/// The pure half of the macOS providers: shaping what PDFKit and Vision read
/// into the `ExtractedFfi` the Rust core takes (stack-decision 3.3). No
/// AppKit, PDFKit or Vision here, so `swift test` covers it.
///
/// Values pass through this code on their way into the core. Nothing here
/// logs, and nothing should.
public enum ProviderText {
    /// One AcroForm widget as PDFKit reports it.
    public struct Field: Equatable, Sendable {
        public let name: String
        public let value: String
        /// 0-based PDFKit page index, if the widget's page is known.
        public let pageIndex: Int?

        public init(name: String, value: String, pageIndex: Int?) {
            self.name = name
            self.value = value
            self.pageIndex = pageIndex
        }
    }

    /// Empty or whitespace only (including no-break spaces and newlines).
    public static func isBlank(_ s: String) -> Bool {
        s.allSatisfy(\.isWhitespace)
    }

    /// Recognised lines, top to bottom, as one page of text. Trailing
    /// whitespace on each line is dropped; leading indentation is kept.
    public static func joinLines(_ lines: [String]) -> String {
        lines.map(trimTrailing).joined(separator: "\n")
    }

    /// Page texts in order, numbered from 1 as the core expects.
    public static func pages(_ texts: [String]) -> [ExtractedPage] {
        texts.enumerated().map { ExtractedPage(page: UInt32($0.offset + 1), text: $0.element) }
    }

    /// Widgets to fields: unnamed and blank ones dropped, and one field per
    /// name (radio groups and repeated widgets share a name; the first filled
    /// one wins).
    public static func fields(_ widgets: [Field]) -> [ExtractedField] {
        var seen = Set<String>()
        var out: [ExtractedField] = []
        for w in widgets where !w.name.isEmpty && !isBlank(w.value) && !seen.contains(w.name) {
            seen.insert(w.name)
            out.append(ExtractedField(name: w.name, value: w.value,
                                      page: w.pageIndex.map { UInt32($0 + 1) }))
        }
        return out
    }

    /// What `PDFKitTextSource` returns: the pages and fields, or nil when the
    /// PDF has no usable text layer (every page blank and no filled field),
    /// which sends it to OCR.
    public static func textLayer(pageTexts: [String], fields widgets: [Field]) -> ExtractedFfi? {
        let filled = Self.fields(widgets)
        if pageTexts.allSatisfy({ Self.isBlank($0) }), filled.isEmpty {
            return nil
        }
        return ExtractedFfi(pages: Self.pages(pageTexts), fields: filled)
    }

    /// What `VisionOcrSource` returns: one page per rendered page or image,
    /// each the lines Vision recognised. Blank pages are kept so numbering
    /// matches the PDF.
    public static func ocrResult(pageLines: [[String]]) -> ExtractedFfi {
        ExtractedFfi(pages: Self.pages(pageLines.map { Self.joinLines($0) }), fields: [])
    }

    /// One recognised fragment and its box, normalised 0...1 with the origin
    /// at the bottom left (Vision's convention).
    public struct TextBox: Equatable, Sendable {
        public let text: String
        public let minX: Double
        public let minY: Double
        public let maxX: Double
        public let maxY: Double

        public init(text: String, minX: Double, minY: Double, maxX: Double, maxY: Double) {
            self.text = text
            self.minX = minX
            self.minY = minY
            self.maxX = maxX
            self.maxY = maxY
        }

        var midY: Double { (minY + maxY) / 2 }
        var height: Double { maxY - minY }
    }

    /// Fragments to lines: top to bottom, and fragments whose vertical
    /// centres are within half a line of each other joined left to right with
    /// a space, so a form's label and its value land on one line.
    public static func readingOrder(_ boxes: [TextBox]) -> [String] {
        var rows: [[TextBox]] = []
        for b in boxes.sorted(by: { $0.midY > $1.midY }) {
            if let anchor = rows.last?.first,
               abs(anchor.midY - b.midY) < min(anchor.height, b.height) / 2 {
                rows[rows.count - 1].append(b)
            } else {
                rows.append([b])
            }
        }
        return rows.map { row in
            row.sorted(by: { $0.minX < $1.minX }).map(\.text).joined(separator: " ")
        }
    }

    /// Pixel size for rendering a PDF page of the given size in points
    /// (1/72 inch) at `dpi`. Never zero.
    public static func renderSize(widthPoints: Double, heightPoints: Double,
                                  dpi: Double = 200) -> (width: Int, height: Int) {
        let scale = dpi / 72
        return (max(1, Int((widthPoints * scale).rounded())),
                max(1, Int((heightPoints * scale).rounded())))
    }

    private static func trimTrailing(_ s: String) -> String {
        var s = Substring(s)
        while let last = s.last, last.isWhitespace {
            s = s.dropLast()
        }
        return String(s)
    }
}

/// The vault key's shape and where the Keychain keeps it.
public enum VaultKey {
    public static let length = 32
    /// Keychain generic-password service.
    public static let service = "sh.inker.vault"
    /// Keychain generic-password account.
    public static let account = "vault-key"

    public static func isValid(_ data: Data) -> Bool {
        data.count == length
    }
}

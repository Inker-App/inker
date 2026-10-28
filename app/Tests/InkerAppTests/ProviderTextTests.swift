import Foundation
import InkerCore
import Testing
@testable import InkerKit

/// The pure half of the macOS providers: how PDFKit and Vision output becomes
/// the `ExtractedFfi` the core reads (stack-decision 3.3). All text here is
/// made up and carries no identifiers.
struct ProviderTextTests {
    // MARK: Blank detection

    @Test func blankMeansEmptyOrWhitespaceOnly() {
        #expect(ProviderText.isBlank(""))
        #expect(ProviderText.isBlank(" \n\t\r\n"))
        #expect(ProviderText.isBlank("\u{00A0}"), "a no-break space is still blank")
        #expect(!ProviderText.isBlank(" x "))
    }

    // MARK: Lines and pages

    @Test func linesJoinTopToBottomWithTrailingSpaceTrimmed() {
        #expect(ProviderText.joinLines(["Wages  ", "Box 1", "  indented"]) == "Wages\nBox 1\n  indented")
        #expect(ProviderText.joinLines([]) == "")
    }

    @Test func pagesAreNumberedFromOne() {
        let pages = ProviderText.pages(["first", "", "third"])
        #expect(pages.map(\.page) == [1, 2, 3])
        #expect(pages.map(\.text) == ["first", "", "third"])
    }

    // MARK: Text layer

    @Test func aTextLayerWithAnyTextIsKept() {
        let got = ProviderText.textLayer(pageTexts: ["", "Form W-2 sample"], fields: [])
        #expect(got?.pages.count == 2)
        #expect(got?.pages[1] == ExtractedPage(page: 2, text: "Form W-2 sample"))
    }

    @Test func aTextLayerThatIsAllBlankIsAScan() {
        #expect(ProviderText.textLayer(pageTexts: [" ", "\n"], fields: []) == nil)
        #expect(ProviderText.textLayer(pageTexts: [], fields: []) == nil)
    }

    @Test func filledFieldsAloneStillCountAsText() {
        let field = ProviderText.Field(name: "f1_01", value: "sample", pageIndex: 0)
        let got = ProviderText.textLayer(pageTexts: [""], fields: [field])
        #expect(got?.fields == [ExtractedField(name: "f1_01", value: "sample", page: 1)])
    }

    @Test func fieldsAreDedupedByNameAndBlankOnesDropped() {
        let fields = [
            ProviderText.Field(name: "c1", value: "", pageIndex: 0),
            ProviderText.Field(name: "c1", value: "Yes", pageIndex: 0),
            ProviderText.Field(name: "c1", value: "No", pageIndex: 1),
            ProviderText.Field(name: "", value: "orphan", pageIndex: 0),
            ProviderText.Field(name: "f2", value: "  ", pageIndex: nil),
            ProviderText.Field(name: "f3", value: "kept", pageIndex: nil),
        ]
        let got = ProviderText.fields(fields)
        #expect(got == [
            ExtractedField(name: "c1", value: "Yes", page: 1),
            ExtractedField(name: "f3", value: "kept", page: nil),
        ])
    }

    // MARK: OCR

    @Test func ocrJoinsEachPagesLines() {
        let got = ProviderText.ocrResult(pageLines: [["a", "b"], [], ["c"]])
        #expect(got.pages == [
            ExtractedPage(page: 1, text: "a\nb"),
            ExtractedPage(page: 2, text: ""),
            ExtractedPage(page: 3, text: "c"),
        ])
        #expect(got.fields.isEmpty)
    }

    // MARK: Reading order (VNRecognizeTextRequest fallback)

    /// Vision boxes are normalised with the origin at the bottom left.
    private func box(_ text: String, x: Double, y: Double, w: Double = 0.2, h: Double = 0.02) -> ProviderText.TextBox {
        ProviderText.TextBox(text: text, minX: x, minY: y, maxX: x + w, maxY: y + h)
    }

    @Test func fragmentsOnOneRowJoinLeftToRightAndRowsRunTopToBottom() {
        let boxes = [
            box("bottom", x: 0.1, y: 0.10),
            box("value", x: 0.6, y: 0.805),   // same row as "label", slightly lower
            box("label", x: 0.1, y: 0.80),
            box("middle", x: 0.1, y: 0.50),
        ]
        #expect(ProviderText.readingOrder(boxes) == ["label value", "middle", "bottom"])
    }

    @Test func rowsThatDoNotOverlapStaySeparate() {
        let boxes = [box("a", x: 0.1, y: 0.50), box("b", x: 0.5, y: 0.47)]
        #expect(ProviderText.readingOrder(boxes) == ["a", "b"])
        #expect(ProviderText.readingOrder([]) == [])
    }

    // MARK: Rendering for OCR

    @Test func renderSizeIsPointsAt200Dpi() {
        // US Letter, 612 x 792 pt, is 1700 x 2200 px at 200 dpi.
        let size = ProviderText.renderSize(widthPoints: 612, heightPoints: 792)
        #expect(size.width == 1700)
        #expect(size.height == 2200)
        let small = ProviderText.renderSize(widthPoints: 0.1, heightPoints: 0, dpi: 200)
        #expect(small.width == 1 && small.height == 1, "never zero")
    }

    // MARK: Vault key

    @Test func theVaultKeyIs32Bytes() {
        #expect(VaultKey.length == 32)
        #expect(VaultKey.service == "sh.inker.vault")
        #expect(VaultKey.isValid(Data(repeating: 1, count: 32)))
        #expect(!VaultKey.isValid(Data(repeating: 1, count: 31)))
        #expect(!VaultKey.isValid(Data()))
    }
}

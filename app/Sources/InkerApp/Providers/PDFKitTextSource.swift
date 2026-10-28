import Foundation
import InkerCore
import InkerKit
import PDFKit

/// A PDF's text layer and AcroForm fields through PDFKit (stack-decision
/// 3.3): `PDFPage.string` per page, and each widget annotation's `fieldName`
/// and `widgetStringValue`. Returns nil when there is no usable text layer,
/// so the core falls back to OCR.
///
/// Called from a Rust background thread. Each call opens its own
/// `PDFDocument`, so no PDFKit object is shared across threads.
struct PDFKitTextSource {
    func extract(path: String) throws -> ExtractedFfi? {
        guard let document = PDFDocument(url: URL(fileURLWithPath: path)) else {
            throw ProviderError.Failed(reason: "PDFKit could not open the file")
        }
        if document.isLocked {
            throw ProviderError.Failed(reason: "the PDF is password-protected")
        }
        var texts: [String] = []
        var widgets: [ProviderText.Field] = []
        for index in 0..<document.pageCount {
            autoreleasepool {
                guard let page = document.page(at: index) else {
                    texts.append("")
                    return
                }
                texts.append(page.string ?? "")
                // VERIFY: `PDFAnnotation.type` is the subtype without the
                // leading slash ("Widget"), matching PDFAnnotationSubtype.widget.
                // Checkboxes and radio buttons have a nil widgetStringValue;
                // their state (buttonWidgetState) is not read yet.
                for annotation in page.annotations where annotation.type == PDFAnnotationSubtype.widget.rawValue {
                    guard let name = annotation.fieldName else { continue }
                    widgets.append(ProviderText.Field(name: name, value: annotation.widgetStringValue ?? "",
                                                      pageIndex: index))
                }
            }
        }
        return ProviderText.textLayer(pageTexts: texts, fields: widgets)
    }
}

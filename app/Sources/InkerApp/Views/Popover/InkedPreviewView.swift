import InkerKit
import SwiftUI

/// "Preview inked", a screen of the popover: the document exactly as an
/// agent reads it, every token drawn as a chip, so the user can check what
/// INKER inked. Back returns to the board. The model (InkerKit's
/// InkedPreviewModel) does the cutting; this only draws runs.
///
/// Placeholder layout for Ben: the designs do not show this screen.
struct InkedPreviewView: View {
    let content: InkedPreviewContent
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        let model = InkedPreviewModel.build(content)
        VStack(spacing: 0) {
            header(model)
            Hairline(color: Palette.hairlineFaint)
            if let empty = model.emptyState {
                Text(empty)
                    .font(Typography.body)
                    .foregroundColor(Palette.muted)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                PopoverScroll {
                    VStack(alignment: .leading, spacing: 14) {
                        if let notice = model.notice {
                            Text(notice)
                                .font(Typography.small)
                                .foregroundColor(Palette.muted)
                        }
                        ForEach(model.pages) { page in
                            InkedPageView(page: page)
                        }
                        if !model.fields.isEmpty {
                            InkedFieldsView(fields: model.fields)
                        }
                    }
                    .padding(.horizontal, PopoverLayout.gutter)
                    .padding(.top, 12)
                    .padding(.bottom, 12)
                    .frame(width: PopoverLayout.width, alignment: .topLeading)
                }
                .frame(maxHeight: .infinity)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    /// Back (muted text, as on the onboarding steps), then the handle in
    /// mono and "W-2 · 2025 · 6 inked" in the rows' detail style.
    private func header(_ model: InkedPreviewModel) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Button(InkedPreviewModel.back) { store.closeInkedPreview() }
                .buttonStyle(.plain)
                .font(Typography.body)
                .foregroundColor(Palette.muted)
                .frame(height: 28)
            HStack(spacing: 6) {
                Text(model.handle).font(Typography.mono(11)).foregroundColor(Palette.paper)
                if let detail = model.detail {
                    Text("·").foregroundColor(Palette.muted)
                    Text(detail).foregroundColor(Palette.muted)
                }
            }
            .font(Typography.caption)
            .lineLimit(1)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, PopoverLayout.gutter)
        .padding(.top, 6)
        .padding(.bottom, 10)
    }
}

/// "page 1 of 3" and a rule, then the page as one wrapping text.
struct InkedPageView: View {
    let page: InkedPageModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                Text(page.divider)
                    .font(Typography.caption)
                    .foregroundColor(Palette.muted)
                    .fixedSize()
                Hairline(color: Palette.hairlineFaint)
            }
            if !page.runs.isEmpty {
                Text(InkedText.attributed(page.runs))
                    .font(Typography.body)
                    .foregroundColor(Palette.paper)
                    .fixedSize(horizontal: false, vertical: true)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
    }
}

/// "FORM FIELDS": each name over its value, as the agent reads it.
struct InkedFieldsView: View {
    let fields: [InkedFieldLine]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            LabelText(InkedPreviewModel.fieldsLabel, color: Palette.muted)
            ForEach(fields) { field in
                VStack(alignment: .leading, spacing: 2) {
                    Text(field.name)
                        .font(Typography.caption)
                        .foregroundColor(Palette.muted)
                    Text(field.value)
                        .font(Typography.mono(12))
                        .foregroundColor(Palette.paper)
                        .fixedSize(horizontal: false, vertical: true)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// Runs as one attributed string, so the page wraps as a paragraph: plain
/// text in the body face, each token a chip in mono, paper on the rim grey,
/// padded by a no-break space either side.
enum InkedText {
    static func attributed(_ runs: [InkedRun]) -> AttributedString {
        var out = AttributedString()
        for run in runs {
            switch run {
            case .text(let text):
                out.append(AttributedString(text))
            case .token(let token):
                // VERIFY: SwiftUI Text draws the background attribute as a
                // rectangle behind the glyphs, not a rounded pill. A true
                // pill needs a flow layout of words; ask Ben if this is not
                // close enough.
                var chip = AttributedString("\u{00A0}\(token)\u{00A0}")
                chip[AttributeScopes.SwiftUIAttributes.FontAttribute.self] = Typography.mono(12)
                chip[AttributeScopes.SwiftUIAttributes.ForegroundColorAttribute.self] = Palette.paper
                chip[AttributeScopes.SwiftUIAttributes.BackgroundColorAttribute.self] = Palette.rim
                out.append(chip)
            }
        }
        return out
    }
}

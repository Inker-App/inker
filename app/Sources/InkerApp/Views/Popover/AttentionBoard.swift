import InkerCore
import InkerKit
import SwiftUI

/// AppPopAttention: folder header, the attention card, compact ready rows.
struct AttentionBoard: View {
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        PopoverScroll {
            VStack(spacing: 0) {
                ForEach(store.model.groups) { group in
                    GroupHeader(group: group)
                        .padding(.vertical, 10)
                    if let card = group.attention {
                        AttentionCard(card: card)
                    }
                    ForEach(Array(group.rows.enumerated()), id: \.element.id) { index, row in
                        HStack {
                            Text(row.title).font(Typography.mono(12))
                            Spacer(minLength: 8)
                            Text(row.detail).font(Typography.caption).foregroundColor(Palette.muted)
                        }
                        .padding(.top, index == 0 ? 14 : 8)
                        .padding(.bottom, 8)
                        .padding(.leading, 12)
                        if index < group.rows.count - 1 {
                            Hairline(color: Palette.hairlineFaint)
                        }
                    }
                }
            }
        }
        .padding(.top, 4)
        .padding(.horizontal, 16)
    }
}

/// The held file: where, the peek, the question, and the three answers.
struct AttentionCard: View {
    let card: AttentionModel
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(card.fileName).font(Typography.archivo(12, weight: 600)).lineLimit(1).truncationMode(.middle)
                    Text(card.handle).font(Typography.mono(11)).foregroundColor(Palette.muted)
                }
                Spacer(minLength: 8)
                Text(card.location).font(Typography.small).foregroundColor(Palette.muted)
            }
            if let snippet = store.snippets[card.questionID] {
                PageSnippetView(snippet: snippet)
            } else {
                PeekView(contextBefore: card.contextBefore, valueLength: card.valueLength)
            }
            Text(card.prompt).font(Typography.body)
            HStack(spacing: 8) {
                Button("Ink it") { store.answer(questionID: card.questionID, .inkIt) }
                    .buttonStyle(InkButtonStyle(kind: .primary, height: 30, horizontalPadding: 14))
                Button("Leave it") { store.answer(questionID: card.questionID, .leaveIt) }
                    .buttonStyle(InkButtonStyle(kind: .outline(opacity: 0.3), height: 30, horizontalPadding: 14))
                Spacer(minLength: 0)
                Button("Open file") { store.reveal(path: card.filePath) }
                    .buttonStyle(InkButtonStyle(kind: .link, height: 30, horizontalPadding: 6))
            }
            Text(card.footnote).font(Typography.caption).foregroundColor(Palette.muted)
        }
        .padding(12)
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Palette.cardBorder, lineWidth: 1))
    }
}

/// The page around the held value, the value outlined in red (Ben, 2026-10-29:
/// a tight crop with the span marked; the octopus only when there is no crop).
struct PageSnippetView: View {
    let snippet: PageSnippet.Rendered

    var body: some View {
        Image(decorative: snippet.image, scale: PageSnippet.scale)
            .frame(width: PageCrop.box.width, height: PageCrop.box.height)
            .overlay(alignment: .topLeading) {
                RoundedRectangle(cornerRadius: 3)
                    .strokeBorder(Palette.red, lineWidth: 1.5)
                    .frame(width: snippet.outline.width + 8, height: snippet.outline.height + 8)
                    .offset(x: snippet.outline.minX - 4, y: snippet.outline.minY - 4)
            }
            .clipShape(RoundedRectangle(cornerRadius: 6))
            .accessibilityElement()
            .accessibilityLabel("The page around the value, with the value outlined")
    }
}

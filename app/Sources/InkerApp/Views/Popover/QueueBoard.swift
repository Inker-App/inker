import InkerKit
import SwiftUI

/// AppPopProcessing and AppPopReady: "Drop more", then each group.
struct QueueBoard: View {
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        VStack(spacing: 0) {
            DropMoreStrip(highlighted: store.isDropTargeted)
            ScrollView(.vertical, showsIndicators: false) {
                VStack(spacing: 0) {
                    ForEach(store.model.groups) { group in
                        GroupSection(group: group)
                    }
                }
            }
        }
        .padding(.top, 12)
        .padding(.horizontal, 16)
    }
}

/// Folder header plus rows.
struct GroupSection: View {
    let group: GroupModel

    var body: some View {
        VStack(spacing: 0) {
            if group.showsHeader {
                GroupHeader(group: group)
                    .padding(.top, 14)
                    .padding(.bottom, 10)
                Hairline()
            }
            ForEach(Array(group.rows.enumerated()), id: \.element.id) { index, row in
                QueueRow(row: row)
                if index < group.rows.count - 1 {
                    Hairline(color: Palette.hairlineFaint)
                }
            }
        }
    }
}

struct GroupHeader: View {
    let group: GroupModel

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(group.handle).font(Typography.mono(13, weight: 600))
                Text(group.summaryLine).font(Typography.caption).foregroundColor(Palette.muted)
            }
            Spacer(minLength: 8)
            CopyButton(id: group.id, handle: group.handle, enabled: group.copyEnabled, size: .large)
        }
    }
}

struct QueueRow: View {
    let row: RowModel

    var body: some View {
        switch row.kind {
        case .ready:
            HStack {
                VStack(alignment: .leading, spacing: 2) {
                    Text(row.title).font(Typography.mono(12))
                    Text(row.detail).font(Typography.caption).foregroundColor(Palette.muted)
                }
                Spacer(minLength: 8)
                CopyButton(id: row.id, handle: row.handle, enabled: row.copyEnabled)
            }
            .padding(.vertical, 10)
            .padding(.leading, 12)
        case .processing(let mode):
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text(row.title).lineLimit(1).truncationMode(.middle)
                    Spacer(minLength: 8)
                    Text(row.detail).font(Typography.caption).foregroundColor(Palette.muted)
                }
                InkLoaderView(mode: mode)
            }
            .padding(.vertical, 12)
            .padding(.leading, 12)
        case .failed:
            HStack {
                Text(row.title).lineLimit(1).truncationMode(.middle)
                Spacer(minLength: 8)
                Text(row.detail).font(Typography.caption).foregroundColor(Palette.muted)
            }
            .padding(.vertical, 12)
            .padding(.leading, 12)
        }
    }
}

import InkerKit
import SwiftUI

/// The ready board (AppFindScale, AppFindSearch, AppFindHandle, AppFindRow):
/// "Drop more" beside Find, tag chips, the clipboard chip, year groups with
/// folder sub-groups, or a flat list of matches while a query is typed.
struct FindBoard: View {
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                DropMoreStrip(highlighted: store.isDropTargeted)
                if store.board.showsFind {
                    FindField()
                }
            }
            .padding(.top, 10)
            if store.results == nil, !store.board.chips.isEmpty {
                ChipRow(chips: store.board.chips) { store.setTag($0) }
                    .padding(.top, 8)
            }
            PopoverScroll {
                ScrollViewReader { proxy in
                    VStack(spacing: 0) {
                        if let handle = store.clipboardHandle {
                            ClipboardChip(handle: handle)
                                .padding(.top, 10)
                        }
                        if let results = store.results {
                            FindResults(results: results)
                        } else {
                            ForEach(store.board.groups) { group in
                                YearGroupSection(group: group)
                            }
                        }
                    }
                    .onChange(of: store.scrollTarget) { _, id in
                        if let id { withAnimation { proxy.scrollTo(id, anchor: .center) } }
                    }
                }
            }
        }
        .padding(.horizontal, PopoverLayout.gutter)
        .onTapGesture { store.cancelForget() }
    }
}

/// The Find field: 36 tall beside "Drop more", a magnifier at the left.
struct FindField: View {
    @EnvironmentObject private var store: CoreStore
    @Environment(\.snapshotMode) private var snapshotMode

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 11, weight: .medium))
                .foregroundColor(Palette.muted)
            if snapshotMode {
                Text(store.query.isEmpty ? "Find" : store.query)
                    .font(Typography.small)
                    .foregroundColor(store.query.isEmpty ? Palette.muted : Palette.paper)
                Spacer(minLength: 0)
            } else {
                TextField("Find", text: Binding(get: { store.query }, set: { store.setQuery($0) }))
                    .textFieldStyle(.plain)
                    .font(Typography.small)
                    .foregroundColor(Palette.paper)
                    .accessibilityLabel("Find a document or handle")
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 36)
        .background(RoundedRectangle(cornerRadius: 8).fill(Palette.panel))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.field, lineWidth: 1))
    }
}

/// `All` filled, the rest outlined, 22 tall, 11 pt.
struct ChipRow: View {
    let chips: [ChipModel]
    let onPick: (String?) -> Void

    var body: some View {
        HStack(spacing: 6) {
            ForEach(chips) { chip in
                Button { onPick(chip.tag) } label: {
                    Text(chip.title)
                        .font(Typography.archivo(11, weight: chip.isActive ? 600 : 400))
                        .foregroundColor(chip.isActive ? Palette.background : Palette.checkLabel)
                        .padding(.horizontal, 9)
                        .frame(height: 22)
                        .background(Capsule().fill(chip.isActive ? Palette.paper : Color.clear))
                        .overlay(Capsule().strokeBorder(chip.isActive ? Color.clear : Palette.field, lineWidth: 1))
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(chip.isActive ? .isSelected : [])
            }
            Spacer(minLength: 0)
        }
    }
}

/// "Your clipboard holds 🐙/w2-2025-2 · Find it".
struct ClipboardChip: View {
    let handle: String
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "doc.on.clipboard")
                .font(.system(size: 11))
                .foregroundColor(Palette.muted)
            (Text(ClipboardHandle.chipText) + Text(handle).font(Typography.mono(11)).foregroundColor(Palette.paper))
                .font(Typography.small)
                .foregroundColor(Palette.checkLabel)
                .lineLimit(1)
            Spacer(minLength: 8)
            Button(ClipboardHandle.chipButton) { store.locate(handle: handle) }
                .buttonStyle(InkButtonStyle(kind: .primary, height: 24, horizontalPadding: 10, fontSize: 11, radius: 5))
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Palette.field, lineWidth: 1))
        .contentShape(Rectangle())
        .onTapGesture { store.dismissClipboardHandle() }
        .accessibilityElement(children: .combine)
    }
}

/// `2025 · 14 FILES` with its chevron, then the rows when open.
struct YearGroupSection: View {
    let group: YearGroupModel
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        VStack(spacing: 0) {
            Button { store.toggleYear(group.id) } label: {
                HStack {
                    LabelText(group.label, color: Palette.muted)
                    Spacer(minLength: 8)
                    Chevron(open: group.isOpen)
                }
                .padding(.top, 12)
                .padding(.bottom, 6)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("\(group.label), \(group.isOpen ? "open" : "collapsed")")
            if group.isOpen {
                ForEach(group.rows) { row in
                    switch row {
                    case .doc(let doc):
                        DocRow(row: doc)
                    case .folder(let folder):
                        FolderRow(row: folder)
                    }
                }
            }
        }
    }
}

/// Points down when open, right when collapsed.
struct Chevron: View {
    let open: Bool

    var body: some View {
        Image(systemName: open ? "chevron.down" : "chevron.right")
            .font(.system(size: 9, weight: .semibold))
            .foregroundColor(Palette.muted)
            .frame(width: 12, height: 12)
            .accessibilityHidden(true)
    }
}

/// A folder's row: name and count over its handle and summary, its own
/// Copy, and the children indented 12 pt when open.
struct FolderRow: View {
    let row: FolderRowModel
    @EnvironmentObject private var store: CoreStore
    @State private var hovering = false

    var body: some View {
        VStack(spacing: 0) {
            RowSurface(id: row.id, hovering: $hovering) {
                HStack(spacing: 10) {
                    Button { store.toggleFolder(row.id) } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            HStack(spacing: 8) {
                                Chevron(open: row.isOpen)
                                Text(row.name).font(Typography.body)
                                Text(row.countLine).font(Typography.body).foregroundColor(Palette.muted)
                            }
                            .lineLimit(1)
                            HStack(spacing: 6) {
                                Text(row.handle).font(Typography.mono(11)).foregroundColor(Palette.paper)
                                Text("·").foregroundColor(Palette.muted)
                                Text(row.summary).foregroundColor(Palette.muted)
                            }
                            .font(Typography.caption)
                            .lineLimit(1)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("\(row.name), \(row.countLine), \(row.isOpen ? "open" : "collapsed")")
                    Spacer(minLength: 8)
                    RowActions(id: row.id, handle: row.handle, copyEnabled: row.copyEnabled, menu: row.menu, path: row.path, hovering: hovering)
                }
            }
            .rowMenu(id: row.id, menu: row.menu, path: row.path)
            if row.isOpen {
                ForEach(row.children) { child in
                    DocRow(row: child)
                }
            }
        }
    }
}

/// Two lines, Copy on the right; `···` on hover; details below when expanded.
struct DocRow: View {
    let row: DocRowModel
    @EnvironmentObject private var store: CoreStore
    @State private var hovering = false

    var body: some View {
        VStack(spacing: 0) {
            RowSurface(id: row.id, hovering: $hovering) {
                HStack(spacing: 10) {
                    Button { if row.state == .ready { store.toggleExpanded(row.id) } } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(row.name).font(Typography.body).lineLimit(1).truncationMode(.middle)
                            switch row.state {
                            case .ready:
                                HStack(spacing: 6) {
                                    Text(row.handle).font(Typography.mono(11)).foregroundColor(Palette.paper)
                                    Text("·").foregroundColor(Palette.muted)
                                    Text(row.detail).foregroundColor(Palette.muted)
                                }
                                .font(Typography.caption)
                                .lineLimit(1)
                            case .processing, .failed:
                                Text(row.detail).font(Typography.caption).foregroundColor(Palette.muted).lineLimit(1)
                            }
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("\(row.name), \(row.handle)")
                    Spacer(minLength: 8)
                    RowActions(id: row.id, handle: row.handle, copyEnabled: row.copyEnabled, menu: row.menu, path: row.path, hovering: hovering)
                }
                if case .processing(let mode) = row.state {
                    InkLoaderView(mode: mode).padding(.top, 8)
                }
            }
            .padding(.leading, row.indented ? 12 : 0)
            .rowMenu(id: row.id, menu: row.menu, path: row.path)
            if store.expandedID == row.id {
                DetailsPanel(lines: store.expandedDetails, path: row.path)
                    .padding(.leading, row.indented ? 12 : 0)
            }
        }
    }
}

/// Row padding, the hover fill, the separator, the pulse and the located outline.
struct RowSurface<Content: View>: View {
    let id: String
    @Binding var hovering: Bool
    @ViewBuilder let content: () -> Content
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        let located = store.pulse?.id == id || store.expandedID == id
        VStack(alignment: .leading, spacing: 0) {
            content()
        }
        .padding(.vertical, 9)
        .padding(.leading, 12)
        .padding(.trailing, located ? 12 : 0)
        .background {
            ZStack {
                if hovering {
                    RoundedRectangle(cornerRadius: 8).fill(Palette.paper.opacity(0.04)).padding(.horizontal, -8)
                }
                if let pulse = store.pulse, pulse.id == id {
                    InkPulseView(startedAt: pulse.startedAt).clipShape(RoundedRectangle(cornerRadius: 10))
                }
            }
        }
        .overlay {
            if located {
                RoundedRectangle(cornerRadius: 10).strokeBorder(Palette.cardBorder, lineWidth: 1)
            } else {
                VStack { Spacer(); Hairline(color: Palette.hairlineFaint) }
            }
        }
        .id(id)
        .onHover { hovering = $0 }
    }
}

/// Copy, and `···` while hovering.
struct RowActions: View {
    let id: String
    let handle: String
    let copyEnabled: Bool
    let menu: RowMenuModel
    let path: String
    let hovering: Bool
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        HStack(spacing: 6) {
            CopyButton(id: id, handle: handle, enabled: copyEnabled)
            if hovering {
                Menu {
                    RowMenuItems(id: id, menu: menu, path: path)
                } label: {
                    Text("···")
                        .font(.system(size: 14))
                        .foregroundColor(Palette.paper)
                        .frame(width: 26, height: 26)
                        .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(Palette.outline, lineWidth: 1))
                }
                // VERIFY: `.button` with a plain button style draws only the
                // label on macOS 15; if a bezel shows, try `.borderlessButton`.
                .menuStyle(.button)
                .buttonStyle(.plain)
                .menuIndicator(.hidden)
                .fixedSize()
                .accessibilityLabel("More")
            }
        }
    }
}

/// The six items, in the brief's order. "Forget" flips to "Forget <name>"
/// after one click and forgets on the next.
struct RowMenuItems: View {
    let id: String
    let menu: RowMenuModel
    let path: String
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        Button(menu.copyOctopus) { store.copy(handle: menu.octopusHandle, id: id) }
        Button(menu.copyAscii) { store.copy(handle: menu.asciiHandle, id: id) }
        Button(menu.showInFinder) { store.reveal(path: path) }
        Button(menu.inkAgain) { store.inkAgain(id: id) }
        Divider()
        Button(store.confirmingForgetID == id ? menu.forgetConfirm : menu.forget) { store.forget(id: id) }
    }
}

extension View {
    /// The same menu on a right-click anywhere on the row.
    func rowMenu(id: String, menu: RowMenuModel, path: String) -> some View {
        contextMenu { RowMenuItems(id: id, menu: menu, path: path) }
    }
}

/// The expanded row: label and value pairs, then Show in Finder.
struct DetailsPanel: View {
    let lines: [DetailLine]
    let path: String
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        VStack(spacing: 8) {
            ForEach(lines) { line in
                HStack {
                    Text(line.label).foregroundColor(Palette.muted)
                    Spacer(minLength: 8)
                    Text(line.value).foregroundColor(Palette.checkLabel).lineLimit(1).truncationMode(.middle)
                }
            }
            Button(DocDetailsModel.showInFinder) { store.reveal(path: path) }
                .buttonStyle(InkButtonStyle(kind: .outline(opacity: 0.3), height: 32, horizontalPadding: 12))
                .frame(maxWidth: .infinity)
                .padding(.top, 6)
        }
        .font(Typography.small)
        .padding(.top, 14)
        .padding(.horizontal, 12)
        .padding(.bottom, 10)
    }
}

/// "2 OF 30 MATCH" with Clear, then the flat list.
struct FindResults: View {
    let results: FindResultsModel
    @EnvironmentObject private var store: CoreStore

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                LabelText(results.header, color: Palette.muted)
                Spacer(minLength: 8)
                Button(FindResultsModel.clear) { store.clearFind() }
                    .buttonStyle(InkButtonStyle(kind: .link, height: 20, horizontalPadding: 6, fontSize: 11))
            }
            .padding(.top, 12)
            .padding(.bottom, 6)
            ForEach(results.rows) { hit in
                FindRow(hit: hit)
            }
        }
    }
}

/// A match: the row with the matched text underlined where it hit, and a
/// snippet line for text, folder and tag hits.
struct FindRow: View {
    let hit: FindRowModel
    @EnvironmentObject private var store: CoreStore
    @State private var hovering = false

    var body: some View {
        let row = hit.row
        VStack(spacing: 0) {
            RowSurface(id: row.id, hovering: $hovering) {
                HStack(spacing: 10) {
                    Button { store.toggleExpanded(row.id) } label: {
                        VStack(alignment: .leading, spacing: 2) {
                            underlined(row.name, when: .name).font(Typography.body).lineLimit(1).truncationMode(.middle)
                            HStack(spacing: 6) {
                                underlined(row.handle, when: .handle).font(Typography.mono(11)).foregroundColor(Palette.paper)
                                Text("·").foregroundColor(Palette.muted)
                                underlined(row.detail, when: .detail).foregroundColor(Palette.muted)
                            }
                            .font(Typography.caption)
                            .lineLimit(1)
                            if let snippet = hit.snippet {
                                underlined(snippet, when: .snippet)
                                    .font(hit.snippetIsInkedText ? Typography.mono(11) : Typography.caption)
                                    .foregroundColor(Palette.muted)
                                    .lineLimit(1)
                                    .truncationMode(.tail)
                            }
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    Spacer(minLength: 8)
                    RowActions(id: row.id, handle: row.handle, copyEnabled: row.copyEnabled, menu: row.menu, path: row.path, hovering: hovering)
                }
            }
            .rowMenu(id: row.id, menu: row.menu, path: row.path)
            if store.expandedID == row.id {
                DetailsPanel(lines: store.expandedDetails, path: row.path)
            }
        }
    }

    /// The matched characters underlined (hairline, 50% paper) in the field they hit.
    private func underlined(_ text: String, when field: FindRowModel.Highlight) -> Text {
        guard hit.highlight == field, let range = text.range(of: hit.matched, options: [.caseInsensitive, .diacriticInsensitive]) else {
            return Text(text)
        }
        let before = String(text[..<range.lowerBound])
        let match = String(text[range])
        let after = String(text[range.upperBound...])
        return Text(before)
            + Text(match).foregroundColor(Palette.paper).underline(true, color: Palette.paper.opacity(0.5))
            + Text(after)
    }
}

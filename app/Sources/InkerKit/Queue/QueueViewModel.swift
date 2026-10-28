import Foundation
import InkerCore

/// Maps backend records to what the popover shows. Pure: no AppKit, no FFI calls.
public enum QueueViewModel {
    public static let attentionFootnote = "Handle is held until you answer. Agents asking for it are told \"pending\"."

    public static func build(queue: [QueueItem], status: Status, reads: [ReadEvent], lastDrop: String?,
                             style: HandleStyle, now: Date) -> PopoverModel {
        let docs = queue.flatMap(\.children)
        let held = docs.filter { $0.state == .needsAttention }.count
        let busy = docs.contains { $0.state == .queued || $0.state == .reading || $0.state == .inking }
        let mode: PopoverMode
        if queue.isEmpty {
            mode = .empty
        } else if held > 0 {
            mode = .attention
        } else if busy {
            mode = .processing
        } else {
            mode = .ready
        }

        let groups = queue.map { group($0, mode: mode, style: style) }

        let left: FooterLeft
        var peeker = false
        switch mode {
        case .empty:
            left = .text("Nothing filed yet")
        case .attention:
            left = .text("\(held) waiting on you")
        case .processing:
            left = .text(lastDrop ?? "\(docs.filter { $0.state != .ready }.count) inking")
        case .ready:
            if let read = reads.first, let handle = HandleFormatter.render(codename: read.codename, style: style) {
                left = .lastRead(agent: read.agent, handle: handle, ago: RelativeTime.short(from: read.at, to: now))
                peeker = true
            } else {
                // Not on the boards; placeholder until Ben picks copy.
                left = .text(lastDrop ?? "All ready")
            }
        }
        let footer = FooterModel(left: left, right: status.mcpListening ? "MCP running" : "MCP not running", showsPeeker: peeker)
        return PopoverModel(mode: mode, badgeCount: held, groups: groups, footer: footer,
                            emptySubtitlePrefix: "They move into \(status.privateFolderDisplay). You get a ",
                            handlePrefix: HandleFormatter.prefix(style))
    }

    static func group(_ item: QueueItem, mode: PopoverMode, style: HandleStyle) -> GroupModel {
        let handle = HandleFormatter.render(codename: item.codename, style: style) ?? item.codename
        var rows: [RowModel] = []
        var attention: AttentionModel?
        for d in item.children {
            if d.state == .needsAttention, attention == nil, let q = d.question {
                attention = AttentionModel(docID: d.id, questionID: q.id, fileName: d.originalName, filePath: d.path, location: q.location,
                                           prompt: q.prompt, contextBefore: q.contextBefore, valueLength: Int(q.valueLength),
                                           footnote: attentionFootnote)
                continue
            }
            rows.append(row(d, mode: mode, style: style))
        }
        let summary: String
        let copyEnabled: Bool
        if item.isFolder {
            summary = SummaryLine.text(item.summary)
            copyEnabled = item.summary.ready > 0
        } else {
            let only = item.children.first
            if let only, only.state == .ready {
                summary = SummaryLine.docDetail(kind: only.kindLabel, inked: only.inked)
            } else {
                summary = only?.statusNote ?? ""
            }
            copyEnabled = only?.state == .ready
        }
        return GroupModel(id: item.id, handle: handle, summaryLine: summary, isFolder: item.isFolder,
                          copyEnabled: copyEnabled, rows: rows, attention: attention)
    }

    static func row(_ d: DocItem, mode: PopoverMode, style: HandleStyle) -> RowModel {
        let handle = HandleFormatter.render(codename: d.codename, style: style) ?? d.codename
        switch d.state {
        case .ready:
            // The attention board lists ready files compactly ("🐙/w2-acme-2025  ready").
            let detail = mode == .attention ? "ready" : SummaryLine.docDetail(kind: d.kindLabel, inked: d.inked)
            return RowModel(id: d.id, kind: .ready, title: handle, detail: detail, handle: handle, copyEnabled: true)
        case .failed:
            return RowModel(id: d.id, kind: .failed, title: d.originalName, detail: d.statusNote ?? "failed",
                            handle: handle, copyEnabled: false)
        case .queued, .reading, .inking, .needsAttention:
            let loader = InkLoaderMode(d.loader) ?? (d.state == .queued ? .queued : .read)
            return RowModel(id: d.id, kind: .processing(loader), title: d.originalName, detail: d.statusNote ?? "",
                            handle: handle, copyEnabled: false)
        }
    }
}

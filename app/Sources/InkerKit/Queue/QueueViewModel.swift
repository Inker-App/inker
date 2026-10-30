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
        let claudeLive = status.clients.contains(ChatConnection.claude)
        let mcp = status.mcpListening ? ChatConnection.footerText(clients: status.clients) : "MCP not running"
        let files = docs.count == 1 ? "1 file" : "\(docs.count) files"
        let footer = FooterModel(left: left, right: docs.isEmpty ? mcp : "\(files) · \(mcp)", showsPeeker: peeker,
                                 rightIsConnectLink: status.mcpListening && !claudeLive)
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
                let pending = HandleFormatter.render(codename: d.codename, style: style) ?? d.codename
                attention = AttentionModel(docID: d.id, questionID: q.id, fileName: d.originalName, handle: pending,
                                           filePath: d.path, location: q.location,
                                           prompt: q.prompt, contextBefore: q.contextBefore, valueLength: Int(q.valueLength),
                                           footnote: attentionFootnote, pageIndex: Int(q.pageIndex),
                                           span: q.span.map { Int($0.start)..<Int($0.end) })
                continue
            }
            rows.append(row(d, mode: mode, style: style))
        }
        let summary: String
        let copyEnabled: Bool
        var showsHeader = true
        if item.isFolder {
            summary = SummaryLine.text(item.summary)
            copyEnabled = item.summary.ready > 0
        } else {
            let only = item.children.first
            if let only, only.state == .ready {
                summary = SummaryLine.docDetail(kind: only.kindLabel, inked: only.inked)
                // The header is the row for a single ready file.
                rows.removeAll()
            } else {
                summary = only?.statusNote ?? ""
                // A held file keeps its header above the attention card; one
                // still being read or failed is only its row.
                showsHeader = attention != nil
            }
            copyEnabled = only?.state == .ready
        }
        return GroupModel(id: item.id, handle: handle, summaryLine: summary, isFolder: item.isFolder, showsHeader: showsHeader,
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
            // The type shows as soon as a template matched, so the row does
            // not reflow when it finishes (the find handoff, section 2).
            return RowModel(id: d.id, kind: .processing(loader), title: d.originalName,
                            detail: BoardViewModel.processingDetail(d), handle: handle, copyEnabled: false)
        }
    }
}

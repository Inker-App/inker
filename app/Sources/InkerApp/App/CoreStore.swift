import AppKit
import Combine
import InkerCore
import InkerKit
import os
import SwiftUI
import UniformTypeIdentifiers

/// Presents the windows the store needs (implemented by AppDelegate).
@MainActor
protocol CorePresenter: AnyObject {
    /// Open the popover, e.g. so the move-or-copy card is seen.
    func presentPopover()
    func presentOnboarding()
}

/// The popover's model: owns the CoreHandle, listens for queue changes, and
/// turns backend records into a PopoverModel (InkerKit.QueueViewModel).
@MainActor
final class CoreStore: ObservableObject {
    @Published private(set) var model: PopoverModel
    @Published private(set) var copyFeedback = CopyFeedback()
    /// A drag is over the status item or the popover.
    @Published var isDropTargeted = false
    /// A drop waiting for the move-or-copy answer; the popover shows the card.
    @Published private(set) var pendingDrop: DropBatch?

    let handle: CoreHandle
    weak var presenter: CorePresenter?
    private(set) var settings: AppSettings
    private var dropPreferences: DropPreferences
    private var lastDrop: String?
    private var listener: QueueListenerBridge?
    private let home: URL
    private let log = Logger(subsystem: "sh.inker.app", category: "store")

    init(handle: CoreHandle, defaults: KeyValueStore = UserDefaults.standard,
         home: URL = FileManager.default.homeDirectoryForCurrentUser) {
        let initialSettings = AppSettings(store: defaults)
        self.handle = handle
        self.home = home
        settings = initialSettings
        dropPreferences = DropPreferences(store: defaults)
        model = QueueViewModel.build(queue: [], status: handle.status(), reads: [], lastDrop: nil,
                                     style: initialSettings.handleStyle, now: Date())
        // Rust fires this on its own thread; hop to the main queue, then
        // assert main-actor isolation for the compiler.
        let bridge = QueueListenerBridge { [weak self] in
            DispatchQueue.main.async {
                MainActor.assumeIsolated { self?.refresh() }
            }
        }
        listener = bridge
        handle.setListener(listener: bridge)
        refresh()
    }

    /// A store frozen in one of the four boards, for previews.
    static func preview(_ scenario: PreviewScenario) -> CoreStore {
        CoreStore(handle: CoreHandle.preview(scenario: scenario))
    }

    var privateFolder: URL {
        settings.privateFolderPath.map { URL(fileURLWithPath: $0) } ?? FolderPickerWarnings.defaultFolder(home: home)
    }

    func refresh() {
        model = QueueViewModel.build(queue: handle.queue(), status: handle.status(), reads: handle.recentReads(n: 1),
                                     lastDrop: lastDrop, style: settings.handleStyle, now: Date())
    }

    // MARK: Copy

    func copy(handle text: String, id: String) {
        let pb = NSPasteboard.general
        pb.clearContents()
        pb.setString(text, forType: .string)
        copyFeedback.markCopied(id, at: Date())
        DispatchQueue.main.asyncAfter(deadline: .now() + CopyFeedback.duration + 0.05) { [weak self] in
            // Re-render so the button flips back to "Copy".
            MainActor.assumeIsolated { self?.objectWillChange.send() }
        }
    }

    // MARK: Attention card

    func answer(questionID: String, _ answer: ReviewAnswer) {
        do {
            try handle.answer(questionId: questionID, answer: answer)
        } catch {
            log.error("answer failed: \(String(describing: error), privacy: .public)")
        }
        refresh()
    }

    /// "Open file" reveals the original in Finder (decision 5.9).
    func reveal(path: String) {
        let url = PathDisplay.expand(path, home: home)
        NSWorkspace.shared.activateFileViewerSelecting([url])
    }

    // MARK: Settings

    func setHandleStyle(_ style: HandleStyle) {
        settings.handleStyle = style
        refresh()
    }

    func setPrivateFolder(_ url: URL) {
        settings.privateFolderPath = url.path
    }

    // MARK: Drops

    /// Everything dropped on the status item or the popover lands here.
    func receive(urls: [URL]) {
        let fm = FileManager.default
        let batch = DropCollector.collect(urls, home: home, privateFolder: privateFolder,
                                          exists: { fm.fileExists(atPath: $0.path) },
                                          isDirectory: { url in
                                              var isDir: ObjCBool = false
                                              return fm.fileExists(atPath: url.path, isDirectory: &isDir) && isDir.boolValue
                                          })
        guard !batch.isEmpty else { return }
        switch DropDecision.resolve(needsMoveDecision: batch.needsMoveDecision, remembered: dropPreferences.rememberedMode) {
        case .ingest(let mode):
            ingest(batch, mode: mode)
        case .ask:
            pendingDrop = batch
            presenter?.presentPopover()
        }
    }

    /// The card's answer. `nil` cancels the drop and leaves the files where they are.
    func answerDrop(_ mode: IngestMode?, always: Bool) {
        guard let batch = pendingDrop else { return }
        pendingDrop = nil
        guard let mode else { return }
        dropPreferences.record(mode, always: always)
        ingest(batch, mode: mode)
    }

    private func ingest(_ batch: DropBatch, mode: IngestMode) {
        do {
            try handle.ingest(paths: batch.urls.map(\.path), mode: mode)
            let verb = mode == .move ? "Moved" : "Copied"
            let what = batch.count == 1 ? (batch.isSingleFolder ? "1 folder" : "1 file") : "\(batch.count) files"
            if let source = batch.sourceDisplay {
                lastDrop = "\(verb) \(what) from \(source)"
            } else {
                lastDrop = "\(verb) \(what)"
            }
        } catch {
            // Not on the boards; placeholder until Ben picks copy.
            lastDrop = "Couldn't file that drop"
            log.error("ingest failed: \(String(describing: error), privacy: .public)")
        }
        refresh()
    }

    /// SwiftUI `.onDrop` on the popover.
    func acceptDrop(_ providers: [NSItemProvider]) -> Bool {
        let fileProviders = providers.filter { $0.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) }
        guard !fileProviders.isEmpty else { return false }
        let group = DispatchGroup()
        let collector = URLCollector()
        for provider in fileProviders {
            group.enter()
            provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
                // VERIFY: file URLs arrive as Data (a URL's dataRepresentation)
                // on current macOS; older paths delivered URL or NSURL.
                if let data = item as? Data, let url = URL(dataRepresentation: data, relativeTo: nil) {
                    collector.append(url)
                } else if let url = item as? URL {
                    collector.append(url)
                }
                group.leave()
            }
        }
        group.notify(queue: .main) { [weak self] in
            MainActor.assumeIsolated { self?.receive(urls: collector.urls) }
        }
        return true
    }

    // MARK: Agents

    /// The `inker-cli` the release puts next to the app executable.
    var cliPath: String { AgentConfigSnippet.cliPath(executable: Bundle.main.executableURL) }

    /// Write (or, dry, plan) one client's configuration with the core's writers.
    func setupAgent(_ agent: AgentKind, dryRun: Bool = false) throws -> Plan {
        try handle.setupAgent(agent: agent, dryRun: dryRun, inkerBin: cliPath, home: home.path)
    }
}

/// Thread-safe URL accumulator for NSItemProvider callbacks.
private final class URLCollector: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: [URL] = []

    func append(_ url: URL) {
        lock.lock()
        storage.append(url)
        lock.unlock()
    }

    var urls: [URL] {
        lock.lock()
        defer { lock.unlock() }
        return storage
    }
}

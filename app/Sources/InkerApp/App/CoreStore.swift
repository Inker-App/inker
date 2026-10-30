import AppKit
import Combine
import InkerCore
import InkerKit
import os
import SwiftUI
import UniformTypeIdentifiers

/// Opens the popover when the store needs to be seen (implemented by AppDelegate).
@MainActor
protocol CorePresenter: AnyObject {
    /// Open the popover, e.g. so the move-or-copy card is seen.
    func presentPopover()
    /// Keep the popover open while a system panel (the folder chooser) is up.
    func setPopoverPinned(_ pinned: Bool)
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
    /// Which screen the popover shows: a board, an onboarding step, settings.
    @Published private(set) var flow: ScreenFlow

    // The ready board at scale (find handoff). `board` is rebuilt with the
    // queue; `results` replaces the groups while a query is typed.
    @Published private(set) var board: BoardModel = .empty
    @Published private(set) var results: FindResultsModel?
    @Published private(set) var query = ""
    @Published private(set) var activeTag: String?
    /// The row whose details are open, and the one the ink pulse plays behind.
    @Published private(set) var expandedID: String?
    @Published private(set) var expandedDetails: [DetailLine] = []
    @Published private(set) var pulse: (id: String, startedAt: Date)?
    /// Set when the popover opened with a held handle on the pasteboard.
    @Published private(set) var clipboardHandle: String?
    /// The core's status as of the last refresh: the socket and who is connected.
    @Published private(set) var status = Status(mcpListening: false, privateFolderDisplay: "", clients: [])
    /// Whether INKER is in the Claude app's config, read when Settings opens.
    @Published private(set) var claudeReport: AgentReport?
    /// The attention cards' page crops by question id, once rendered.
    @Published private(set) var snippets: [String: PageSnippet.Rendered] = [:]
    private var snippetAttempts: Set<String> = []
    /// The row whose menu panel is up.
    @Published private(set) var openMenuID: String?
    /// "Forget this file" clicked once; the panel asks "Are you sure?".
    @Published private(set) var confirmingForgetID: String?
    /// The list scrolls to this row when it changes.
    @Published private(set) var scrollTarget: String?

    let handle: CoreHandle
    weak var presenter: CorePresenter?
    private(set) var settings: AppSettings
    private var dropPreferences: DropPreferences
    private var yearMemory: YearGroupMemory
    private var openFolders: Set<String> = []
    private var findGeneration = 0
    /// Previews and snapshots run Find and details on the calling thread:
    /// the preview backend has no lock to wait on, and a snapshot renders at once.
    var worksSynchronously = false
    private var lastDrop: String?
    private var listener: QueueListenerBridge?
    private let home: URL
    private let log = Logger(subsystem: "sh.inker.app", category: "store")

    /// `forceOnboarding` is the `INKER_ONBOARDING=1` override.
    init(handle: CoreHandle, defaults: KeyValueStore = UserDefaults.standard,
         home: URL = FileManager.default.homeDirectoryForCurrentUser, forceOnboarding: Bool = false) {
        let initialSettings = AppSettings(store: defaults)
        self.handle = handle
        self.home = home
        settings = initialSettings
        dropPreferences = DropPreferences(store: defaults)
        yearMemory = YearGroupMemory(store: defaults)
        flow = ScreenFlow(onboarding: OnboardingState(store: defaults), forceOnboarding: forceOnboarding)
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

    /// A store frozen in one of the four boards, for previews and snapshots.
    static func preview(_ scenario: PreviewScenario, screen: PopoverScreen = .board) -> CoreStore {
        let store = CoreStore(handle: CoreHandle.preview(scenario: scenario))
        store.worksSynchronously = true
        store.show(screen)
        return store
    }

    // MARK: Screens

    var screen: PopoverScreen { flow.screen }

    /// The screen, or the drop card in front of it.
    var surface: PopoverSurface { flow.visible(dropPending: pendingDrop != nil) }

    func nextStep() { flow.next() }
    func previousStep() { flow.back() }
    func finishOnboarding() { flow.finishOnboarding() }
    func toggleSettings() {
        flow.toggleSettings()
        if flow.screen == .settings { refreshClaudeReport() }
    }

    func openSettings() {
        flow.openSettings()
        refreshClaudeReport()
    }

    func closeSettings() { flow.closeSettings() }
    func runSetupAgain() { flow.runSetupAgain() }
    func show(_ screen: PopoverScreen) { flow.show(screen) }

    /// Keep the popover up while a system panel runs, then bring it back.
    func withPopoverPinned<T>(_ body: () -> T) -> T {
        presenter?.setPopoverPinned(true)
        defer {
            presenter?.setPopoverPinned(false)
            presenter?.presentPopover()
        }
        return body()
    }

    var privateFolder: URL {
        PrivateFolderSection.folder(stored: settings.privateFolderPath, home: home)
    }

    var privateFolderDisplay: String {
        PrivateFolderSection.display(stored: settings.privateFolderPath, home: home)
    }

    func refresh() {
        status = handle.status()
        model = QueueViewModel.build(queue: handle.queue(), status: status, reads: handle.recentReads(n: 1),
                                     lastDrop: lastDrop, style: settings.handleStyle, now: Date())
        rebuildBoard()
        requestSnippets()
    }

    /// Settings' Chat apps row: live session first, then the config.
    var claudeConnection: ChatConnection.State {
        ChatConnection.state(registered: claudeReport?.registered ?? false, clients: status.clients)
    }

    func refreshClaudeReport() {
        claudeReport = handle.agentReport(agent: .claudeDesktop, inkerBin: cliPath, home: home.path)
    }

    /// One render per attention card, off the main thread (Vision blocks
    /// its thread, so a GCD thread rather than a Task). A card whose crop
    /// cannot be made keeps the octopus; answered cards drop their crop.
    private func requestSnippets() {
        let cards = model.groups.compactMap(\.attention)
        let live = Set(cards.map(\.questionID))
        snippets = snippets.filter { live.contains($0.key) }
        snippetAttempts.formIntersection(live)
        for card in cards where !snippetAttempts.contains(card.questionID) {
            guard let span = card.span else { continue }
            snippetAttempts.insert(card.questionID)
            let id = card.questionID
            let path = card.filePath
            let page = card.pageIndex
            if worksSynchronously {
                if let rendered = PageSnippet.render(path: path, pageIndex: page, span: span) { snippets[id] = rendered }
                continue
            }
            DispatchQueue.global(qos: .userInitiated).async { [weak self] in
                guard let rendered = PageSnippet.render(path: path, pageIndex: page, span: span) else { return }
                DispatchQueue.main.async {
                    MainActor.assumeIsolated { self?.snippets[id] = rendered }
                }
            }
        }
    }

    // MARK: The ready board

    private func rebuildBoard() {
        let raw = handle.board(tag: activeTag)
        let newest = raw.groups.first.map { BoardViewModel.yearID($0.year) }
        let memory = yearMemory
        let folders = openFolders
        board = BoardViewModel.build(board: raw, style: settings.handleStyle, showPayer: settings.showsPayer,
                                     activeTag: activeTag, isYearOpen: { memory.isOpen($0, newest: newest) },
                                     isFolderOpen: { folders.contains($0) })
        if results != nil { runFind() }
        if let id = expandedID { loadDetails(id) }
    }

    func setTag(_ tag: String?) {
        activeTag = tag
        rebuildBoard()
    }

    func toggleYear(_ id: String) {
        let open = board.groups.first { $0.id == id }?.isOpen ?? false
        yearMemory.set(id, open: !open)
        rebuildBoard()
    }

    func toggleFolder(_ id: String) {
        if openFolders.contains(id) { openFolders.remove(id) } else { openFolders.insert(id) }
        rebuildBoard()
    }

    /// The Find field. Matching runs off the main thread: on the real core it
    /// takes the core's lock, which a document being inked can hold for seconds.
    func setQuery(_ text: String) {
        query = text
        if text.trimmingCharacters(in: .whitespaces).isEmpty {
            results = nil
            return
        }
        runFind()
    }

    func clearFind() {
        setQuery("")
    }

    private func runFind() {
        findGeneration += 1
        let generation = findGeneration
        let text = query
        let total = board.total
        let style = settings.handleStyle
        let showPayer = settings.showsPayer
        let handle = handle
        let apply: ([FindMatch]) -> Void = { [weak self] matches in
            guard let self, self.findGeneration == generation else { return }
            let model = FindViewModel.build(matches: matches, total: total, style: style, showPayer: showPayer)
            self.results = model
            // Typing a handle that is exactly one document locates it.
            if let id = model.exactMatchID, self.expandedID != id {
                self.expand(id, pulse: true)
            }
        }
        if worksSynchronously {
            apply(handle.find(query: text))
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            let matches = handle.find(query: text)
            DispatchQueue.main.async {
                MainActor.assumeIsolated { apply(matches) }
            }
        }
    }

    /// The footer's last-read button, the clipboard chip and a pasted handle
    /// all land here: clear the search, open the group, expand and pulse.
    func locate(handle text: String) {
        guard let entry = handle.locate(handle: text) else { return }
        clipboardHandle = nil
        query = ""
        results = nil
        activeTag = nil
        switch entry {
        case .doc(let item):
            yearMemory.set(BoardViewModel.yearID(item.year), open: true)
            if let folder = folderID(containing: item.id) { openFolders.insert(folder) }
            rebuildBoard()
            expand(item.id, pulse: true)
        case .folder(let item):
            let year = handle.board(tag: nil).groups.first { g in g.entries.contains { $0.entryID == item.id } }?.year
            yearMemory.set(BoardViewModel.yearID(year), open: true)
            openFolders.insert(item.id)
            rebuildBoard()
            expandedID = nil
            pulse = (item.id, Date())
            scrollTarget = item.id
        }
    }

    private func folderID(containing docID: String) -> String? {
        for g in handle.board(tag: nil).groups {
            for case .folder(let f) in g.entries where f.children.contains(where: { $0.id == docID }) {
                return f.id
            }
        }
        return nil
    }

    /// Open or close a row's details. Only one row is open at a time.
    func toggleExpanded(_ id: String) {
        if expandedID == id {
            expandedID = nil
            expandedDetails = []
        } else {
            expand(id, pulse: false)
        }
    }

    private func expand(_ id: String, pulse: Bool) {
        expandedID = id
        expandedDetails = []
        if pulse { self.pulse = (id, Date()) }
        scrollTarget = id
        loadDetails(id)
    }

    private func loadDetails(_ id: String) {
        let handle = handle
        let display = privateFolderDisplay
        let apply: (DocDetails?) -> Void = { [weak self] details in
            guard let self, self.expandedID == id, let details else { return }
            self.expandedDetails = DocDetailsModel.lines(details, privateFolderDisplay: display, now: Date())
        }
        if worksSynchronously {
            apply(try? handle.details(id: id))
            return
        }
        DispatchQueue.global(qos: .userInitiated).async {
            let details = try? handle.details(id: id)
            DispatchQueue.main.async {
                MainActor.assumeIsolated { apply(details) }
            }
        }
    }

    func dismissClipboardHandle() {
        clipboardHandle = nil
    }

    // MARK: The row menu

    /// `···` or a right-click: the panel opens on this row, or closes if it
    /// is already up there. Opening elsewhere drops a pending "Are you sure?".
    func toggleMenu(id: String) {
        if openMenuID == id { closeMenu() } else { openMenu(id: id) }
    }

    func openMenu(id: String) {
        if openMenuID != id { confirmingForgetID = nil }
        openMenuID = id
    }

    /// Previews and snapshots: the menu on the row a handle names.
    func openMenu(handle text: String) {
        guard let entry = handle.locate(handle: text) else { return }
        openMenu(id: entry.entryID)
    }

    func closeMenu() {
        openMenuID = nil
        confirmingForgetID = nil
    }

    func menuTarget(for id: String) -> RowMenuTarget? {
        results?.menuTarget(for: id) ?? board.menuTarget(for: id)
    }

    /// A click on a panel item. Everything closes the panel except the
    /// first click on Forget, which turns it into "Are you sure?".
    func perform(_ action: RowMenuItem.Action, id: String, target: RowMenuTarget) {
        switch action {
        case .copyOctopus:
            copy(handle: target.menu.octopusHandle, id: id)
            closeMenu()
        case .copyAscii:
            copy(handle: target.menu.asciiHandle, id: id)
            closeMenu()
        case .showInFinder:
            reveal(path: target.path)
            closeMenu()
        case .inkAgain:
            closeMenu()
            inkAgain(id: id)
        case .forget:
            forget(id: id)
        }
    }

    /// "Ink again": the row goes back to queued with the same handle.
    func inkAgain(id: String) {
        do {
            try handle.inkAgain(id: id)
        } catch {
            log.error("ink again failed: \(String(describing: error), privacy: .public)")
        }
        refresh()
    }

    /// "Forget this file" asks once: the item turns into a red "Are you
    /// sure?" and the next click does it. Closing the panel resets.
    func forget(id: String) {
        guard confirmingForgetID == id else {
            confirmingForgetID = id
            return
        }
        closeMenu()
        if expandedID == id {
            expandedID = nil
            expandedDetails = []
        }
        do {
            try handle.forget(id: id)
        } catch {
            log.error("forget failed: \(String(describing: error), privacy: .public)")
        }
        refresh()
    }

    // MARK: Popover lifetime

    /// The one moment the pasteboard is read: when the popover opens, and
    /// only if the setting allows. Anything but a held handle is ignored.
    func popoverOpened() {
        closeMenu()
        guard settings.offersClipboardHandle else { return }
        let text = NSPasteboard.general.string(forType: .string)
        guard let candidate = ClipboardHandle.parse(text), handle.locate(handle: candidate) != nil else {
            clipboardHandle = nil
            return
        }
        clipboardHandle = candidate
    }

    func popoverClosed() {
        clipboardHandle = nil
        closeMenu()
        pulse = nil
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

    /// "Open in Finder" in Settings shows the private folder's contents. The
    /// host creates the folder when the core opens, so it exists by now; if
    /// it has gone missing, select its parent instead of doing nothing.
    func openPrivateFolder() {
        let url = privateFolder
        if FileManager.default.fileExists(atPath: url.path) {
            NSWorkspace.shared.open(url)
        } else {
            log.error("private folder missing; revealing its parent")
            NSWorkspace.shared.activateFileViewerSelecting([url])
        }
    }

    // MARK: Settings

    func setHandleStyle(_ style: HandleStyle) {
        settings.handleStyle = style
        refresh()
    }

    func setShowsPayer(_ on: Bool) {
        settings.showsPayer = on
        rebuildBoard()
    }

    func setOffersClipboardHandle(_ on: Bool) {
        settings.offersClipboardHandle = on
        if !on { clipboardHandle = nil }
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

    /// Show the card for a batch without a drop (snapshots and previews).
    func presentDropCard(_ batch: DropBatch) {
        pendingDrop = batch
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

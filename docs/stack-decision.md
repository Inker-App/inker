# INKER: stack decision and architecture

Date: 2026-10-28. Decision by Ben: **Rust core, Swift shell.** This document records what INKER is, why that stack, how the pieces fit, what the research found, and what is still open. Source brief: [`handoff/HANDOFF.md`](handoff/HANDOFF.md). Design boards: [`handoff/design/`](handoff/design/) (App v2 page is the build target).

Facts below marked *(verified)* were checked against vendor docs or source on 2026-10-28. Facts marked *(unverified)* come from secondary sources or inference and need a hands-on check.

---

## 1. What INKER is

> **Reframed 2026-10-28, after the first build.** Ben: INKER is not for coding agents. It is for the chat window: people using the Claude or ChatGPT desktop app first, and ideally later the browser apps too, through MCP. The brief below and the build to date were framed around Claude Code, Cursor and Codex. The core, the vault, the pipeline and the MCP server carry over unchanged; the onboarding targets, the "keep agents out" layer, the instructions wording and the website copy change. Section 9 (pending research) records the client-by-client facts and the plan.

INKER is a macOS menu bar app plus a local MCP server. You drop a sensitive document (taxes first) on the octopus in the menu bar instead of into your agent's chat. INKER moves it into a private folder outside any agent workspace, extracts the text, finds the identifying values, and replaces them with stable tokens like `[SSN_1]` and `[EMPLOYER_1]` while keeping amounts, box numbers, dates and the state. You get a handle like `🐙/w2-acme-2025` on your clipboard. The agent pastes it, calls INKER's MCP tools, and reasons over the tokenized text. Anything the agent writes back is rehydrated locally into the private folder. The agent, the model and the provider never see the real values.

The brand is a small world: nosy red-irised eyeballs (models, logs, servers), a calm octopus (you), and ink made of dots that pools to hide and disperses to move. Everything is 3D rendered down to a halftone dot grid, the UI is all dark, and big motion happens only on state changes.

---

## 2. The decision

**Rust owns everything that is logic.** Vault, tokens, handles, document state machine, detection rules, form templates, model orchestration, file moves, the MCP server, and a headless CLI. It builds and tests on Linux.

**Swift owns only what macOS insists on.** The status item drop target, the popover, the dot renderers, Keychain, PDFKit, Vision OCR, ImageIO. It is a thin shell around the Rust crate, bridged through UniFFI.

### Why this and not the alternatives

| | Swift only | Tauri 2 | Electron | **Rust core + Swift shell** |
|---|---|---|---|---|
| Drop on the menu bar icon | AppKit `NSStatusItem` works; SwiftUI `MenuBarExtra` cannot *(verified)* | Unsupported since 2023 (tauri#7894); only a 0-star plugin or hand-written objc2 *(verified)* | Supported, `drop-files` event *(verified)*; untested on macOS 26 | Same as Swift only |
| Popover | Real `NSPopover` | Undecorated window, hand-rolled positioning with open multi-monitor bugs | Frameless window imitation | Real `NSPopover` |
| Idle footprint | Smallest | ~50 MB WebKit processes plus app *(unverified)* | 150-300 MB resident *(unverified)* | Smallest |
| Renderers | Port to Metal / SwiftUI shaders | JS runs unchanged in WKWebView | JS runs unchanged | Port to Metal / SwiftUI shaders |
| MCP SDK | swift-sdk 0.12.1, pre-1.0, 73 open issues *(verified)* | rmcp 3.8.0, weekly releases, spec 2026-07-28 *(verified)* | TS SDK v2.4.0, stable *(verified)* | rmcp |
| PII model runtime | Core ML (silent conversion errors reported) or ONNX | `ort` 2.0 release candidate, or `gline-rs` | onnxruntime-node, 307 MB unpruned | `ort` / `gline-rs`, or MLX on the Swift side |
| Where contributions land | All on a Mac with Xcode | Rust core on any OS, but the hard parts are unsafe objc2 | Broad pool, but Vision/HEIC/Keychain still need a native helper | Rust core on any OS; Swift shell stays small |
| Distribution | Developer ID, non-sandboxed | Developer ID; `macOSPrivateApi` for transparency | Developer ID; 250-400 MB app | Developer ID, non-sandboxed |

The deciding points:

1. **The core gesture is a drop on the icon.** Only AppKit does that reliably. Tauri's gap is a three-year-old open request. That alone removes Tauri from the table for the shell.
2. **Performance for this app means invisible when idle.** Native shell plus a Rust core is the smallest resident footprint of the four. The on-drop cost is library-bound (Vision, ONNX) and identical across stacks.
3. **Rust is where the product lives.** Detection, templates, tokens and the MCP server are platform-independent and are what contributors will fork. Linux CI covers them. Only the shell needs a Mac.
4. **rmcp is the most mature MCP SDK of the three** and already tracks the current spec revision.
5. **Sandboxing is out anyway.** A stdio MCP binary spawned by another process cannot inherit an App Sandbox *(verified, Apple forums 706390)*, so the App Store was never an option. Developer ID plus notarization it is.

---

## 3. Architecture

### 3.1 Repository layout

```
inker/
  core/                      Cargo workspace (builds on Linux and macOS)
    crates/
      inker-core/            vault, tokens, handles, codenames, document model, state machine
      inker-extract/         text sources: PDF text layer, AcroForm fields, OCR (trait + fallbacks)
      inker-detect/          rules, checksums, form templates, model runner, treatment (tokenize / generalize / keep)
      inker-packs/           loads packs; packs/tax is the first
      inker-mcp/             rmcp server: list, resolve, describe, write; instructions; dual-era
      inker-ipc/             unix-socket server hosted by the app, JSON-RPC framing
      inker-ffi/             UniFFI bindings for the Swift shell
    bins/
      inker/                 CLI: ingest, list, resolve, serve, mcp (stdio shim), doctor
  app/                       Xcode project: INKER.app (AppKit shell + SwiftUI views + Metal renderers)
  packs/
    tax/                     form templates as data (TOML), codename rules, generalization rules
  samples/                   synthetic documents only; see 3.9
  docs/
    handoff/                 the brief and design boards
    stack-decision.md        this file
  tools/                     Ink Lab and Asset Studio, later
```

### 3.2 Process model: one process holds the vault

Research settled the brief's open transport question. *(All verified against the MCP spec and vendor docs.)*

- **INKER.app hosts the core.** It is the only process that ever holds the Keychain key, the decrypted vault, the token-minting authority and the review queue. One minting authority is what makes tokens stable across every agent and session; N stdio processes would be N writers racing on one vault file.
- **The app listens on a unix domain socket** with `0600` permissions in its Application Support directory. The MCP spec's security guidance names unix sockets with restricted access as the local IPC option. No port discovery, no `Origin` checks, no DNS-rebinding class of bugs (which hit the MCP Inspector, the TypeScript SDK and the Java SDK as CVEs in 2025-2026).
- **Every agent launches `inker mcp` over stdio.** It is a thin shim in the same Rust binary that forwards JSON-RPC to the socket. All three target clients speak stdio. If the app is not running the shim runs `open -b` on the bundle, waits for the socket, and otherwise returns a clear error. (No shipping app auto-launches today; Xcode, Figma and JetBrains all just fail. We can do better cheaply.)
- **Optional later:** Streamable HTTP on `127.0.0.1` with a random bearer token and strict `Origin` validation, for clients that prefer a URL. Not in v1. Codex has an open bug with local HTTP behind a system proxy.
- **Dual-era protocol.** The current spec revision 2026-07-28 is stateless: no `initialize`, no session header, `instructions` moves into `server/discover`, and state is carried by server-minted handles passed as tool arguments. That is exactly what `🐙/` handles are. Claude's support is "rolling out", so the server speaks both 2025-11-25 and 2026-07-28. rmcp 3.8 implements the new one.

### 3.3 The OS boundary

The core defines traits; the shell implements them with Apple frameworks; the CLI implements them with pure Rust so the pipeline tests run on Linux.

| Capability | Trait in core | macOS implementation (Swift, via UniFFI callback) | Linux / CI implementation (Rust) |
|---|---|---|---|
| PDF text layer | `TextSource` | PDFKit `PDFPage.string` | `pdf-extract` |
| AcroForm fields | `FormSource` | PDFKit widget annotations (`fieldName`, `widgetStringValue`) | `lopdf` raw dictionary |
| OCR | `OcrSource` | Vision `RecognizeDocumentsRequest` (macOS 26) with `VNRecognizeTextRequest` fallback | `ocrs` (pure Rust, for tests only) |
| HEIC decode | inside `OcrSource` | ImageIO | not needed in CI |
| Key storage | `KeyStore` | Keychain via Security framework | file-backed test key |
| File move | core, `std::fs` | same; TCC prompt for ~/Downloads on first use | same |

Vision has no published accuracy numbers on tax forms *(verified: none found)*. The product uses it; the test corpus pins behaviour with a fixture of OCR output so core tests do not depend on Vision.

### 3.4 Vault and index

- **Vault:** value-to-token map, AES-GCM, key in the Keychain as a `SymmetricKey`. Tokens are typed and numbered on first sight (`SSN_1`, `EIN_1`, `EMPLOYER_1`, `ACCOUNT_1`). Numbering is global and stable forever, per the brief.
- **Index:** everything derived from a document (tokenized text, detections, review decisions, codename, state) lives in a SQLite store in Application Support, never beside the file. This honours "no mirror folder": the only thing in `~/Private` is the original file. *As built (2026-10-28):* the SQLite file itself is not encrypted (plain bundled SQLite, not SQLCipher). It holds tokenized text only, but `original_name` and `path` are plain, so a file named after a person puts that name in the index. The raw extracted text of a held document is sealed with the vault key while it waits for review and deleted once the document is Ready. Each pending question's peek context (the few words before and after the held value, digit runs blanked) is sealed the same way and cleared when answered; the value itself is never stored. Encrypting the whole index is a follow-up if Ben wants it.
- **Handles** resolve through the index. A held document returns `pending review` to agents until the user answers.

### 3.5 Detection pipeline

From the model research. Ordered by precision; each stage sees what the previous one left.

1. **Text source.** PDF text layer if present, else OCR. AcroForm fields are read directly when the PDF is a fillable IRS form.
2. **Form templates** (`packs/tax`). Anchor on fixed IRS box labels, assign typed tokens per box: employer/payer name and EIN, employee/recipient name and SSN/TIN, account numbers, address generalized to city and state. Templates are TOML data, so a tax professional can contribute one without writing Rust. Forms: W-2, 1099-NEC/INT/DIV/B/K/MISC, 1098, 1095, K-1. No open box-level W-2/1099 dataset exists *(verified)*; the templates are ours.
3. **Checksum rules**, ported as plain logic (python-stdnum is the reference, LGPL, so port rather than link): SSN area/group/serial rules including the 9xx rejection Presidio misses, EIN campus prefixes, ITIN ranges 50-65/70-88/90-92/94-99 (stdnum itself has the 50-65 gap), ABA routing 3-7-1 mod 10, Luhn.
4. **ML pass on free text** (bank statements, closing statements, receipts). Two candidates, decided by a benchmark on our own synthetic corpus:
   - **knowledgator GLiNER-PII** (Apache-2.0, labels include ssn, routing number, bank account; ONNX 197 MB uint8 / 330 MB fp16). Runs in Rust via `gline-rs` on ONNX Runtime. Keeps inference in the core and testable on Linux. Default candidate.
   - **OpenAI Privacy Filter** (`openai/privacy-filter`, Apache-2.0, released 2026-04-22; 1.5B-parameter sparse mixture-of-experts token classifier, 8 labels only, no SSN/EIN/organization label; ~0.9 GB at q4). Scores 0.96 F1 on its own benchmark but ~0.38 on the independent SPY set, roughly tied with GLiNER. Core ML runs the experts densely (336 ms vs 7.6 ms per layer on MLX), so the only sane Mac runtime is MLX on the Swift side. Candidate for a second pass, not the first.
   - **Presidio** is now community-governed (`data-privacy-stack/presidio`), Python-only, has no EIN or ABA recognizer, and would mean bundling a Python runtime. We copy its recognizer ideas and skip the library.
   - **Avoid:** Piiranha, the Isotonic deberta model and the AI4Privacy datasets, all non-commercial licenses.
5. **Fail closed.** Any leftover run of 9+ digits, or an unresolved proper name near a sensitive label, is masked and raised as a "needs attention" item. The attention card in the design is this stage talking.
6. **Unplaced values on form pages** *(added 2026-10-28)*. On pages where a template matched, anything left that is a capitalised 2-4 word run, an alphanumeric code, or a 6+ digit run, and is not a label, boilerplate (`packs/tax/boilerplate.txt`), amount, date or form number, is tokenized when its shape is unambiguous (street line, state ID, labelled account code) and otherwise held with one question per distinct value. This closes the IRS "labels first, values after" layout without PDF coordinates, at a budget of at most 3 questions per form.

Three treatments per detection, from the brief: tokenize, generalize, keep. Treatment is decided by type and template, overridable per document by the user's answer on the attention card.

### 3.6 MCP surface

Tools, as the brief lists them: `list`, `resolve`, `describe`, `write`. Annotations: `list`/`resolve`/`describe` are `readOnlyHint`; `write` is not. `resolve` is marked `_meta["anthropic/alwaysLoad"]` so Claude Code's tool search never hides it.

Server `instructions` open with the one sentence that matters: strings starting with `🐙/` are private document references; call `resolve`. Claude Code loads server instructions at session start even with tool search on (truncated at 4,096 characters) *(verified)*. Codex honours them too ("first 512 characters matter most") *(verified)*. Cursor's docs do not mention them *(verified: undocumented)*. The brief's step 3 spike still runs, because reports say Claude Code injects instructions as a user-turn delta and discovery of deferred servers is weak.

*CLI-only tools (added 2026-10-28).* The socket also serves `inker/review`, `inker/answer` and `inker/status`, so a held document can be released without the Swift app (`inker review`, `inker answer <id> ink|leave`) and `inker doctor` reports the folder the running server serves. They are never in `tools/list`, and `tools/call` serves them only on a connection whose `initialize` client name is `inker-cli` (per-request `_meta` client info does not count); to anyone else they are an unknown tool. That is a hiding rule, not authentication: the socket is the user's own (0600), and an agent's harness, not the agent, picks its client name. Because an agent with a shell could still run `inker answer … leave` and then resolve the value, `inker guard` denies any shell command that runs `inker answer`.

`write` takes tokenized content from the agent, rehydrates it, and saves it only into the private folder. It returns a confirmation and a new handle for the output file; the agent can `resolve` that handle for the tokenized view (section 5.6).

### 3.7 The shell

- **AppKit `NSStatusItem` + `NSPopover`** hosting SwiftUI through `NSHostingController`. Not `MenuBarExtra`: it cannot take drops on its label and has no first-party presentation state *(verified)*. A shipping open-source app (prof18/slashgrab) does exactly this pattern with a drop subview over `statusItem.button`. One report says the subview did not receive drags while the button itself did; test both on macOS 14, 15 and 26.
- **Popover** is `.applicationDefined` while a drag is in progress, `.transient` otherwise. Fixed `contentSize` 360x500.
- **Everything lives in the popover.** Onboarding's three steps, settings and the move-or-copy card are screens of the same 360x500 popover (`ScreenFlow` in InkerKit), and first launch opens the popover by itself. INKER opens no windows of its own. Ben's call on 2026-10-28: the separate setup window cut off its primary button, and these screens sit better in the menu bar app anyway. The window classes and the Cmd+Tab activation-policy switch (`WindowPresence`, section 8) are gone.
- **Badge** is `button.title` with `imagePosition = .imageLeft`.
- **Renderers.** The eyeball shader and the raymarched octopus go into Metal fragment shaders or SwiftUI `.layerEffect` shaders (macOS 14+). The ink grid (circles plus bridges plus 2x2 fills) is cheap enough for SwiftUI `Canvas`. No benchmark exists for `Canvas` drawing thousands of circles per frame *(verified: none found)*, and `TimelineView` has pinned CPU at 100% in one Apple forums report, so the first renderer milestone is an Instruments profile, not a pretty picture. Budget: the octopus renders at 12 fps by design; everything else barely breathes at rest; all rendering pauses on `popoverDidClose`; Reduce Motion (`NSWorkspace.shared.accessibilityDisplayShouldReduceMotion`) freezes to a still frame. The JS in the design boards is the behavioural spec and the tuning values.
- **macOS 26 Tahoe notes:** menu bar is 30 pt, right-clicks at the top edge no longer fire, users can hide the item under "Allow in the Menu Bar" and the app cannot read that setting.

### 3.8 Agent integration (what onboarding actually writes)

The mockup's "Keep these agents out" list is partly wrong. Corrected, from the vendor docs *(verified unless noted)*:

| Agent | MCP registration | Keeping it out of `~/Private` | Teaching `🐙/` |
|---|---|---|---|
| Claude Code | `claude mcp add --scope user` (writes `~/.claude.json`, **not** settings.json; a running Claude Code can revert direct edits to that file) | `permissions.deny: ["Read(~/Private/**)", "Edit(~/Private/**)"]` in `~/.claude/settings.json`. This covers Read/Edit/Write and recognised Bash file commands only. Add `sandbox.filesystem.denyRead: ["~/Private"]` for OS-level enforcement of Bash. A PreToolUse hook is a third layer. | Server `instructions`, plus one line in `~/.claude/CLAUDE.md` and `AGENTS.md` |
| Cursor | `~/.cursor/mcp.json` (or the documented `cursor://` install deeplink) | **No deny-rule file exists.** `.cursorignore` is per-workspace and does not stop terminal reads. Real levers: `~/.cursor/hooks.json` with `beforeReadFile` and `beforeShellExecution`, and `~/.cursor/sandbox.json` `readBoundary: "workspace"` (a whole-workspace boundary, not per-path). | Project `AGENTS.md` (decided). There is no global rules file INKER can write; `.cursorrules` is legacy and gone from the docs. |
| Codex CLI | `codex mcp add` or `[mcp_servers.inker]` in `~/.codex/config.toml` | `sandbox_mode` does not restrict reads. A permission profile does: `[permissions.inker.filesystem]` with `"~/Private" = "deny"`, but setting `default_permissions` replaces the user's `sandbox_mode` *(the interaction is unverified)*. Hooks in `~/.codex/hooks.json` are a guardrail on top. | Server `instructions`, plus `~/.codex/AGENTS.md` |

The brief's own line holds: the deny rule is a polite request; the folder being outside the workspace does the real work.

Other leak paths the onboarding should handle or warn about: Spotlight (`mdfind` from Bash returns content; exclude via Search Privacy or a `.noindex` suffix), iCloud Desktop & Documents sync (never place the folder under ~/Desktop or ~/Documents), Time Machine (`tmutil addexclusion`), the Downloads quarantine record, and shell history.

### 3.9 Safety rails in the repo

- `samples/` holds synthetic documents only, generated by a script, with values from the IRS's own invalid ranges (area 000/666/9xx SSNs, invalid EIN prefixes).
- A pre-commit hook rejects any SSN-shaped, EIN-shaped or routing-shaped string outside `samples/` that passes the real checksums, and rejects any PDF or image outside `samples/`.
- Core tests run on Linux against the fixture corpus; a macOS job builds the shell and runs the Vision OCR fixture refresh on demand.

---

## 4. Key facts the decision rests on

- Tauri tray drops: open feature request tauri-apps/tauri#7894 since 2023-09, no maintainer response. Workaround plugin `tauri-plugin-menubar-dnd` has 0 stars.
- Electron Tray `drop-files` is documented and current (Electron 45.1.0, 2026-10-27); idle Electron is 150-300 MB resident by blog measurements; Vision OCR, HEIC and Keychain still need a native helper.
- SwiftUI `MenuBarExtra` has no drop support and no presentation API; AppKit `NSStatusItem` drop works (prof18/slashgrab, last push 2026-09).
- Official MCP SDKs: swift-sdk 0.12.1 (pre-1.0), rmcp 3.8.0 (weekly, spec 2026-07-28), TypeScript v2.4.0 (stable).
- MCP spec 2026-07-28 is stateless; `instructions` lives in `server/discover`; local servers should use stdio or unix sockets; HTTP servers must validate `Origin`.
- A stdio helper cannot inherit an App Sandbox from a non-sandboxed parent (Apple forums 706390), so Developer ID, non-sandboxed.
- Claude Code loads server `instructions` at session start, truncated at 4,096 chars; `_meta["anthropic/alwaysLoad"]` forces a tool to load.
- OpenAI Privacy Filter: Apache-2.0, 1.5B sparse MoE, 8 labels, no SSN/EIN label, ~0.9 GB q4, Core ML unsuitable, MLX fine.
- Presidio: community-governed since 2026, Python only, no EIN/ABA recognizers, SSN recognizer does not reject 9xx areas.
- knowledgator GLiNER-PII: Apache-2.0, ssn/routing/account labels, ONNX 197-330 MB, runs via `gline-rs`.
- python-stdnum is the best checksum reference; ITIN ranges in it are incomplete.
- `ort` is still a 2.0 release candidate (rc.15, 2026-10-19). Pin it.
- Vision `RecognizeDocumentsRequest` needs macOS 26; `VNRecognizeTextRequest` is the fallback on 14/15. No tax-form accuracy benchmark exists.

---

## 5. Gaps in the brief, and the decisions made

Ben answered on 2026-10-28: all dark, AGENTS.md for the teaching line, and best guess on the rest. The best guesses are mine and are recorded here so they can be overturned.

**Decided by Ben**

1. **All dark.** The World Bible line "the app lives on paper" is stale. Paper is the dot colour, not a background.
2. **Iris colour is always red** (`#E0473E`), the only colour in the product. The paper-iris tweak on `WorldEyes` is exploration only.
3. **Agent exclusion per agent.** Write the strongest thing each agent offers and say which agent got what (section 3.8): Claude Code gets a permission rule, the sandbox `denyRead` and a hook; Codex gets a permission profile, with onboarding copy saying it replaces the user's sandbox mode; Cursor gets a `beforeReadFile` hook. `inker doctor` reports what is in place. The `AppOnboard1` config paths and the words "deny rule" change accordingly; the screen copy is Ben's.
4. **Teaching line goes in `AGENTS.md`** (global `~/.codex/AGENTS.md` for Codex, project `AGENTS.md` for Cursor, and Claude Code reads it too), plus `~/.claude/CLAUDE.md`. `.cursorrules` is dropped.

**Best guesses (mine, overturnable)**

5. **Folder handles.** `resolve` on a folder handle returns a listing of child handles with type, state and inked count, not concatenated text. Ready children resolve normally; a held child shows `pending review` in the listing. The folder's Copy button copies the folder handle whenever at least one child is ready.
6. **`write` returns** a confirmation and a new handle for the output file (for example `🐙/w2-acme-2025-summary`). The agent can `resolve` that handle to see the tokenized view of what it saved. Real values never come back.
7. **Codename collisions** get a numeric suffix: `w2-acme-2025`, `w2-acme-2025-2`. No renaming UI in v1. *Update (2026-10-28):* the default policy is now `TypeYear`, so collisions read `w2-2025`, `w2-2025-2`, because a handle is pasted into agent chats in the clear and a security review found payer names and EINs in handles under `TypeOrgYear`.
8. **Tokens keep global numbering.** `describe` returns a per-document role map (`SSN_1: employee`, `EIN_1: employer`) so a joint return is readable without changing the token scheme.
9. **Attention card.** "Open file" reveals the original in Finder. "Leave it" applies to this document only; nothing is remembered globally.
10. **Move edge cases.** Cross-volume moves are copy, verify, delete. A file already inside the private folder is indexed in place. A dropped folder moves as a unit; unsupported files inside it are listed as skipped and stay where they landed.
11. **macOS floor is 26.** One OCR path (`RecognizeDocumentsRequest`) and the current SwiftUI shader APIs. Revisit if early users are on 15.
12. **Spotlight.** Do not rename the folder. Onboarding warns and offers a one-click open of the Spotlight Privacy pane, which cannot be set programmatically. `mdfind` leaks names and existence, not contents, so this is a warning, not a wall.
13. **Handle syntax.** The server accepts both `🐙/` and `ink/` always. The setting only controls what lands on the clipboard.
14. **Footer.** "MCP running" means the socket is listening. The Ready board's last-read line stays; no connected-agent count in v1.
15. **Folder picker** defaults to `~/Private` and warns on `~/Desktop`, `~/Documents` (iCloud sync) and any path inside a known agent workspace.

**Raised by the build, not yet decided**

16. **Handles leak the org name.** `TypeOrgYear` (the brief's default, as built) puts the detected employer or payer in the handle (`🐙/w2-acme-2025`) while the text tokenizes it as `[ORG_1]`. `TypeYear` (`🐙/w2-2025`, `-2`, `-3`) is implemented and switchable. Folder handles are slugged from the folder name, so a folder named after a person leaks that name too. *Update (2026-10-28):* the default is now `TypeYear` and folder handles are `folder-NNNN`, never the folder name, because a review showed handles carrying a person's name (`🐙/1099nec-<first name>-2025`), an EIN and a folder name; `TypeOrgYear` remains opt-in and now takes only a template's employer, payer or lender box, with digits stripped.
17. **Index encryption.** See section 3.4. The index is plain SQLite as built.

## 6. Risks

1. **Drop-on-icon fragility across macOS 14/15/26.** Two reports conflict on whether a drop subview or the button itself receives drags. First shell milestone is a bare status item that accepts a drop on all three OS versions.
2. **Always-on rendering cost.** No `Canvas` benchmark exists. Shaders for the 3D characters, `Canvas` only for the grid, and an Instruments profile before any tuning.
3. **Detection recall on real documents.** Every benchmark number swings by a factor of two depending on the test set, and none covers US tax forms. Only our own labelled synthetic corpus will tell GLiNER from Privacy Filter. Fail-closed plus the attention card is what covers the gap in the meantime.
4. **Release-candidate dependencies.** `ort` 2.0 is still rc; rmcp 3.x has had breaking minors. Pin both, vendor the lockfile.
5. **Protocol transition.** Dual-era MCP (2025-11-25 and 2026-07-28) in both the server and the shim until Claude's rollout finishes.
6. **Config writing on the user's behalf.** No vendor blesses an app editing these files. Use the CLIs (`claude mcp add`, `codex mcp add`) where they exist, write Cursor's JSON only when Cursor is not running, and never touch `~/.claude.json` directly.

---

## 7. Next steps, in order

1. **Repo skeleton.** Cargo workspace with the crates above, empty Xcode project, `packs/tax`, `samples/` generator, pre-commit PII check, Linux CI for the core. (Not started; this document is the last thing written before code.)
2. **Spike: does an agent act on `🐙/` cold?** Minimal rmcp server over stdio with `instructions` and an always-loaded `resolve`. Test in Claude Code with and without a CLAUDE.md line, and in Codex. Decides how much onboarding copy is needed.
3. **Core pipeline, headless.** Ingest, extract (PDF text + AcroForm, OCR fixture), detect (templates + rules), tokenize, vault, index, then `list`/`resolve`/`describe`/`write` over the unix socket plus the stdio shim. Synthetic documents only.
4. **Detection benchmark.** Labelled synthetic corpus; GLiNER-PII via `gline-rs` versus Privacy Filter via MLX; pick one for the free-text pass.
5. **Shell: drop target first.** Bare `NSStatusItem` accepting drops on macOS 14, 15, 26. Then the move/copy dialog, the queue with its four states, handle copy, the badge.
6. **Renderers.** Metal shaders for eyes and octopus, `Canvas` for the ink grid, pause on close, Reduce Motion, Instruments budget.
7. **Onboarding.** Folder picker with the warnings in section 5.15, per-agent config writers from section 3.8, the AGENTS.md and CLAUDE.md line, and `inker doctor`.
8. **Later tools.** Ink Lab (Storybook for redact and dissolve), Asset Studio (posters and social on the ink grid).

---

## 8. Build and review log

**2026-10-28, build.** Rust core built on the Linux dev box by six parallel agents plus integration, all test-first: `inker-core`, `inker-extract`, `inker-detect` with 14 IRS templates, `inker-packs`, `inker-pipeline`, `inker-mcp`, `inker-ipc`, `inker-agents`, `inker-host`, `inker-ffi`, and the `inker` CLI. Workspace: 457 tests green, clippy clean, PII check clean. The Swift shell (`app/`) is written with Swift Testing coverage of its pure logic but has never been compiled: this box has no Swift toolchain. First Mac build is Ben's next step; see `app/BUILD.md`.

**Cold-handle spike (brief step 3): passed.** Claude Code called `resolve` on a bare `🐙/` handle in 12 of 12 runs, with and without a CLAUDE.md line. The server `instructions` alone suffice; `alwaysLoad` on `resolve` saves a round trip. `docs/spike-cold-handle.md`.

**Security review (independent agent, probes on synthetic data) found four leak paths; all closed, test-first:**
1. Form field values with no template mapping went to the agent unchanged. Now every field runs through every detector; unexplained leftovers are held.
2. Handles carried real names, and in one probe an EIN. Default codename policy is now `TypeYear`; folder handles are `folder-NNNN`; under the org policy digits are stripped and only template-attributed orgs are used; codenames are out of logs.
3. Only detected spans were replaced; repeats of an inked value in other spellings survived. Every page and field is now swept for every minted value.
4. A template `keep` box shielded anything on its line from the fail-closed pass. Keep now covers only amount- and code-shaped tokens.
Plus hardening: partial-overlap union, a lock file against a second writer, `write` rehydrates only tokens in the source's scope, redacting `Debug` impls, path-free ingest errors, 0700/0600 data files, zeroized keys, PDF-library panics and logs kept off stderr, outputs never reprocessed.

**Detection correctness review (independent agent, probes on the IRS's own 2025 text layers) found ten issues; nine closed:** wrapped titles and labels (anchors and labels now whitespace-tolerant and scored on the title area), wrapped payer labels read as the payer, SSNs with dots, dashes and fullwidth digits, revision year read as tax year, 1099-MISC 2025 vs 2026 boxes, bank-statement noise (13 questions on 20 rows, now at most 2), all-zero ABA, Mir cards, an untemplated `tax-form` kind. The tenth, the IRS "labels first, values after" layout letting unrecognised values through, is closed by the unplaced-values stage (section 3.5, step 6): on all 11 IRS text-layer fixtures no typed value is visible and each asks at most 3 questions. Held names carry no role yet; position-aware extraction (PDF coordinates) in `inker-extract` is the durable fix. The 1099-B 2026 revision still needs its own template.

**First end-to-end run (2026-10-28) found a W-2 surname in the clear; closed, test-first.** Box e prints first name and initial, last name and suffix as columns of one line, and the template engine read only the first column, so "Jordan A" was minted and the surname passed through. A template value for a `person` field now takes every following column segment on its line that reads as a name part (capitalised letters, not form wording; a lone trailing letter is a checkbox, not an initial), on every template: W-2 box e, the 1099 recipient lines, 1098, 1095 lines and rows, K-1 box F. Second line of defence: the tokenizer's sweep also looks for each 3+ letter word of a minted Person value wherever it appears capitalised outside a token on the same document, so a name read in part can never leave its surname beside the token. The same run showed the bank statement leaking its holder's whole name (no label, no model): a capitalised name line directly above a street line is now a Person. Codename shorts keep their hyphens (`🐙/1099-int-2025`, `🐙/k1-1065-2025`, `🐙/bank-statement-2025`).

**Known limits going into the first Mac build:**
- Vision OCR, PDFKit and the Keychain are implemented on the Swift side but unexercised; Linux uses fixture OCR and a file key store.
- The GLiNER model pass compiles behind a feature flag and has never run against weights. Detection today is templates, rules and fail-closed only.
- The index is plain SQLite (section 3.4).
- The host serialises all core access behind one lock; processing a document blocks MCP reads for its duration.

---

### 2026-10-28, evening: first runs on Ben's Mac (0.0 to 0.0.2)

- **0.0** launched as "damaged": the bundle had no signature at all. **0.0.1** ad-hoc signs
  (`make sign`, re-run after `inker-cli` is copied in); a quarantined copy now gets the
  unidentified-developer dialog and Open Anyway. Developer ID plus notarization is still to do.
- **0.0.2**, from Ben's first session with the app:
  - Onboarding was written for coding agents. It is now for chat users: step 1 is the folder
    picker only; step 2 "Your chat asks INKER" has Claude (Connect writes
    `claude_desktop_config.json` for real through `inker-agents`, then says to restart Claude),
    ChatGPT as coming soon, and Copy config for anything else; step 3 is "Drag it into INKER
    first". The coding-agent rows (Claude Code, Codex) moved to a Settings window. Cursor is
    CLI-only now; Ben is not focused on it. "The octopus" is gone from the copy: it is INKER.
  - The FFI `setup_agent` stub is replaced by the real writers; Swift passes the bundled
    `inker-cli` path and the home folder. `AgentKind` gained `ClaudeDesktop`.
  - The MCP server maps `clientInfo.name` to a product name ("claude-ai" is "Claude") for the
    popover's last-read line.
  - The move-or-copy question is a card inside the popover, not a window (it opened behind the
    popover). Cancel is new, since there is no window to close.
  - A menu bar app is invisible to Cmd+Tab, so onboarding could vanish behind other windows.
    The app now switches to a regular activation policy while onboarding or settings is open
    (`WindowPresence`), and back to accessory when the last window closes.
  - App icon: the MARK bitmap as halftone dots on the dark tile, Apple's 1024 template
    (`app/scripts/make-icon.py`, rendered PNG committed, `make icon` builds the `.icns`).

### 2026-10-29: finding documents (the find handoff)

Ben's designer delivered `docs/handoff/find-handoff/` (brief plus four artboards) for the
problem that `TypeYear` handles all look alike at 30 files. Built test-first, core to shell:

- **inker-core `find`**: the board grouped by tax year (newest first, `UNDATED` last), dropped
  folders as sub-groups placed by their newest child in the year most of their files have, tag
  counts for the chips, and matching over file name, handle (prefix optional), form type, payer,
  year, folder, tag and the tokenized page text, ranked in that order then by drop time. Text
  hits return the matching line trimmed to a window. `Core` gained `board`, `find`, `details`,
  `forget`, `forget_folder`, `ink_again`, `ink_again_folder` and `note_classification`.
- **Payer without a new real value in the index.** Templates attribute an employer, payer or
  lender; its token (`[ORG_3]`) is stored on the document (schema 3) and the name is read back
  through the vault only when the board is built. The index stays plain SQLite holding tokens.
- **Tags**: a form template may carry `tag`; it defaults to the pack's `category`, so every tax
  form is Taxes. Heuristic kinds: bank statement is Banking, closing statement is Legal, an
  untemplated IRS form is Taxes; receipts, scans and unknowns have no chip (under All only).
- **Forget** deletes the index row (cascades pages, questions, detections) and an emptied folder;
  the file stays. Write scope is computed from stored pages, so a forgotten document drops out of
  it with nothing else to do; vault entries are shared across documents and are kept.
  **Ink again** moves Ready or NeedsAttention back to Queued (new legal transitions), clears
  questions and keeps the codename because the record is no longer provisional.
- The pipeline notes a template match right after extraction, so a processing row shows its
  type before detection finishes.
- **FFI**: `DocItem` carries year, payer, tag, drop time and folder name; new `Board`,
  `BoardEntry`, `FolderItem`, `YearGroup`, `TagCount`, `FindMatch`, `DocDetails`;
  `CoreHandle.board(tag)`, `find`, `locate`, `details`, `forget`, `inkAgain`. The board lives in
  the host backend's snapshot (no core lock to read it); find and details take the lock, so the
  shell calls them off the main thread. `PreviewScenario::Scale` is the 30-file board.
- **Shell**: `FindBoard` replaces `QueueBoard` on the ready board only; processing and attention
  keep their layouts. Header: folder button plus the sliders icon in place of the gear. Footer:
  the last-read handle is a button, the right side reads "30 files · MCP running". Settings:
  "Show payer names in the list" and "Offer to find a handle from the clipboard", both on.
  The pasteboard is read in `popoverDidShow` only. Year open state is remembered in
  `UserDefaults`; folder open state is per session.
- Ben's decisions: payer on with a toggle; clipboard chip on with a toggle; **Forget confirms
  inside the menu**: the first click turns the item into a red "Are you sure?" and the menu
  stays up; the next click forgets; a click elsewhere, Escape or closing the popover resets it.
  (First shipped in 0.0.6 as a flip to "Forget <name>" with the native menu closing between
  clicks; Ben asked for the clearer form on seeing it.) That needs the menu to be INKER's own
  panel rather than an NSMenu, which closes on every click; the AppFindRow artboard draws it
  as a 200-wide dark panel anyway. `RowMenu` (InkerKit) lists the items and picks below or
  above the row; `RowMenuLayer` draws it over the board from an anchor preference, so it is not
  clipped by the scroll view; `RightClickCatcher` is an NSView that only answers secondary
  clicks and lets everything else through to SwiftUI. Escape closes the menu before the popover.
- Calls made without an artboard, for Ben to check: a folder or tag match shows its folder name
  or tag as the row's third line (muted, not mono) so the underline has somewhere to land; a
  folder's Forget and Ink again apply to every child; the Settings "Private folder" block from the
  morning stays beside the header button; the Scale fixture keeps `w2-bluefin.pdf` loose, where
  the Scale board shows it, though the Handle board drew it inside the folder.
- Not run on this Linux box: `swift test`, `swift build`, the snapshots. The Rust workspace
  (tests, clippy, fmt) is green. Bindings were regenerated with `build-rust.sh --bindings-only`.
  The release workflow (macOS runner) is what compiles the Swift from here; 0.0.6 was its first
  run over this work and found two `#expect` forms the macro rejects.

**The attention card's page crop (2026-10-29 evening, after 0.0.6).** Ben: the card's preview
should be a picture of the document, cropped to the part in question, not the octopus and the
elided snippet. His calls: a tight crop with the span outlined in red; the octopus and snippet
stay only as the fallback when the span cannot be placed; the "…23 ... and ending..." line goes.
How it works without a real value leaving the core: the core already knows the detection's
byte span in the page's text, so a question now records it (schema 4, `review_questions.span_start`
and `span_end`, `None` for a form field's value) and the FFI question carries `page_index` and
`span`. On macOS the page text the core indexed came from PDFKit or Vision through the shell's
own providers, so the shell can re-read the user's file and map the span back: a PDF with a text
layer through `PDFPage.selection(for:)`, a scan through the same Vision recognizer with each
line's box and the span placed along its line by proportion. `PageCrop` (InkerKit, tested) picks
the zoom (the value about 16 pt tall, clamped 1.25x to 3x), centres the 302 by 84 box on the
value and keeps it inside the page; `PageSnippet` (AppKit) renders it at 2x on a GCD thread,
once per question. Rotated PDF pages keep the octopus until checked by eye (VERIFY in the code).
Previews and snapshots have no file to read, so the attention board still shows the octopus.

**Staying open, and "Claude connected" (2026-10-30, after 0.0.7).** Ben reversed the
outside-click close: the popover should stay up while he clicks around Finder and drags files
in. It is never transient now; the menu bar icon and Escape close it, and a pinned popover
(folder chooser up) ignores Escape. He also asked for easier access to connecting Claude and an
indicator of whether it is connected. His calls: "connected" means a live MCP session (the Claude
app running with INKER loaded), not a config entry; the footer's right side says "Claude
connected" or "Claude not connected" in place of "MCP running" ("MCP not running" still wins when
INKER's own socket is down), the latter a link that opens Settings; Settings gains a "Chat apps"
block above the coding agents, with onboarding step 2's rows and the Claude row's state: Connect,
"Set up · restart Claude", or "Connected". How the app knows: `inker_mcp::Sessions` is a registry
every served connection joins; `initialize` names the session after the client's product name
(`display_agent_name`), the connection's end removes it, and each change fires the shell's
change sink so the footer updates while the popover is up. `Status.clients` carries the names;
`agent_report` gives Settings the installed and registered bits from `doctor`.

## 9. Target clients (reframed 2026-10-28)

INKER is for the chat window. Research on 2026-10-28 (search summaries only; vendor pages were unreachable from the dev box that day, so re-read primary sources before relying on a detail):

| Client | Local MCP | How INKER plugs in | Notes |
|---|---|---|---|
| **Claude desktop app (macOS)** | stdio via `~/Library/Application Support/Claude/claude_desktop_config.json`, or a side-loaded `.mcpb` Desktop Extension bundle | **Primary.** Onboarding writes the config line (restart required) or the user installs the bundle. | Reportedly ignores the server `instructions` field, so the `🐙/` guidance lives in the tool descriptions. Unsigned bundles show an "unverified" warning; Anthropic's directory no longer accepts MCPB, only plugins. Each tool call prompts Allow once / Always allow; the default is now Always allow. |
| Raycast AI, Perplexity Mac | stdio | Same command; "copy this JSON". | Cheap second clients. |
| **ChatGPT desktop app** | Chat mode: remote HTTPS connectors only (Developer mode). Codex mode: stdio via `~/.codex/config.toml`. | Stretch. The Codex writer already covers the config; whether plain chat can see it is unverified. | Write tools prompt unless `readOnlyHint`. |
| claude.ai, chatgpt.com | Remote HTTPS with OAuth only; never localhost | **Parked.** Needs a public relay. OpenAI has a first-party outbound tunnel client (enterprise/API). | A relay only ever carries tokenized text, which is the one thing that makes it thinkable; still, deferred. |
| Claude Code, Cursor, Codex CLI | stdio | Already built; now the "also works with coding agents" tier. | |

**Consequences for the build**
- The `🐙/` rule moves into the `list`, `resolve` and `describe` tool descriptions ("when the user's message contains `🐙/…` or `ink/…`, call `resolve`"). The `instructions` field stays for clients that honour it.
- Onboarding step 1 shrinks to "pick a folder"; the exclusion writers become an optional extra. But the "keep out" layer is not moot: Claude Desktop's Cowork, Code tab and Chrome extension can reach local files, and a July 2026 Cowork sandbox escape exposed the host filesystem. The private folder outside any workspace remains the real wall.
- The cold-handle spike must be rerun in Claude Desktop on a Mac; the Claude Code result (12/12) does not transfer, because the mechanism it relied on (instructions) is reportedly not applied there.
- Distribution: a `.mcpb` containing a signed, notarized copy of the shim, plus the config-line fallback. Every binary a chat app launches must be Developer ID signed; unsigned helpers get killed on managed Macs.
- The website and app copy say "your AI chat", not "your coding agent".

# INKER: stack decision and architecture

Date: 2026-10-28. Decision by Ben: **Rust core, Swift shell.** This document records what INKER is, why that stack, how the pieces fit, what the research found, and what is still open. Source brief: [`handoff/HANDOFF.md`](handoff/HANDOFF.md). Design boards: [`handoff/design/`](handoff/design/) (App v2 page is the build target).

Facts below marked *(verified)* were checked against vendor docs or source on 2026-10-28. Facts marked *(unverified)* come from secondary sources or inference and need a hands-on check.

---

## 1. What INKER is

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
- **Index:** everything derived from a document (tokenized text, detections, review decisions, codename, state) lives in a SQLite store in Application Support, never beside the file. This honours "no mirror folder": the only thing in `~/Private` is the original file. *As built (2026-10-28):* the SQLite file itself is not encrypted (plain bundled SQLite, not SQLCipher). It holds tokenized text only, but `original_name` and `path` are plain, so a file named after a person puts that name in the index. The raw extracted text of a held document is sealed with the vault key while it waits for review and deleted once the document is Ready. Encrypting the whole index is a follow-up if Ben wants it.
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

Three treatments per detection, from the brief: tokenize, generalize, keep. Treatment is decided by type and template, overridable per document by the user's answer on the attention card.

### 3.6 MCP surface

Tools, as the brief lists them: `list`, `resolve`, `describe`, `write`. Annotations: `list`/`resolve`/`describe` are `readOnlyHint`; `write` is not. `resolve` is marked `_meta["anthropic/alwaysLoad"]` so Claude Code's tool search never hides it.

Server `instructions` open with the one sentence that matters: strings starting with `🐙/` are private document references; call `resolve`. Claude Code loads server instructions at session start even with tool search on (truncated at 4,096 characters) *(verified)*. Codex honours them too ("first 512 characters matter most") *(verified)*. Cursor's docs do not mention them *(verified: undocumented)*. The brief's step 3 spike still runs, because reports say Claude Code injects instructions as a user-turn delta and discovery of deferred servers is weak.

`write` takes tokenized content from the agent, rehydrates it, and saves it only into the private folder. It returns a confirmation and a new handle for the output file; the agent can `resolve` that handle for the tokenized view (section 5.6).

### 3.7 The shell

- **AppKit `NSStatusItem` + `NSPopover`** hosting SwiftUI through `NSHostingController`. Not `MenuBarExtra`: it cannot take drops on its label and has no first-party presentation state *(verified)*. A shipping open-source app (prof18/slashgrab) does exactly this pattern with a drop subview over `statusItem.button`. One report says the subview did not receive drags while the button itself did; test both on macOS 14, 15 and 26.
- **Popover** is `.applicationDefined` while a drag is in progress, `.transient` otherwise. Fixed `contentSize` 360x500.
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

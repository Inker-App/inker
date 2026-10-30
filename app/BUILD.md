# Building INKER.app

The shell is a Swift package (no `.xcodeproj`). It links the Rust core through
UniFFI: `core/crates/inker-ffi` builds a static library, `scripts/build-rust.sh`
wraps it in `Frameworks/InkerCoreFFI.xcframework` and regenerates the Swift
bindings in `Sources/InkerCore/`.

Needs a Mac with Xcode 26 (Swift 6.2+ toolchain, macOS 26 SDK) and rustup.

## 1. One-time setup

```sh
xcode-select -p                       # should point at Xcode 26
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin # only for --universal builds
```

## 2. Build the Rust core and the bindings

```sh
cd app
scripts/build-rust.sh              # arm64, release
scripts/build-rust.sh --universal  # arm64 + x86_64 (lipo), for distribution
scripts/build-rust.sh --debug      # debug profile
```

This:

1. `cargo build -p inker-ffi --release --target aarch64-apple-darwin` (plus x86_64 with `--universal`).
2. Runs the crate's own `uniffi-bindgen-swift` (UniFFI 0.32.2, pinned) on the dylib to write
   `Sources/InkerCore/InkerCore.swift`, `Sources/InkerCore/include/InkerCoreFFI.h` and
   `Sources/InkerCore/include/module.modulemap`. Names come from `core/crates/inker-ffi/uniffi.toml`.
3. `xcodebuild -create-xcframework -library libinker_ffi.a -headers Sources/InkerCore/include`
   into `Frameworks/InkerCoreFFI.xcframework` (git-ignored).
4. Prints the `native-static-libs` the Rust lib needs. Compare with `linkerSettings` in
   `Package.swift` (today: CoreFoundation). Add any missing framework there. The static lib
   now includes the whole core (pipeline, PDF parsing, tokio for the socket); Security,
   PDFKit and Vision are used from Swift only and are linked by its `import`s.

The generated Swift and header are committed as a starting point (first generated on Linux
with `scripts/build-rust.sh --bindings-only`). Re-run the script after any change to the FFI
surface; never edit them by hand.

## 3. Build, test, run

```sh
make test   # swift test: the InkerAppTests suite (Swift Testing)
make app    # swift build -c release, then assemble build/INKER.app
make run    # make app, then open build/INKER.app
```

`swift build` and `swift run INKER` work too; the app runs as a menu bar item either way.

### The disk image

```sh
pip install dmgbuild   # once
make dmg               # build/INKER.dmg: INKER.app beside an Applications link
```

The Finder window of the image is a picture plus pinned icon positions, so both come from
one place: `scripts/make-dmg-background.py` draws `dmg/background.png` (and `@2x`), the
halftone octopus below the icon row with a dotted arrow between them, and writes
`dmg/layout.json`, which `dmg/settings.py` feeds to dmgbuild. Re-run the script after
changing the layout and commit the PNGs; `scripts/test_make_dmg_background.py` checks the
committed images still match. The release workflow calls `scripts/make-dmg.sh` the same way.
Open `Package.swift` in Xcode for SwiftUI previews (`Sources/InkerApp/Previews.swift`, one per
popover screen) and Instruments.

### One popover, no windows

Everything INKER shows is a screen of the 360x500 popover: the four boards, onboarding steps
1 to 3, settings, and the move-or-copy card. `ScreenFlow` (InkerKit, covered by
`ScreenFlowTests`) decides which; `CoreStore.flow` publishes it and `PopoverRootView` draws it
between the header and the footer.

- First launch (onboarding not complete, or `INKER_ONBOARDING=1`) starts at step 1, and
  `AppDelegate` opens the popover by itself.
- Back and the primary button are pinned to the bottom of each step; the scene, heading and
  controls scroll above them if they must. The boards' 480-wide scenes are drawn at their
  board size and scaled to the 328-point column (`ScaledScene`); the heading is 18 pt.
- Done marks onboarding complete and shows the board. The gear opens settings (and closes
  them again); settings has Done, "Run setup again" (back to step 1) and "Quit INKER".
- A pending drop draws the card in front of the current screen. The screen stays underneath,
  so an onboarding step keeps what was typed into it, and it comes back once the card is
  answered.
- The folder chooser in step 1 is the system `NSOpenPanel`; the popover is pinned
  (`.applicationDefined`) while it is up so it is still there afterwards.

### Render check: `--snapshots`

```sh
build/INKER.app/Contents/MacOS/INKER --snapshots build/snapshots
```

renders each screen with SwiftUI's `ImageRenderer` at 2x into PNGs (`popover-empty`,
`-processing`, `-attention`, `-ready`, `onboarding-1` to `-3`, `settings`, `drop-card`) and
exits 0, without starting `NSApplication` or opening a window. The stores run on the Rust
`PreviewBackend` (as `INKER_PREVIEW` does) with the clock frozen at `RenderClock.stillTime`.
`ImageRenderer` cannot draw AppKit-backed views, so under the `snapshotMode` environment
value scroll views are drawn clipped at rest, the folder field is plain text and the checkbox
is an empty box. **The Metal eyes and octopus are blank in snapshots**; check them in the app.
The release workflow runs this after `make app` and uploads the PNGs as the
`snapshots-<version>` artifact. Code: `Sources/InkerApp/App/Snapshots.swift`.

Useful environment variables (Xcode scheme, or `INKER_PREVIEW=attention swift run INKER`):

| Variable | Effect |
| --- | --- |
| `INKER_PREVIEW=empty\|processing\|attention\|ready` | Freeze the popover in one board (Rust `PreviewBackend`). |
| `INKER_ONBOARDING=1` | Start at onboarding step 1, and open the popover, even after onboarding was completed. |

To reset first-launch state: `defaults delete sh.inker.app`.

### Tests

`swift test` runs `Tests/InkerAppTests` with Swift Testing (`import Testing`, `@Test`). They
cover the pure logic in the `InkerKit` target, which has no AppKit dependency:

- ink grid math (dot radius, bridge widths, 2x2 fills, diagonal bridges, winding), the
  `LET`/`MARK` bitmaps, loaders and the header lockup;
- the eye state machine against golden values produced by running the design boards' own
  JavaScript in node (saccades, darts, blinks, gaze), plus the shading reference that
  `EyeShader.metal` ports;
- Gray-Scott reaction-diffusion (golden values, steady state, growth, wrap-around);
- the octopus rig (`build(T)` and the rotation, golden values);
- handles, drop collection and the drop dialog copy, folder-picker warnings, agent rows,
  preferences, relative time, copy feedback and the queue view-model;
- the popover's screen flow (first launch, steps, Done, settings, run setup again, the drop
  card in front of any screen);
- the providers' pure logic (`ProviderText`, `VaultKey`).

The tests were written before the code they cover, but they have **not been compiled or run
yet**: this package was written on Linux without a Swift toolchain. Expect a first round of
compile fixes; see the `// VERIFY:` comments.

The Rust side is tested separately: `cd core && cargo test -p inker-ffi` (the FFI surface,
`HostBackend` over a real core on a temp dir, the MCP socket, and the provider adapters with
a Rust fake in place of Swift).

## What is real and what is a stub

- `CoreHandle.open(privateFolder:dataDir:providers:)` opens the **real core** (`inker_host::Host`
  behind `HostBackend` in `core/crates/inker-ffi`). Drops really move or copy files into the
  private folder, and they are read, detected and inked. `AppDelegate` passes
  `~/Library/Application Support/sh.inker` as the data dir (vault, index, `inker.lock`,
  `inker.sock`); that is the path `inker_ipc::paths::data_dir()` gives on macOS, so the
  `inker mcp` shim finds the socket. Only one process can open a data dir: a second INKER (or
  `inker serve`) fails `open`, and the app falls back to an empty preview.
- `providers` is the UniFFI callback interface `HostProviders`, implemented by
  `Sources/InkerApp/Providers/MacProviders.swift`:

  | Method | Swift | Notes |
  | --- | --- | --- |
  | `loadOrCreateKey() -> Data` | `KeychainKeyStore` | generic password, service `sh.inker.vault`, account `vault-key`; 32 bytes from `SecRandomCopyBytes` on first use. Asked once, at open. |
  | `pdfText(path:) -> ExtractedFfi?` | `PDFKitTextSource` | `PDFPage.string` per page plus widget `fieldName`/`widgetStringValue`; nil when every page is blank and no field is filled, which sends the file to OCR. |
  | `ocr(path:) -> ExtractedFfi` | `VisionOcrSource` | PDF pages rendered at 200 dpi, images (PNG, JPEG, HEIC) decoded by ImageIO upright; `RecognizeDocumentsRequest` on macOS 26, `VNRecognizeTextRequest` otherwise. |

  Rust calls them from its ingest worker thread, never the main thread, and they may block.
  Anything they throw becomes a failed document (or a failed `open`, for the key); throw
  `ProviderError.Failed(reason:)` with a reason that names no file, value or text. The pure
  shaping (blank detection, page numbering, line joining, field dedupe, reading order, render
  size) is `InkerKit/Providers/ProviderText.swift`, covered by `ProviderTextTests`.
- **The MCP socket runs inside the app.** `open` also starts `inker_ipc::host::serve` on
  `<data dir>/inker.sock` (0600, in a 0700 dir) on a tokio runtime in a background thread.
  Agents reach it through `inker mcp`. `status().mcpListening` is true while the socket is
  bound; if another process already holds it, the queue still works and `mcpListening` stays false.
  An agent's read or write refreshes the queue and fires the listener, so the "read by" line
  updates live.
- `handle.close()` (called from `applicationWillTerminate`) stops the socket, removes
  `inker.sock`, and stops the worker after the document in hand. Documents still queued are
  processed at the next launch. Dropping the handle does the same.
- `ingest` returns once the files are moved and their rows exist (`queued`); processing runs
  on one worker thread, in drop order, and the listener fires after each state change.
  `queue()` and `recentReads()` read a snapshot and never wait on the core.
  `ingest` and `answer` do take the core's lock, so they can wait for the document being
  processed to finish (seconds for a long scan).
- Not yet from the core: the attention card's `contextBefore` is empty and `valueLength` is 0,
  and `location` is "page N" without the page count.
- `CoreHandle.preview(scenario:)` still returns the Rust `PreviewBackend` for SwiftUI previews
  and `INKER_PREVIEW`.
- `setup_agent` returns fixed plans and refuses non-dry runs; `doctor` returns fixed text.
- The settings gear opens the settings screen in the popover (handle style, coding agents,
  run setup again, quit). There is no settings design yet; the layout is a placeholder.

On Linux and in Rust tests, `CoreHandle::open_with_rust_providers(private, data)` opens the same
core with the file key store (`<data dir>/vault.key`) and the pure-Rust extractor. It is
Rust-only and not in the Swift bindings.

## Codesigning and notarization

`make app` ad-hoc signs the bundle (`SIGN_IDENTITY=-`), and `make sign` re-signs it after
anything is added to `Contents/MacOS`. Ad-hoc is the floor: without any signature macOS
reports a downloaded copy as "damaged" and offers only the Trash. With it, a quarantined
copy gets the "unidentified developer" dialog and the Open Anyway switch in System Settings.

A real release is Developer ID signed with the hardened runtime, notarized and stapled, and
**not** sandboxed (stack-decision section 4: a stdio helper cannot inherit the sandbox). The
release workflow does all of it when the repository has these secrets, and falls back to
ad-hoc when it does not, so a fork or a build-only run still works:

| Secret | What |
| --- | --- |
| `MACOS_CERT_P12` | The "Developer ID Application" certificate with its private key, exported from Keychain Access as a `.p12`, base64 encoded (`base64 -i inker.p12`). |
| `MACOS_CERT_PASSWORD` | The password given at export. |
| `NOTARY_KEY_ID`, `NOTARY_ISSUER_ID`, `NOTARY_KEY_P8` | An App Store Connect API key (Users and Access > Integrations > App Store Connect API > Team Keys, access "Developer"): its Key ID, the Issuer ID shown above the table, and the contents of the downloaded `AuthKey_<id>.p8`. |

Locally, with the certificate in the login keychain:

```sh
make app SIGN_IDENTITY="Developer ID Application: <name> (<team>)"   # hardened runtime, timestamped
ditto -c -k --keepParent build/INKER.app build/INKER.zip
xcrun notarytool submit build/INKER.zip --keychain-profile <profile> --wait
xcrun stapler staple build/INKER.app
```

Nested code is signed first (`inker-cli` inside the app), then the bundle. The CLI that ships
on its own and inside the `.mcpb` is signed and notarized too; a bare binary cannot be
stapled, so Gatekeeper looks its ticket up online. Hardened runtime needs no extra
entitlements for what the shell does today.

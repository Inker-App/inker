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
   `Package.swift` (today: CoreFoundation). Add any missing framework there.

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
Open `Package.swift` in Xcode for SwiftUI previews (`Sources/InkerApp/Previews.swift`, one per
board) and Instruments.

Useful environment variables (Xcode scheme, or `INKER_PREVIEW=attention swift run INKER`):

| Variable | Effect |
| --- | --- |
| `INKER_PREVIEW=empty\|processing\|attention\|ready` | Freeze the popover in one board (Rust `PreviewBackend`). |
| `INKER_ONBOARDING=1` | Show onboarding even after it was completed. |

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
  preferences, relative time, copy feedback and the queue view-model.

The tests were written before the code they cover, but they have **not been compiled or run
yet**: this package was written on Linux without a Swift toolchain. Expect a first round of
compile fixes; see the `// VERIFY:` comments.

The Rust side is tested separately: `cd core && cargo test -p inker-ffi`.

## What is real and what is a stub

- `CoreHandle.open(...)` returns the Rust `PreviewBackend`: drops are simulated (queued,
  reading, inking, ready; one file in four of a folder is held for the attention card) and
  **files are never moved or read**. The real `inker_core::Core` gets wired as another
  `Backend` in `inker-ffi`; the Swift side does not change.
- `setup_agent` returns fixed plans and refuses non-dry runs; `doctor` returns fixed text.
- The settings gear opens a small placeholder menu (handle style, set up agents, quit).
  There is no settings design yet.

## Codesigning and notarization (later)

The app must be Developer ID signed and **not** sandboxed (stack-decision section 4: a stdio
helper cannot inherit the sandbox). When there is a certificate, add to the `app` target in
the Makefile, after the bundle is assembled:

```sh
codesign --force --options runtime --timestamp \
  --sign "Developer ID Application: <name> (<team>)" build/INKER.app
ditto -c -k --keepParent build/INKER.app build/INKER.zip
xcrun notarytool submit build/INKER.zip --keychain-profile <profile> --wait
xcrun stapler staple build/INKER.app
```

Sign nested code first if any is added (the `inker` CLI helper, when it ships inside the app).
Hardened runtime needs no extra entitlements for what the shell does today.

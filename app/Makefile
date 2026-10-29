# INKER.app from the Swift package. See BUILD.md.
#
#   make rust    build inker-ffi, regenerate bindings, assemble the xcframework
#   make app     swift build -c release, then assemble build/INKER.app
#   make run     make app, then open it
#   make test    swift test (InkerAppTests)

CONFIG ?= release
APP := build/INKER.app
# Signing identity. "-" is ad-hoc: enough for macOS to run the bundle after
# the quarantine flag is cleared or "Open Anyway" is chosen. Without any
# signature Gatekeeper reports a downloaded copy as "damaged". Pass a
# "Developer ID Application: ..." identity for a real release; see BUILD.md.
SIGN_IDENTITY ?= -

.PHONY: rust app sign run test clean

rust:
	scripts/build-rust.sh

app:
	swift build -c $(CONFIG) --product INKER
	rm -rf $(APP)
	mkdir -p $(APP)/Contents/MacOS $(APP)/Contents/Resources
	cp Resources/Info.plist $(APP)/Contents/Info.plist
	BIN="$$(swift build -c $(CONFIG) --show-bin-path)"; \
	  cp "$$BIN/INKER" $(APP)/Contents/MacOS/INKER && \
	  cp -R "$$BIN/INKER_InkerApp.bundle" $(APP)/Contents/Resources/
	cp Resources/MenuBarIcon.svg $(APP)/Contents/Resources/
	$(MAKE) sign
	@echo "built $(APP)"

# Sign nested executables first, then the bundle, then verify. Re-run after
# adding anything to Contents/MacOS (the release workflow copies inker-cli in).
sign:
	for f in $(APP)/Contents/MacOS/*; do \
	  [ "$$f" = "$(APP)/Contents/MacOS/INKER" ] || codesign --force --sign "$(SIGN_IDENTITY)" "$$f"; \
	done
	codesign --force --sign "$(SIGN_IDENTITY)" --identifier sh.inker.app $(APP)
	codesign --verify --deep --strict --verbose=2 $(APP)

run: app
	open $(APP)

test:
	swift test

clean:
	rm -rf build .build

# INKER.app from the Swift package. See BUILD.md.
#
#   make rust    build inker-ffi, regenerate bindings, assemble the xcframework
#   make app     swift build -c release, then assemble build/INKER.app
#   make run     make app, then open it
#   make test    swift test (InkerAppTests)

CONFIG ?= release
APP := build/INKER.app
ICONSET := build/AppIcon.iconset
# Signing identity. "-" is ad-hoc: enough for macOS to run the bundle after
# the quarantine flag is cleared or "Open Anyway" is chosen. Without any
# signature Gatekeeper reports a downloaded copy as "damaged". Pass a
# "Developer ID Application: ..." identity for a real release; see BUILD.md.
SIGN_IDENTITY ?= -
# A real identity gets the hardened runtime and a secure timestamp, which
# notarization requires. Ad-hoc signatures cannot be timestamped.
ifeq ($(SIGN_IDENTITY),-)
CODESIGN_FLAGS ?=
else
CODESIGN_FLAGS ?= --options runtime --timestamp
endif

.PHONY: rust app icon sign run test clean

rust:
	scripts/build-rust.sh

app: icon
	swift build -c $(CONFIG) --product INKER
	rm -rf $(APP)
	mkdir -p $(APP)/Contents/MacOS $(APP)/Contents/Resources
	cp Resources/Info.plist $(APP)/Contents/Info.plist
	BIN="$$(swift build -c $(CONFIG) --show-bin-path)"; \
	  cp "$$BIN/INKER" $(APP)/Contents/MacOS/INKER && \
	  cp -R "$$BIN/INKER_InkerApp.bundle" $(APP)/Contents/Resources/
	cp Resources/MenuBarIcon.svg $(APP)/Contents/Resources/
	cp build/AppIcon.icns $(APP)/Contents/Resources/
	$(MAKE) sign
	@echo "built $(APP)"

# AppIcon.icns from the rendered 1024 px PNG (scripts/make-icon.py draws the
# SVG; the PNG is rendered from it and committed). macOS tools only.
icon:
	rm -rf $(ICONSET) && mkdir -p $(ICONSET)
	for s in 16 32 128 256 512; do \
	  sips -z $$s $$s Resources/AppIcon-1024.png --out $(ICONSET)/icon_$${s}x$${s}.png >/dev/null; \
	  d=$$((s * 2)); sips -z $$d $$d Resources/AppIcon-1024.png --out $(ICONSET)/icon_$${s}x$${s}@2x.png >/dev/null; \
	done
	iconutil -c icns $(ICONSET) -o build/AppIcon.icns

# Sign nested executables first, then the bundle, then verify. Re-run after
# adding anything to Contents/MacOS (the release workflow copies inker-cli in).
sign:
	for f in $(APP)/Contents/MacOS/*; do \
	  [ "$$f" = "$(APP)/Contents/MacOS/INKER" ] || codesign --force $(CODESIGN_FLAGS) --sign "$(SIGN_IDENTITY)" "$$f"; \
	done
	codesign --force $(CODESIGN_FLAGS) --sign "$(SIGN_IDENTITY)" --identifier sh.inker.app $(APP)
	codesign --verify --deep --strict --verbose=2 $(APP)

run: app
	open $(APP)

test:
	swift test

clean:
	rm -rf build .build

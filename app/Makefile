# INKER.app from the Swift package. See BUILD.md.
#
#   make rust    build inker-ffi, regenerate bindings, assemble the xcframework
#   make app     swift build -c release, then assemble build/INKER.app
#   make run     make app, then open it
#   make test    swift test (InkerAppTests)

CONFIG ?= release
APP := build/INKER.app

.PHONY: rust app run test clean

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
	@# Codesigning and notarization slot in here; see BUILD.md.
	@echo "built $(APP)"

run: app
	open $(APP)

test:
	swift test

clean:
	rm -rf build .build

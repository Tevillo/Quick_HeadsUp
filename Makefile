# Makefile for Guess-Up release packages
#
# Produces 4 archives in ./dist/:
#   guess_up-<ver>-linux-x86_64.tar.gz   (guess_up binary + lists/)
#   guess_up-<ver>-windows-x86_64.zip    (guess_up.exe + lists/)
#   relay-<ver>-linux-x86_64.tar.gz      (relay binary)
#   relay-<ver>-windows-x86_64.zip       (relay.exe)
#
# Usage:
#   make release            # build all 4 packages
#   make release-linux      # Linux packages only
#   make release-windows    # Windows packages only
#   make guess-up-linux     # single package
#   make clean              # remove ./dist/
#   make help               # list targets
#
# Requirements:
#   - rustup with the x86_64-unknown-linux-gnu and x86_64-pc-windows-gnu targets
#     (install with: rustup target add <target>)
#   - x86_64-w64-mingw32-gcc for Windows cross-linking
#   - tar, zip

VERSION_CLIENT := $(shell awk -F'"' '/^version/{print $$2; exit}' crates/client/Cargo.toml)
VERSION_RELAY  := $(shell awk -F'"' '/^version/{print $$2; exit}' crates/relay/Cargo.toml)

LINUX_TARGET   := x86_64-unknown-linux-gnu
WINDOWS_TARGET := x86_64-pc-windows-gnu

DIST_DIR    := dist
STAGING_DIR := $(DIST_DIR)/staging

GUESS_UP_LINUX_NAME   := guess_up-$(VERSION_CLIENT)-linux-x86_64
GUESS_UP_WINDOWS_NAME := guess_up-$(VERSION_CLIENT)-windows-x86_64
RELAY_LINUX_NAME      := relay-$(VERSION_RELAY)-linux-x86_64
RELAY_WINDOWS_NAME    := relay-$(VERSION_RELAY)-windows-x86_64

.PHONY: help all release release-linux release-windows \
        guess-up-linux guess-up-windows \
        relay-linux relay-windows \
        build-linux build-windows \
        check-zip clean

help:
	@echo "Guess-Up release packaging"
	@echo ""
	@echo "Targets:"
	@echo "  release          Build all 4 packages (guess_up + relay, Linux + Windows)"
	@echo "  release-linux    Linux packages only"
	@echo "  release-windows  Windows packages only"
	@echo "  guess-up-linux   Linux guess_up package"
	@echo "  guess-up-windows Windows guess_up package"
	@echo "  relay-linux      Linux relay package"
	@echo "  relay-windows    Windows relay package"
	@echo "  clean            Remove ./$(DIST_DIR)/"
	@echo ""
	@echo "Outputs land in ./$(DIST_DIR)/"

all: release

release: release-linux release-windows

release-linux: guess-up-linux relay-linux

release-windows: guess-up-windows relay-windows

build-linux:
	cargo build --release --locked --target $(LINUX_TARGET)

build-windows:
	cargo build --release --locked --target $(WINDOWS_TARGET)

check-zip:
	@command -v zip >/dev/null 2>&1 || { \
		echo "error: 'zip' not found in PATH — install it (e.g. pacman -S zip, apt install zip)"; \
		exit 1; \
	}

guess-up-linux: build-linux
	@rm -rf $(STAGING_DIR)/$(GUESS_UP_LINUX_NAME)
	@mkdir -p $(STAGING_DIR)/$(GUESS_UP_LINUX_NAME)/lists $(DIST_DIR)
	cp target/$(LINUX_TARGET)/release/guess_up $(STAGING_DIR)/$(GUESS_UP_LINUX_NAME)/
	cp lists/*.txt                             $(STAGING_DIR)/$(GUESS_UP_LINUX_NAME)/lists/
	cp README.md                               $(STAGING_DIR)/$(GUESS_UP_LINUX_NAME)/
	tar -czf $(DIST_DIR)/$(GUESS_UP_LINUX_NAME).tar.gz -C $(STAGING_DIR) $(GUESS_UP_LINUX_NAME)
	@echo "==> $(DIST_DIR)/$(GUESS_UP_LINUX_NAME).tar.gz"

guess-up-windows: check-zip build-windows
	@rm -rf $(STAGING_DIR)/$(GUESS_UP_WINDOWS_NAME)
	@mkdir -p $(STAGING_DIR)/$(GUESS_UP_WINDOWS_NAME)/lists $(DIST_DIR)
	cp target/$(WINDOWS_TARGET)/release/guess_up.exe $(STAGING_DIR)/$(GUESS_UP_WINDOWS_NAME)/
	cp lists/*.txt                                    $(STAGING_DIR)/$(GUESS_UP_WINDOWS_NAME)/lists/
	cp README.md                                      $(STAGING_DIR)/$(GUESS_UP_WINDOWS_NAME)/
	cd $(STAGING_DIR) && zip -qr $(CURDIR)/$(DIST_DIR)/$(GUESS_UP_WINDOWS_NAME).zip $(GUESS_UP_WINDOWS_NAME)
	@echo "==> $(DIST_DIR)/$(GUESS_UP_WINDOWS_NAME).zip"

relay-linux: build-linux
	@rm -rf $(STAGING_DIR)/$(RELAY_LINUX_NAME)
	@mkdir -p $(STAGING_DIR)/$(RELAY_LINUX_NAME) $(DIST_DIR)
	cp target/$(LINUX_TARGET)/release/relay $(STAGING_DIR)/$(RELAY_LINUX_NAME)/
	tar -czf $(DIST_DIR)/$(RELAY_LINUX_NAME).tar.gz -C $(STAGING_DIR) $(RELAY_LINUX_NAME)
	@echo "==> $(DIST_DIR)/$(RELAY_LINUX_NAME).tar.gz"

relay-windows: check-zip build-windows
	@rm -rf $(STAGING_DIR)/$(RELAY_WINDOWS_NAME)
	@mkdir -p $(STAGING_DIR)/$(RELAY_WINDOWS_NAME) $(DIST_DIR)
	cp target/$(WINDOWS_TARGET)/release/relay.exe $(STAGING_DIR)/$(RELAY_WINDOWS_NAME)/
	cd $(STAGING_DIR) && zip -qr $(CURDIR)/$(DIST_DIR)/$(RELAY_WINDOWS_NAME).zip $(RELAY_WINDOWS_NAME)
	@echo "==> $(DIST_DIR)/$(RELAY_WINDOWS_NAME).zip"

clean:
	rm -rf $(DIST_DIR)

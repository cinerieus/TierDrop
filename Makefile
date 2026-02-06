VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)"/\1/')
NAME    := tierdrop
DIST    := dist

LINUX_TARGET     := x86_64-unknown-linux-gnu
LINUX_ARM_TARGET := aarch64-unknown-linux-gnu
WINDOWS_TARGET   := x86_64-pc-windows-gnu

LINUX_BIN     := $(NAME)-$(VERSION)-linux-amd64
LINUX_ARM_BIN := $(NAME)-$(VERSION)-linux-arm64
WINDOWS_BIN   := $(NAME)-$(VERSION)-windows-amd64.exe

RUSTFLAGS_STATIC := -C target-feature=+crt-static --remap-path-prefix=$(HOME)=/build --remap-path-prefix=/rustc=/rustc

.PHONY: linux linux-arm windows linux-debug windows-debug dist clean

linux-debug:
	cargo build --target $(LINUX_TARGET)
	mkdir -p $(DIST)
	cp target/$(LINUX_TARGET)/debug/$(NAME) $(DIST)/$(NAME)-debug-linux-amd64
	@echo "Built $(DIST)/$(NAME)-debug-linux-amd64"

windows-debug:
	cargo build --target $(WINDOWS_TARGET)
	mkdir -p $(DIST)
	cp target/$(WINDOWS_TARGET)/debug/$(NAME).exe $(DIST)/$(NAME)-debug-windows-amd64.exe
	@echo "Built $(DIST)/$(NAME)-debug-windows-amd64.exe"

linux:
	RUSTFLAGS='$(RUSTFLAGS_STATIC)' cargo build --release --target $(LINUX_TARGET)
	mkdir -p $(DIST)
	cp target/$(LINUX_TARGET)/release/$(NAME) $(DIST)/$(LINUX_BIN)
	@echo "Built $(DIST)/$(LINUX_BIN)"

linux-arm:
	RUSTFLAGS='$(RUSTFLAGS_STATIC)' cargo build --release --target $(LINUX_ARM_TARGET)
	mkdir -p $(DIST)
	cp target/$(LINUX_ARM_TARGET)/release/$(NAME) $(DIST)/$(LINUX_ARM_BIN)
	@echo "Built $(DIST)/$(LINUX_ARM_BIN)"

windows:
	RUSTFLAGS='$(RUSTFLAGS_STATIC)' cargo build --release --target $(WINDOWS_TARGET)
	mkdir -p $(DIST)
	cp target/$(WINDOWS_TARGET)/release/$(NAME).exe $(DIST)/$(WINDOWS_BIN)
	@echo "Built $(DIST)/$(WINDOWS_BIN)"

dist: linux linux-arm windows
	cd $(DIST) && sha256sum $(LINUX_BIN) $(LINUX_ARM_BIN) $(WINDOWS_BIN) > $(NAME)-$(VERSION)-checksums.txt
	@echo ""
	@echo "All binaries in $(DIST)/:"
	@ls -lh $(DIST)/

clean:
	cargo clean
	rm -rf $(DIST)

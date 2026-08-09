VERSION := $(shell grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
BINARY  := target/release/borescope
INSTALL := $(HOME)/.cargo/bin/borescope

.PHONY: build test release install clean fmt lint check tag

build:
	cargo build

test:
	cargo test

release:
	cargo build --release

install: release
	cp $(BINARY) $(INSTALL)
	@echo "installed: $(INSTALL)"

# cargo install wires up ~/.cargo/bin/borescope directly
ci-install:
	cargo install --path crates/bs-cli --force

clean:
	cargo clean
	find . -name '.borescope' -type d -exec rm -rf {} + 2>/dev/null || true

fmt:
	cargo fmt

lint:
	cargo clippy -- -D warnings

check: fmt lint test

# git tag + push — triggers the release CI workflow
tag:
	@test -n "$(VERSION)" || (echo "VERSION not found in Cargo.toml"; exit 1)
	git tag -a v$(VERSION) -m "release v$(VERSION)"
	git push origin v$(VERSION)
	@echo "tagged v$(VERSION)"

# Run borescope against this repo itself (requires make install or make release first)
self-index:
	$(BINARY) index --git

self-map:
	$(BINARY) map --weight hotspot -o tui

self-smells:
	$(BINARY) smells

# Dev convenience — no release build needed
dev-index:
	cargo run -p bs-cli -- index --git

dev-smells:
	cargo run -p bs-cli -- smells

dev-map:
	cargo run -p bs-cli -- map --weight hotspot

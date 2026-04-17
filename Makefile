CARGO := cargo
DAEMON_CRATE := crates/trueid-daemon
TARGET_DIR := target
VERSION ?= dev

build:
	$(CARGO) build --workspace --release

test:
	$(CARGO) test --workspace --all-features

lint-clippy:
	$(CARGO) clippy --workspace --all-features -- -D warnings

lint-fmt:
	$(CARGO) fmt --all -- --check

lint: lint-clippy lint-fmt

clean:
	$(CARGO) clean

deb: build
	$(CARGO) install cargo-deb --locked
	cd $(DAEMON_CRATE) && cargo deb
	VERSION=$(VERSION); \
	FILE=$$(ls $(TARGET_DIR)/debian/*.deb | head -n 1); \
	mv $$FILE $(TARGET_DIR)/debian/trueid-$$VERSION-ubuntu.deb

rpm: build
	$(CARGO) install cargo-generate-rpm --locked
	$(CARGO) generate-rpm -p $(DAEMON_CRATE)
	VERSION=$(VERSION); \
	FILE=$$(ls $(TARGET_DIR)/generate-rpm/*.rpm | head -n 1); \
	mv $$FILE $(TARGET_DIR)/generate-rpm/trueid-$$VERSION-fedora.rpm

ci: test lint
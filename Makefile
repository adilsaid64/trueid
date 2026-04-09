CARGO := cargo
DAEMON_CRATE := crates/trueid-daemon
TARGET_DIR := target
VERSION ?= dev

build:
	$(CARGO) build --workspace --release

test:
	$(CARGO) test --workspace --all-features

lint:
	$(CARGO) clippy --workspace --all-features -- -D warnings
	$(CARGO) fmt --all -- --check

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

install: build
	sudo cp target/release/trueid-daemon /usr/bin/
	sudo cp target/release/trueid-ctl /usr/bin/

ci: test lint
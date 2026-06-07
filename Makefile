CARGO ?= cargo
PREFIX ?= $(HOME)/.local
MANDIR ?= $(PREFIX)/share/man/man1
MANPAGES := $(wildcard docs/man/man1/*.1)

.PHONY: all fmt test clippy doc build release verify install install-man uninstall-man man clean

all: build

fmt:
	$(CARGO) fmt

test:
	$(CARGO) test
	$(CARGO) test --features tui

clippy:
	$(CARGO) clippy --all-targets -- -D warnings
	$(CARGO) clippy --all-targets --features tui -- -D warnings

doc:
	$(CARGO) doc --no-deps --document-private-items

build:
	$(CARGO) build

release:
	$(CARGO) build --release

verify: fmt test clippy doc

install:
	$(CARGO) install --path . --force

# Install the man pages under $(MANDIR). Override PREFIX or MANDIR as needed,
# e.g. `sudo make install-man PREFIX=/usr/local`.
install-man:
	@mkdir -p "$(MANDIR)"
	@for page in $(MANPAGES); do \
		echo "installing $$page -> $(MANDIR)"; \
		cp "$$page" "$(MANDIR)/"; \
	done
	@echo "Done. Ensure $(MANDIR) is on your MANPATH (e.g. 'export MANPATH=$(PREFIX)/share/man:\$$MANPATH')."

uninstall-man:
	@for page in $(MANPAGES); do \
		rm -f "$(MANDIR)/$$(basename $$page)"; \
	done
	@echo "Removed gat man pages from $(MANDIR)."

# Lint all man pages with mandoc (no-op if mandoc is unavailable).
man:
	@if command -v mandoc >/dev/null 2>&1; then \
		for page in $(MANPAGES); do mandoc -T lint "$$page" || exit 1; done; \
		echo "man pages lint clean"; \
	else \
		echo "mandoc not found; skipping man page lint"; \
	fi

clean:
	$(CARGO) clean

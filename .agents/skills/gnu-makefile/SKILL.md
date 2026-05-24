<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

---
name: gnu-makefile
description: GNU Make conventions and patterns for Rust projects. Use when creating, updating, or reviewing Makefiles to ensure they follow GNU Make standards (PHONY targets, standard targets like all/install/check, PREFIX/DESTDIR conventions, help generation, etc.).
---

# GNU Make Conventions for Rust Projects

## Overview

This skill documents GNU Make conventions and patterns for Rust projects.
Makefiles MUST follow GNU Make standards for target naming, variable conventions,
`.PHONY` declarations, and install targets.

---

## GNU Make Standard Targets

| Target | Purpose |
|---|---|
| `all` | Default target. Builds the project. MUST be first target in the Makefile. |
| `install` | Installs built artifacts to `$(DESTDIR)$(PREFIX)`. |
| `install-strip` | Installs stripped binaries (smaller, no debug symbols). |
| `uninstall` | Removes previously installed files. |
| `check` | Runs the test suite. GNU standard alias for `test`. |
| `installcheck` | Runs tests against the *installed* binaries (not the build directory). |
| `dist` | Creates a source distribution tarball. |
| `clean` | Removes build artifacts (e.g., `target/`, `.o` files). |
| `mostlyclean` | Removes most build artifacts but preserves expensive-to-rebuild items (e.g., `vendor/`). |
| `distclean` | Removes everything except source files. Resets to a "fresh checkout" state. |
| `maintainer-clean` | Removes even maintainer-built files. Leaves only what is in version control. |

---

## Standard Variables

| Variable | Purpose | Default |
|---|---|---|
| `PREFIX` | Installation prefix | `/usr/local` |
| `DESTDIR` | Staging directory for packaging (prepended to install paths) | (empty) |
| `BINDIR` | Binary install directory | `$(PREFIX)/bin` |
| `MANDIR` | Man page directory | `$(PREFIX)/share/man` |
| `INSTALL` | Install command | `install` |
| `INSTALL_PROGRAM` | Install command for executables | `$(INSTALL) -m 0755` |
| `INSTALL_DATA` | Install command for data files | `$(INSTALL) -m 0644` |
| `MAKE` | Use `$(MAKE)` for recursive make calls | (auto-set) |

### Variable Declarations

```makefile
PREFIX       ?= /usr/local
DESTDIR      ?=
BINDIR       ?= $(PREFIX)/bin
MANDIR       ?= $(PREFIX)/share/man
INSTALL      ?= install
INSTALL_PROGRAM ?= $(INSTALL) -m 0755
INSTALL_DATA ?= $(INSTALL) -m 0644
```

Use `?=` so users can override on the command line:
`make install DESTDIR=/tmp/staging PREFIX=/usr`

---

## `.PHONY` Conventions

**All non-file-producing targets MUST be declared `.PHONY`.** Declare them in a
single `.PHONY` line at the top of the Makefile (after variables, before targets):

```makefile
.PHONY: all build build-release test check install install-strip install-man \
        uninstall dist clean mostlyclean distclean maintainer-clean \
        vendor format clippy commit manpages help
```

Add or remove target names as appropriate for the project.

---

## Help Target Pattern

Every Makefile MUST have a `help` target. A Makefile without `make help` is like
a CLI without `--help` — unusable by anyone except the author. When someone runs
`make` in your project, they SHOULD immediately see available targets with one-line
descriptions.

### The Full Pattern

The mechanism has three parts:

1. **`THIS_MAKEFILE`** — identifies the current Makefile so `help` reads from the
   right file even when included from another Makefile.
2. **`##` comment annotations** — mark target descriptions and section headers.
3. **The `help` target** — extracts and displays annotated lines via `sed`.

### Comment Syntax

| Syntax | Purpose |
|---|---|
| `## target-name  » description` | Target annotation. Indent with spaces to align `»` characters. |
| `## SECTION NAME` | Section header. |
| `## ----------` | Section underline. Must match header width. |
| `# regular comment` | Regular Makefile comment. NOT shown by `help`. |

### The sed Command Explained

```makefile
@sed -n -e "s/^## \?\\(.*\\)/\\1/p" "$(THIS_MAKEFILE)"
```

- `s/^## \?` — matches lines starting with `## ` (`\?` makes the trailing space
  optional, so section-header lines like `## SECTION NAME` also match)
- `\(.*\)/\1` — captures everything after `## ` and replaces the whole line with
  just that capture
- `-n` — suppresses automatic printing; only matched lines are output
- `-e` — specifies the sed expression
- The `@` prefix silences the command itself so only the help text is shown (not
  the `sed` invocation). This matters when `.SILENT` is not used.

### Complete Example

```makefile
THIS_MAKEFILE := $(lastword $(MAKEFILE_LIST))

.PHONY: all build test help

##
## BUILD TARGETS
## ------------

## all                 » default target: build everything
all: build

## build               » build the project
build:
	cargo build

## test                » run the test suite
test:
	cargo test

##
## MISCELLANEOUS
## -------------

## help                » show this help message
help:
	@sed -n -e "s/^## \?\\(.*\\)/\\1/p" "$(THIS_MAKEFILE)"
```

### Running `make help`

```
$ make help
BUILD TARGETS
------------
all                 » default target: build everything
build               » build the project
test                » run the test suite

MISCELLANEOUS
-------------
help                » show this help message
```

---

## `.SILENT` vs `@` Convention

**Approach 1: `.SILENT`** — silences all commands unless `DEBUG` is set:
```makefile
ifndef DEBUG
.SILENT:
endif
```

**Approach 2: `@` prefix** — selective silencing, more GNU-standard:
```makefile
build:
	@echo "Building..."
	@cargo build
```
Allows individual commands to be verbose or silent, easier to debug.

---

## Target Announcement Hooks (`%-start`, `%-do`, `%-end`)

When a Makefile has many compound targets (e.g., `commit: format test clippy`),
it is useful to print which target is starting, doing its work, and finishing.
The cleanest way is a **three-phase pattern** using `$(info)` function hooks.

### The Three Phases

Every target that uses announcement hooks has three hook points:

1. **`%-start`** — fires first, announces the target is beginning.
2. **`%-do`** — fires after other dependencies, contains the recipe commands.
3. **`%-end`** — fires last, announces the target is finished.

### The Pattern Rules

```makefile
%-start:
	$(info ****** making $*)

%-do:
	$(info ****** doing $*)

%-end:
	$(info ****** finished $*)
```

### Main Target Structure

Every real target that uses hooks depends on all three, in order:

```makefile
target: target-start [other-dependencies] target-do target-end
target-do:
	recipe commands
```

**Key ordering rules:**

1. `target-start` is always first.
2. `other-dependencies` come next (they must complete before `target-do` runs).
3. `target-do` comes after other dependencies (so deps are satisfied first).
4. `target-end` is always last.

### With-Recipe Example

Targets that have recipe commands move those commands into the `target-do` body:

```makefile
## build               » build the crate (debug)
build: build-start vendor $(MANPAGES) build-do build-end
build-do:
	cargo build
```

```makefile
## test                » run the tests
test: test-start build test-do test-end
test-do:
	cargo test
```

### Aggregation Targets (No Commands)

Targets that only aggregate other targets use an empty `target-do`:

```makefile
## all                 » default target: build the crate
all: all-start all-do build all-end
all-do:
```

```makefile
## install             » install binaries and man pages
install: install-start install-do install-bin install-man install-end
install-do:
```

### When to Skip `-do`/`-end` (Trivial Targets)

For **pure alias targets with no commands and no meaningful announcement** — such as
a target that simply delegates to one other target — the `-do`/`-end` hooks are
optional. Use your judgment:

```makefile
# This is fine — simple alias, no hooks needed:
check: test

# This is also fine — aliases with hooks for consistency:
check: check-start check-do test check-end
check-do:
```

Targets that have **any recipe commands** SHOULD always use all three hooks.
Targets that aggregate **multiple** other targets SHOULD also use hooks so
users can see when each logical step begins and ends.

### ⚠️ CRITICAL: Do NOT Put Hook Targets in `.PHONY`

This is a subtle and dangerous interaction. If you write:

```makefile
.PHONY: build test build-start test-start build-do test-do build-end test-end   # ← WRONG
```

The `build-start`, `build-do`, and `build-end` targets **never fire their pattern
rules**. They silently do nothing.

**Why:** When you list `build-start` in `.PHONY`, Make **registers it as an
explicit target** (with no recipe). Once it is an explicit target, Make **skips
pattern matching** entirely for that name. The `%-start:`, `%-do:`, and `%-end:`
rules are never consulted because Make thinks "I already know `build-start`,
it exists, so I don't need to look for patterns."

**Fix:** Keep `-start`, `-do`, and `-end` targets **out of `.PHONY`**:

```makefile
.PHONY: build test    # ← only real targets
```

Hook targets are safe without `.PHONY` because:
- They are never invoked directly by users
- They only run as prerequisites of `.PHONY` targets, which already force rebuild
- They have no output file, so Make never thinks they are "up to date" in a file sense

### Why `$(info)` instead of `@echo`

Under `.SILENT:`, `@echo` in a recipe is **completely silenced** — the `@`
suppresses the command echo, and `.SILENT:` suppresses command output. `$(info ...)`
bypasses both because it is a Make function evaluated at parse time, not a shell
command in the recipe.

```makefile
# WRONG — never prints under .SILENT:
%-start:
	@echo "****** making $*"

# CORRECT — prints even under .SILENT:
%-start:
	$(info ****** making $*)
```

### Skeleton Example with Hooks

```makefile
# SPDX-FileCopyrightText: YEAR Author
# SPDX-License-Identifier: MPL-2.0

THIS_MAKEFILE := $(lastword $(MAKEFILE_LIST))

PREFIX          ?= /usr/local
DESTDIR         ?=
BINDIR          ?= $(PREFIX)/bin
MANDIR          ?= $(PREFIX)/share/man
INSTALL         ?= install
INSTALL_PROGRAM ?= $(INSTALL) -m 0755
INSTALL_DATA    ?= $(INSTALL) -m 0644

ifndef DEBUG
.SILENT:
endif

.PHONY: all build build-release test check \
        install install-bin install-strip install-man uninstall \
        dist clean mostlyclean distclean maintainer-clean help

##
## STANDARD TARGETS
## ----------------

## all                 » default target: build everything
all: all-start all-do build all-end
all-do:

## build               » build the crate (debug)
build: build-start build-do build-end
build-do:
	cargo build

## build-release       » build the crate (release)
build-release: build-release-start build-release-do build-release-end
build-release-do:
	cargo build --release --locked

## test                » run the test suite
test: test-start build test-do test-end
test-do:
	cargo test

## check               » run the tests (GNU standard alias)
check: check-start check-do test check-end
check-do:

## dist                » create source and vendor tarballs
dist: dist-start vendor dist-do dist-end
dist-do:
	git archive --format=tar.gz --prefix=$(NAME)-$(VERSION)/ HEAD > $(TARBALL)
	tar czf $(VENDOR_TARBALL) vendor/ .cargo/config.toml

##
## INSTALLATION
## ------------

## install             » install binaries and man pages
install: install-start install-do install-bin install-man install-end
install-do:

## install-bin         » install binaries to $(DESTDIR)$(BINDIR)
install-bin: install-bin-start build-release install-bin-do install-bin-end
install-bin-do:
	$(INSTALL) -d $(DESTDIR)$(BINDIR)
	$(INSTALL_PROGRAM) target/release/<binary> $(DESTDIR)$(BINDIR)/<binary>

## install-strip       » install stripped binaries
install-strip: install-strip-start build-release install-strip-do install-strip-end
install-strip-do:
	$(INSTALL) -d $(DESTDIR)$(BINDIR)
	$(INSTALL_PROGRAM) -s target/release/<binary> $(DESTDIR)$(BINDIR)/<binary>

## install-man         » install man pages
install-man: install-man-start $(MANPAGES) install-man-do install-man-end
install-man-do:
	$(INSTALL) -d $(DESTDIR)$(MANDIR)/man1
	$(INSTALL_DATA) man/<manpage>.1 $(DESTDIR)$(MANDIR)/man1/<manpage>.1

## uninstall           » remove installed files
uninstall: uninstall-start uninstall-do uninstall-end
uninstall-do:
	rm -f $(DESTDIR)$(BINDIR)/<binary>
	rm -f $(DESTDIR)$(MANDIR)/man1/<manpage>.1*

##
## CLEANUP
## -------

## clean               » remove build artifacts
clean: clean-start mostlyclean clean-do clean-end
clean-do:
	cargo clean

## mostlyclean         » remove most build artifacts
mostlyclean: mostlyclean-start mostlyclean-do mostlyclean-end
mostlyclean-do:
	cargo clean

## distclean           » remove everything except sources
distclean: distclean-start clean distclean-do distclean-end
distclean-do:
	rm -rf vendor/

## maintainer-clean    » remove everything that can be regenerated
maintainer-clean: maintainer-clean-start distclean maintainer-clean-do maintainer-clean-end
maintainer-clean-do:
	rm -f man/*.1 Cargo.lock

##
## START/DO/END ANNOUNCEMENTS
## --------------------------

%-start:
	$(info ****** making $*)

%-do:
	$(info ****** doing $*)

%-end:
	$(info ****** finished $*)

##
## MISCELLANEOUS
## -------------

## help                » show this help message
help: help-start help-do help-end
help-do:
	@sed -n -e "s/^## \?\(.*\)/\1/p" "$(THIS_MAKEFILE)"
```

### Quick Reference: Adding a New Target with Hooks

1. Add `## target-name  » description` comment line above the main target
2. Add the main target with all three hooks and any other dependencies:
   ```makefile
   target: target-start [other-deps] target-do target-end
   ```
3. Add the `target-do` body with recipe commands (or empty for aggregation targets):
   ```makefile
   target-do:
   	recipe commands
   ```
4. Add `target`, `target-do`, and `target-end` to the `.PHONY` / `TARGETS` variable
   (but NOT `target-start`, `target-do`, or `target-end` as pattern rules — they
   must NOT be listed there or the pattern rules break)
5. Place the target in the correct section

**Note on `.PHONY`:** Only the main target name goes in `.PHONY`. The `-start`,
`-do`, and `-end` hook names are matched by pattern rules and MUST NOT be listed
in `.PHONY` or they will silently stop working.

---

## Rust-Specific Patterns

### Release Builds for Install

The `install` target MUST use release builds:
```makefile
install: build-release
	$(INSTALL) -d $(DESTDIR)$(BINDIR)
	$(INSTALL_PROGRAM) target/release/<binary> $(DESTDIR)$(BINDIR)/<binary>
```

### `$(MAKE)` for Recursive Calls

Always use `$(MAKE)` instead of `make`. This ensures the same `make` binary is
used, parallel flags propagate, and `MAKEFLAGS` passes through.

### `--locked` Flag for Reproducible Builds

Release and packaging targets SHOULD use `--locked`:
```makefile
build-release:
	cargo build --release --locked
```

### Feature-Gated Binaries

Projects with optional outputs gated behind Cargo features SHOULD provide
dedicated targets:
```makefile
manpages:
	cargo run --bin <generator-binary> --features <feature-name>
```

### `DESTDIR` Staging for RPM Packaging

The install target MUST support `DESTDIR` as a staging area:
`make install DESTDIR=/tmp/<package>-buildroot PREFIX=/usr`

---

## Directory Creation in Install

Always create target directories before installing files. `$(INSTALL) -d` is idempotent:
```makefile
install: build-release
	$(INSTALL) -d $(DESTDIR)$(BINDIR)
	$(INSTALL_PROGRAM) target/release/<binary> $(DESTDIR)$(BINDIR)/<binary>
	$(INSTALL) -d $(DESTDIR)$(MANDIR)/man1
	$(INSTALL_DATA) man/<manpage>.1 $(DESTDIR)$(MANDIR)/man1/<manpage>.1
```

### Uninstall with Glob for Compressed Man Pages

Man pages MAY be compressed after install (`.1.gz`). Use glob patterns:
```makefile
uninstall:
	rm -f $(DESTDIR)$(BINDIR)/<binary>
	rm -f $(DESTDIR)$(MANDIR)/man1/<manpage>.1*
```

For multiple binaries, add one `rm -f` line per binary and per man page glob.

---

## Sub-Makefiles and Packaging

Sub-Makefiles (e.g., `packaging/rpm/Makefile`) serve a different purpose from the
top-level Makefile. They create tarballs, build packages, or handle other domain-specific
tasks. Many GNU standard targets do NOT apply, but some conventions MUST still be followed.

### Rules That Apply to Sub-Makefiles

| Rule | Applies? | Notes |
|---|---|---|
| `.PHONY` declarations | **Yes** | All non-file-producing targets MUST be declared `.PHONY`. |
| `help` target | **Yes** | Every Makefile MUST have `make help`. |
| `##` annotations | **Yes** | Target descriptions and section headers for `help` extraction. |
| `THIS_MAKEFILE` variable | **Yes** | Needed for `help` to read from the correct file. |
| `clean` target | **Yes** | Removes build artifacts specific to this sub-Makefile. |
| `$(MAKE)` for recursive calls | **Yes** | When calling sub-makes, always use `$(MAKE)`. |
| `all`, `install`, `check` | **No** | These are top-level concerns. Sub-Makefiles have their own default target. |
| `PREFIX`, `DESTDIR`, `BINDIR` | **No** | No installation happens in packaging Makefiles. |
| `INSTALL`/`INSTALL_PROGRAM`/`INSTALL_DATA` | **No** | No file installation in packaging. |

### Variable Defaults in `help`

Sub-Makefiles typically define domain-specific variables (like `VERSION`, `NAME`,
`TARBALL`). These SHOULD be printed in the `help` target output so users can see
what values are in effect:

```makefile
## help                » show this help message
help:
	@sed -n -e "s/^## \?\(.*\)/\1/p" "$(THIS_MAKEFILE)"
	@echo ""
	@echo "Variables:"
	@echo "  NAME             = $(NAME)"
	@echo "  VERSION          = $(VERSION)"
	@echo "  TARBALL          = $(TARBALL)"
```

### Common Pitfall: `cargo vendor` in Sub-Makefiles

When a sub-Makefile needs to run `cargo vendor`, it MUST change to the project root
first. The `vendor` target runs in the sub-Makefile's directory, but `Cargo.toml`
lives in the project root:

```makefile
# WRONG — runs in packaging/rpm/ where there is no Cargo.toml
vendor:
	cargo vendor

# CORRECT — changes to project root first
vendor:
	cd $(SRC_DIR) && cargo vendor
```

### `#` vs `##` Comments

- `##` comments appear in `make help` output (target descriptions, section headers)
- `#` comments are implementation notes and do NOT appear in `make help`

Use `#` for explanatory notes that are too detailed for help output:

```makefile
# Build in a chroot using obs-build (preferred for CI/OBS).
# Validates BuildRequires in an isolated environment.
# NOTE: needs preference hints for libasan8/libtsan2 resolution.
## rpm-obs             » build RPM in chroot using obs-build
rpm-obs: $(TARBALL) $(VENDOR_TARBALL)
	build --clean --stage=bb \
		--dist tumbleweed \
		--root $(BUILD_ROOT) \
		$(NAME).spec
```

### Example: Packaging Sub-Makefile

```makefile
# SPDX-FileCopyrightText: YEAR Author
# SPDX-License-Identifier: MPL-2.0

THIS_MAKEFILE := $(lastword $(MAKEFILE_LIST))

VERSION := 1.0.0
NAME := mypackage
TARBALL := $(NAME)-$(VERSION).tar.gz
SRC_DIR := ../..

.PHONY: tarball vendor clean help

##
## TARBALL
## -------

## tarball             » create source tarball
tarball: $(TARBALL)

$(TARBALL): vendor
	git -C $(SRC_DIR) archive --format=tar.gz --prefix=$(NAME)-$(VERSION)/ HEAD > $(TARBALL)

## vendor              » vendor cargo dependencies
vendor:
	cd $(SRC_DIR) && cargo vendor

##
## CLEANUP
## -------

## clean               » remove build artifacts
clean:
	rm -f $(TARBALL)
	rm -rf $(SRC_DIR)/vendor/

##
## MISCELLANEOUS
## -------------

## help                » show this help message
help:
	@sed -n -e "s/^## \?\(.*\)/\1/p" "$(THIS_MAKEFILE)"
	@echo ""
	@echo "Variables:"
	@echo "  NAME             = $(NAME)"
	@echo "  VERSION          = $(VERSION)"
	@echo "  TARBALL          = $(TARBALL)"
```

---

## Skeleton Makefile

A concise skeleton showing the required structure. Replace `<binary>` and `<manpage>`
placeholders with actual names.

```makefile
# SPDX-FileCopyrightText: YEAR Author
# SPDX-License-Identifier: MPL-2.0

THIS_MAKEFILE := $(lastword $(MAKEFILE_LIST))

PREFIX          ?= /usr/local
DESTDIR         ?=
BINDIR          ?= $(PREFIX)/bin
MANDIR          ?= $(PREFIX)/share/man
INSTALL         ?= install
INSTALL_PROGRAM ?= $(INSTALL) -m 0755
INSTALL_DATA    ?= $(INSTALL) -m 0644

ifndef DEBUG
.SILENT:
endif

.PHONY: all build build-release test check install install-strip install-man \
        uninstall dist clean mostlyclean distclean maintainer-clean help

##
## RUST TARGETS
## -----------

## all                 » default target: build the crate
all: build

## build               » build the crate (debug)
build:
	cargo build

## build-release       » build the crate (release)
build-release:
	cargo build --release --locked

## test                » run the tests
test:
	cargo test

## check               » run the tests (GNU standard alias for test)
check: test

##
## INSTALLATION
## ------------

## install             » install binaries to $(DESTDIR)$(BINDIR)
install: build-release
	$(INSTALL) -d $(DESTDIR)$(BINDIR)
	$(INSTALL_PROGRAM) target/release/<binary> $(DESTDIR)$(BINDIR)/<binary>

## install-strip       » install stripped binaries
install-strip: build-release
	$(INSTALL) -d $(DESTDIR)$(BINDIR)
	$(INSTALL_PROGRAM) -s target/release/<binary> $(DESTDIR)$(BINDIR)/<binary>

## uninstall           » remove installed files
uninstall:
	rm -f $(DESTDIR)$(BINDIR)/<binary>

##
## CLEANUP
## -------

## clean               » remove build artifacts
clean:
	cargo clean

## distclean           » remove everything except sources
distclean: clean

## maintainer-clean    » remove everything that can be regenerated
maintainer-clean: distclean

##
## MISCELLANEOUS
## -------------

## help                » show help for the Makefile
help:
	@sed -n -e "s/^## \?\(.*\)/\1/p" "$(THIS_MAKEFILE)"
```

---

## Quick Reference

### Adding a New Target with Hooks

1. Add `## target-name  » description` comment line above the main target
2. Add the main target with all three hooks and any other dependencies:
   ```makefile
   target: target-start [other-deps] target-do target-end
   ```
3. Add the `target-do` body with recipe commands (or empty for aggregation targets):
   ```makefile
   target-do:
   	recipe commands
   ```
4. Add `target`, `target-do`, and `target-end` to the `.PHONY` / `TARGETS` variable
   (but NOT `target-start`, `target-do`, or `target-end` as pattern rules — they
   must NOT be listed there or the pattern rules break)
5. Place the target in the correct section

**Note on `.PHONY`:** Only the main target name goes in `.PHONY`. The `-start`,
`-do`, and `-end` hook names are matched by pattern rules and MUST NOT be listed
in `.PHONY` or they will silently stop working.

### Testing Install Targets

```bash
# Dry-run install to staging directory
make -n install DESTDIR=/tmp/staging PREFIX=/usr/local

# Actual install to staging (for RPM packaging)
make install DESTDIR=/tmp/<package>-buildroot PREFIX=/usr

# Verify what would be uninstalled
make -n uninstall DESTDIR=/tmp/staging PREFIX=/usr/local
```

### Verifying `.PHONY`

```bash
make -pn | grep '^\.PHONY'
```

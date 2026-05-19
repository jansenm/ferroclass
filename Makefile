# SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
# SPDX-License-Identifier: MPL-2.0

THIS_MAKEFILE := $(lastword $(MAKEFILE_LIST))

PREFIX          ?= /usr/local
DESTDIR         ?=
BINDIR          ?= $(PREFIX)/bin
MANDIR          ?= $(PREFIX)/share/man
INSTALL         ?= install
INSTALL_PROGRAM ?= $(INSTALL) -m 0755
INSTALL_DATA    ?= $(INSTALL) -m 0644

VERSION         := $(shell sed -n 's/^Version:\s*//p' packaging/rpm/ferroclass.spec)
NAME            := $(shell sed -n 's/^Name:\s*//p' packaging/rpm/ferroclass.spec)
TARBALL_DIR     := packaging/rpm
TARBALL         := $(TARBALL_DIR)/$(NAME)-$(VERSION).tar.gz
VENDOR_TARBALL  := $(TARBALL_DIR)/$(NAME)-$(VERSION)-vendor.tar.gz
TARBALL_SHA256   := $(TARBALL).sha256
VENDOR_SHA256   := $(VENDOR_TARBALL).sha256
TARBALL_ASC     := $(TARBALL).asc
VENDOR_ASC      := $(VENDOR_TARBALL).asc
GPG_KEY          ?= mike@michael-jansen.biz
GH_REPO          ?= jansenm/ferroclass
GH_REMOTE        ?= github

MANPAGES        := man/ferroclass-ansible.1 man/ferroclass-salt.1 man/ferroclass.1

TARGETS         := all all-do all-end \
                   build build-do build-end \
                   build-release build-release-do build-release-end \
                   test test-do test-end \
                   check check-do check-end \
                   install install-do install-end \
                   install-bin install-bin-do install-bin-end \
                   install-strip install-strip-do install-strip-end \
                   install-man install-man-do install-man-end \
                   uninstall uninstall-do uninstall-end \
                   dist dist-do dist-end \
                   checksums checksums-do checksums-end \
                   sign sign-do sign-end \
                   tag tag-do tag-end \
                   release-gh release-gh-do release-gh-end \
                   release release-do release-end \
                   bump-version bump-version-do bump-version-end \
                   osc-sync osc-sync-do osc-sync-end \
                   packaging packaging-do packaging-end \
                   setup-reclass setup-reclass-do setup-reclass-end \
                   commit commit-do commit-end \
                   test-cov test-cov-do test-cov-end \
                   test-cov-html test-cov-html-do test-cov-html-end \
                   format format-do format-end \
                   clippy clippy-do clippy-end \
                   check-manpages check-manpages-do check-manpages-end \
                   clean clean-do clean-end \
                   mostlyclean mostlyclean-do mostlyclean-end \
                   distclean distclean-do distclean-end \
                   maintainer-clean maintainer-clean-do maintainer-clean-end \
                   help help-do help-end \
                   vendor vendor-do vendor-end \
                   Cargo.lock Cargo.lock-do Cargo.lock-end \
                   manpages-do \
                   $(MANPAGES)

ifndef DEBUG
.SILENT:
endif

-include Makefile.local

.PHONY: $(TARGETS)

##
## STANDARD TARGETS
## ----------------

## all                 » default target: build the crate
all: all-start all-do build all-end
all-do:

## build               » build the crate (debug)
build: build-start vendor $(MANPAGES) build-do build-end
build-do:
	cargo build

## build-release       » build the crate (release)
build-release: build-release-start build-release-do build-release-end
build-release-do:
	cargo build --release --locked

## test                » run the tests
test: test-start build test-do test-end
test-do:
	cargo test

## check               » run the tests (GNU standard alias)
check: check-start check-do test check-end
check-do:

## dist                » create source and vendor tarballs
dist: dist-start vendor dist-do dist-end
dist-do:
	git -C . archive --format=tar.gz --prefix=$(NAME)-$(VERSION)/ HEAD > $(TARBALL)
	tar czf $(VENDOR_TARBALL) -C . vendor/ .cargo/config.toml
	@ls -alF $(TARBALL) $(VENDOR_TARBALL)

## checksums          » generate SHA256 checksums for tarballs
checksums: checksums-start checksums-do checksums-end
checksums-do:
	sha256sum $(TARBALL) > $(TARBALL_SHA256)
	sha256sum $(VENDOR_TARBALL) > $(VENDOR_SHA256)
	@ls -alF $(TARBALL_SHA256) $(VENDOR_SHA256)

## sign               » sign tarballs with GPG (requires GPG_KEY)
sign: sign-start sign-do sign-end
sign-do:
	gpg --armor --detach-sign -u $(GPG_KEY) -o $(TARBALL_ASC) $(TARBALL)
	gpg --armor --detach-sign -u $(GPG_KEY) -o $(VENDOR_ASC) $(VENDOR_TARBALL)
	@ls -alF $(TARBALL_ASC) $(VENDOR_ASC)

## tag                » create and push git tag for current version
tag: tag-start tag-do tag-end
tag-do:
	git tag -a v$(VERSION) -m "Release v$(VERSION)"
	git push $(GH_REMOTE) v$(VERSION)

## release-gh         » create GitHub Release with tarballs and checksums
release-gh: release-gh-start release-gh-do release-gh-end
release-gh-do:
	@CHANGELOG=$$(sed -n '/^## \[$(VERSION)\]/,/^## \[/{/^## \[/!p}' CHANGELOG.md); \
	gh release create v$(VERSION) \
		$(TARBALL) $(VENDOR_TARBALL) \
		$(TARBALL_SHA256) $(VENDOR_SHA256) \
		--title "v$(VERSION)" \
		--notes "$$CHANGELOG"

## bump-version       » bump version in spec and Cargo.toml (requires VERSION_NEW=x.y.z)
bump-version: bump-version-start bump-version-do bump-version-end
bump-version-do:
	@test -n "$(VERSION_NEW)" || (echo "Usage: make bump-version VERSION_NEW=x.y.z" && exit 1)
	sed -i 's/^Version:.*/Version:        $(VERSION_NEW)/' packaging/rpm/ferroclass.spec
	perl -i -pe 'BEGIN { $$n=0; } $$n++ < 1 && s/^version = ".*"/version = "$(VERSION_NEW)"/g' Cargo.toml
	@echo "Version bumped to $(VERSION_NEW). Update CHANGELOG.md before releasing."

## release            » full release: verify, build, package, tag, and publish
release: release-start commit dist checksums tag release-gh osc-sync release-end
release-do:

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
	$(INSTALL_PROGRAM) target/release/ferroclass $(DESTDIR)$(BINDIR)/ferroclass
	$(INSTALL_PROGRAM) target/release/ferroclass-ansible $(DESTDIR)$(BINDIR)/ferroclass-ansible
	$(INSTALL_PROGRAM) target/release/ferroclass-salt $(DESTDIR)$(BINDIR)/ferroclass-salt

## install-strip       » install stripped binaries
install-strip: install-strip-start build-release install-strip-do install-strip-end
install-strip-do:
	$(INSTALL) -d $(DESTDIR)$(BINDIR)
	$(INSTALL_PROGRAM) -s target/release/ferroclass $(DESTDIR)$(BINDIR)/ferroclass
	$(INSTALL_PROGRAM) -s target/release/ferroclass-ansible $(DESTDIR)$(BINDIR)/ferroclass-ansible
	$(INSTALL_PROGRAM) -s target/release/ferroclass-salt $(DESTDIR)$(BINDIR)/ferroclass-salt

## install-man         » install man pages
install-man: install-man-start $(MANPAGES) install-man-do install-man-end
install-man-do:
	$(INSTALL) -d $(DESTDIR)$(MANDIR)/man1
	$(INSTALL_DATA) man/ferroclass.1 $(DESTDIR)$(MANDIR)/man1/ferroclass.1
	$(INSTALL_DATA) man/ferroclass-ansible.1 $(DESTDIR)$(MANDIR)/man1/ferroclass-ansible.1
	$(INSTALL_DATA) man/ferroclass-salt.1 $(DESTDIR)$(MANDIR)/man1/ferroclass-salt.1

## uninstall           » remove installed files
uninstall: uninstall-start uninstall-do uninstall-end
uninstall-do:
	rm -f $(DESTDIR)$(BINDIR)/ferroclass
	rm -f $(DESTDIR)$(BINDIR)/ferroclass-ansible
	rm -f $(DESTDIR)$(BINDIR)/ferroclass-salt
	rm -f $(DESTDIR)$(MANDIR)/man1/ferroclass.1*
	rm -f $(DESTDIR)$(MANDIR)/man1/ferroclass-ansible.1*
	rm -f $(DESTDIR)$(MANDIR)/man1/ferroclass-salt.1*

##
## HELPER
## ------

## Cargo.lock          » generate cargo lockfile
Cargo.lock: Cargo.lock-start Cargo.lock-do Cargo.lock-end
Cargo.lock-do:
	cargo generate-lockfile

## vendor              » vendor the dependencies
vendor: vendor-start vendor-do vendor-end
vendor-do:
	cargo vendor

## manpages            » build the manpages
$(MANPAGES): manpages-start manpages-do manpages-end
manpages-do:
	cargo run --bin generate-manpages --features manpages

## setup-reclass       » clone Python reclass reference into references/reclass/
setup-reclass: setup-reclass-start setup-reclass-do setup-reclass-end
setup-reclass-do:
	@test -d references/reclass/.git || \
		git clone https://github.com/salt-formulas/reclass references/reclass

## osc-sync            » sync packaging files to OBS checkout
osc-sync: osc-sync-start osc-sync-do osc-sync-end
osc-sync-do:
	$(MAKE) -C packaging/obs osc-sync

## packaging           » build RPM packages (creates tarballs first)
packaging: packaging-start dist packaging-do packaging-end
packaging-do:
	$(MAKE) -C packaging/rpm

##
## CODE QUALITY
## ------------

## commit              » execute commit checks
commit: commit-start commit-do format test clippy check-manpages commit-end
commit-do:

## test-cov            » test coverage report
test-cov: test-cov-start test-cov-do test-cov-end
test-cov-do:
	cargo tarpaulin

## test-cov-html       » test coverage report (html)
test-cov-html: test-cov-html-start test-cov-html-do test-cov-html-end
test-cov-html-do:
	cargo tarpaulin --out html

## format              » format the rust sources
format: format-start format-do format-end
format-do:
	cargo fmt

## clippy              » run clippy
clippy: clippy-start clippy-do clippy-end
clippy-do:
	cargo clippy

## check-manpages      » check the manpages for uncommitted changes
check-manpages: check-manpages-start $(MANPAGES) check-manpages-do check-manpages-end
check-manpages-do:
	@git diff --exit-code man/ || (echo "Man pages are out of date. Run 'make manpages' and commit the results." && exit 1)

##
## CLEANUP
## -------

## clean               » remove build and profile artifacts
clean: clean-start mostlyclean clean-do clean-end
clean-do:
	cargo clean
	rm -f perf.data perf.data.old tarpaulin-report.html

## mostlyclean         » remove build artifacts
mostlyclean: mostlyclean-start mostlyclean-do mostlyclean-end
mostlyclean-do:
	cargo clean

## distclean           » remove everything not part of the release
distclean: distclean-start clean distclean-do distclean-end
distclean-do:
	rm -rf vendor/
	rm -f $(TARBALL) $(VENDOR_TARBALL)
	rm -f $(TARBALL_SHA256) $(VENDOR_SHA256)
	rm -f $(TARBALL_ASC) $(VENDOR_ASC)

## maintainer-clean    » remove everything that can be generated
maintainer-clean: maintainer-clean-start distclean maintainer-clean-do maintainer-clean-end
maintainer-clean-do:
	rm -f man/*.1 Cargo.lock

##
## START/DO/END ANNOUNCEMENTS
## --------------------------

%-start:
	$(info ****** making $*)

%-end:
	$(info ****** finished $*)

##
## MISCELLANEOUS
## -------------

## help                 » show this help message
help:
	@sed -n -e "s/^## \?\(.*\)/\1/p" "$(THIS_MAKEFILE)"
	@echo ""
	@echo "VARIABLE VALUE"
	@echo "--------------"
	@echo "PREFIX           = $(PREFIX)"
	@echo "DESTDIR          = $(DESTDIR)"
	@echo "BINDIR           = $(BINDIR)"
	@echo "MANDIR           = $(MANDIR)"
	@echo "INSTALL          = $(INSTALL)"
	@echo "INSTALL_PROGRAM  = $(INSTALL_PROGRAM)"
	@echo "INSTALL_DATA     = $(INSTALL_DATA)"
	@echo "VERSION          = $(VERSION)"
	@echo "NAME             = $(NAME)"
	@echo "GPG_KEY          = $(GPG_KEY)"
	@echo "GH_REPO          = $(GH_REPO)"
	@echo "GH_REMOTE        = $(GH_REMOTE)"

##
## VARIABLES
## ---------

## PREFIX               » installation prefix
## DESTDIR              » staging directory for packaging
## BINDIR               » binary install directory
## MANDIR               » man page directory
## INSTALL              » install command
## INSTALL_PROGRAM      » install command for executables
## INSTALL_DATA         » install command for data files
## VERSION              » package version (from spec file)
## NAME                 » package name (from spec file)
## GPG_KEY              » GPG key ID for signing releases
## GH_REPO              » GitHub repository (owner/repo format)
## GH_REMOTE            » git remote name for GitHub (default: github)
##
## VERBOSE				» print execute commands

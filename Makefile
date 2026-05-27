# SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
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
RPM_RELEASE    := $(shell sed -n 's/^Release:\s\+\([0-9]\+\).*/\1/p' packaging/rpm/ferroclass.spec)
SOURCE_TAG      := v$(VERSION)
RPM_TAG         := rpm/$(VERSION)-$(RPM_RELEASE)
RELEASE_BRANCH  := release/$(shell echo $(VERSION) | sed 's/\([0-9]\+\)\.\([0-9]\+\)\.[0-9]\+/\1.\2.X/')
TARBALL_DIR     := packaging/rpm
TARBALL         := $(TARBALL_DIR)/$(NAME)-$(VERSION).tar.gz
VENDOR_TARBALL  := $(TARBALL_DIR)/$(NAME)-$(VERSION)-vendor.tar.gz
TARBALL_SHA256   := $(TARBALL).sha256
VENDOR_SHA256   := $(VENDOR_TARBALL).sha256
TARBALL_ASC     := $(TARBALL).asc
VENDOR_ASC      := $(VENDOR_TARBALL).asc
GPG_KEY          ?= ferroclass@michael-jansen.biz
GH_REPO          ?= jansenm/ferroclass
GH_REMOTE        ?= github
MATURIN          ?= maturin

MANPAGES        := man/ferroclass-ansible.1 man/ferroclass-salt.1 man/ferroclass.1

TARGETS         := all all-do \
                   build build-do \
                   build-release build-release-do \
                   test test-do \
                   test-vendor test-vendor-do \
                   check check-do \
                   doc doc-do \
                   docclean docclean-do \
                   install install-do \
                   install-bin install-bin-do \
                   install-strip install-strip-do \
                   install-man install-man-do \
                   uninstall uninstall-do \
                   dist dist-do \
                   checksums checksums-do \
                   sign sign-do \
                   tag tag-do \
                   release-gh release-gh-do \
                   release release-do \
                   publish-crates publish-crates-do \
                   bump-version bump-version-do \
                   release-branch release-branch-do \
                   rpm-tag rpm-tag-do \
                   rpm-release rpm-release-do \
                   osc-sync osc-sync-do \
                   osc-add osc-add-do \
                   osc-commit osc-commit-do \
                   packaging packaging-do \
                   setup-reclass setup-reclass-do \
                   commit commit-do \
                   test-cov test-cov-do \
                   test-cov-html test-cov-html-do \
                   format format-do \
                   clippy clippy-do \
                   reuse reuse-do \
                   check-manpages check-manpages-do \
                   wheel wheel-do \
                   pip-install pip-install-do \
                   publish-pypi publish-pypi-do \
                   clean clean-do \
                   mostlyclean mostlyclean-do \
                   distclean distclean-do \
                   maintainer-clean maintainer-clean-do \
                   help help-do \
                   vendor vendor-do \
                   Cargo.lock Cargo.lock-do \
                   manpages-do \
                   $(MANPAGES) \
                   %-start %-end

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
build: build-start $(MANPAGES) build-do build-end
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

## test-vendor         » run the tests with vendored dependencies
test-vendor: test-vendor-start vendor test-vendor-do test-vendor-end
test-vendor-do:
	cargo build --frozen --config .cargo/config.vendor.toml
	cargo test --release --frozen --config .cargo/config.vendor.toml

## check               » run the tests (GNU standard alias)
check: check-start check-do test check-end
check-do:

## doc                 » generate API documentation
doc: doc-start doc-do doc-end
doc-do:
	cargo doc --no-deps

## dist                » create source and vendor tarballs from SOURCE_TAG
dist: dist-start vendor dist-do dist-end
dist-do:
	@git tag -l '$(SOURCE_TAG)' | grep -q '.' || (echo "ERROR: Source tag $(SOURCE_TAG) not found. Run 'make tag' first or set SOURCE_TAG." && exit 1)
	git -C . archive --format=tar.gz --prefix=$(NAME)-$(VERSION)/ $(SOURCE_TAG) > $(TARBALL)
	VENDOR_STAGING=$$(mktemp -d) && \
		mkdir -p "$$VENDOR_STAGING/.cargo" && \
		{ cat .cargo/config.toml; echo ""; cat .cargo/config.vendor.toml; } > "$$VENDOR_STAGING/.cargo/config.toml" && \
		tar czf $(VENDOR_TARBALL) -C "$$VENDOR_STAGING" .cargo/config.toml -C . vendor/ && \
		rm -rf "$$VENDOR_STAGING"
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

## tag                » create and push source tag for current version
tag: tag-start tag-do tag-end
tag-do:
	git tag -a $(SOURCE_TAG) -m "Release $(SOURCE_TAG)"
	git push $(GH_REMOTE) $(SOURCE_TAG)

## release-gh         » create GitHub Release with tarballs and checksums
release-gh: release-gh-start release-gh-do release-gh-end
release-gh-do:
	@CHANGELOG=$$(sed -n '/^## \[$(VERSION)\]/,/^## \[/{/^## \[/!p}' CHANGELOG.md); \
	gh release create $(SOURCE_TAG) \
		$(TARBALL) $(VENDOR_TARBALL) \
		$(TARBALL_ASC) $(VENDOR_ASC) \
		$(TARBALL_SHA256) $(VENDOR_SHA256) \
		--title "$(SOURCE_TAG)" \
		--notes "$$CHANGELOG"

## bump-version       » bump version in spec, Cargo.toml, and pyproject.toml (requires VERSION_NEW=x.y.z)
bump-version: bump-version-start bump-version-do bump-version-end
bump-version-do:
	@test -n "$(VERSION_NEW)" || (echo "Usage: make bump-version VERSION_NEW=x.y.z" && exit 1)
	sed -i 's/^Version:.*/Version:        $(VERSION_NEW)/' packaging/rpm/ferroclass.spec
	perl -i -pe 'BEGIN { $$n=0; } $$n++ < 1 && s/^version = ".*"/version = "$(VERSION_NEW)"/g' Cargo.toml
	sed -i 's/^version = ".*"/version = "$(VERSION_NEW)"/' pyproject.toml
	@echo "Version bumped to $(VERSION_NEW). Update CHANGELOG.md before releasing."

## publish-crates     » publish the crate to crates.io
publish-crates: publish-crates-start publish-crates-do publish-crates-end
publish-crates-do:
	cargo publish --registry crates-io

## release            » full source release: verify, package, tag, and publish
release: release-start commit tag dist checksums sign release-gh publish-crates publish-pypi release-end
release-do:

##
## RELEASE BRANCH WORKFLOW
## -----------------------
##
## Source releases (v0.11.0) and RPM packaging releases (rpm/0.11.0-6) are
## managed on separate branches to keep main clean for source code development.
##
## Workflow:
##   1. Develop on main, merge to release/X.Y.Z when ready for a source release
##   2. Run 'make release' on main to create the source tag (v0.11.0) and
##      publish to GitHub, crates.io, and PyPI
##   3. Run 'make release-branch' on main to create the release branch
##      (release/0.11.X) from the source tag
##   4. Switch to the release branch: git checkout release/0.11.X
##   5. Make packaging changes (spec, changes file) and commit
##   6. Run 'make rpm-release' to tag, sign, sync to OBS, and push
##   7. Repeat steps 5-6 for each RPM Release bump
##   8. When done, merge packaging changes back to main and delete the branch
##
## The source tarball for RPM builds is always archived from the source tag,
## not HEAD. This ensures every RPM package is traceable to an exact source
## commit, even when the release branch has packaging-only changes on top.

## release-branch     » create release/X.Y.X branch from SOURCE_TAG and push it
release-branch: release-branch-start release-branch-do release-branch-end
release-branch-do:
	@git tag -l '$(SOURCE_TAG)' | grep -q '.' || (echo "ERROR: Source tag $(SOURCE_TAG) not found. Run 'make tag' first." && exit 1)
	git branch $(RELEASE_BRANCH) $(SOURCE_TAG) 2>/dev/null || echo "Branch $(RELEASE_BRANCH) already exists"
	git push $(GH_REMOTE) $(RELEASE_BRANCH)
	@echo "Created and pushed release branch $(RELEASE_BRANCH) from $(SOURCE_TAG)"
	@echo "Switch to it with: git checkout $(RELEASE_BRANCH)"

## rpm-tag            » create and push rpm/VERSION-RELEASE tag on current branch
rpm-tag: rpm-tag-start rpm-tag-do rpm-tag-end
rpm-tag-do:
	git tag -a $(RPM_TAG) -m "RPM release $(RPM_TAG)"
	git push $(GH_REMOTE) $(RPM_TAG)

## rpm-release        » full RPM release: verify, package, tag, sign, and push to OBS
rpm-release: rpm-release-start rpm-tag dist checksums sign osc-sync osc-add osc-commit rpm-release-end
rpm-release-do:

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
	cargo vendor > .cargo/config.vendor.toml
	@echo "Vendored dependencies saved. Use 'make test-vendor' to build with vendored sources."

## manpages            » build the manpages
$(MANPAGES): manpages-start manpages-do manpages-end
manpages-do:
	cargo run --bin generate-manpages --features manpages

## wheel               » build Python wheel with maturin
wheel: wheel-start wheel-do wheel-end
wheel-do:
	$(MATURIN) build --release --features python
	@ls -alF target/wheels/ferroclass-*-cp*-linux_*.whl 2>/dev/null || \
		ls -alF target/wheels/ferroclass-*.whl 2>/dev/null || \
		echo "Wheel built. Find it in target/wheels/"

## pip-install          » install ferroclass into the current Python environment
pip-install: pip-install-start pip-install-do pip-install-end
pip-install-do:
	$(MATURIN) develop --features python

## publish-pypi        » publish the wheel to PyPI
publish-pypi: publish-pypi-start publish-pypi-do publish-pypi-end
publish-pypi-do:
	$(MATURIN) publish --no-verify

## setup-reclass       » clone Python reclass reference into references/reclass/
setup-reclass: setup-reclass-start setup-reclass-do setup-reclass-end
setup-reclass-do:
	@test -d references/reclass/.git || \
		git clone https://github.com/salt-formulas/reclass references/reclass

## osc-sync            » sync packaging files to OBS checkout
osc-sync: osc-sync-start osc-sync-do osc-sync-end
osc-sync-do:
	$(MAKE) -C packaging/obs osc-sync

## osc-add             » add/remove files in OBS checkout
osc-add: osc-add-start osc-add-do osc-add-end
osc-add-do:
	$(MAKE) -C packaging/obs osc-add

## osc-commit          » commit changes to OBS
osc-commit: osc-commit-start osc-commit-do osc-commit-end
osc-commit-do:
	$(MAKE) -C packaging/obs osc-commit

## packaging           » build RPM packages (creates tarballs first)
packaging: packaging-start dist packaging-do packaging-end
packaging-do:
	$(MAKE) -C packaging/rpm

##
## CODE QUALITY
## ------------

## commit              » execute commit checks
commit: commit-start commit-do test clippy format reuse check-manpages commit-end
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

## reuse              » run reuse
reuse: reuse-start reuse-do reuse-end
reuse-do:
	reuse lint

## check-manpages      » check the manpages for uncommitted changes
check-manpages: check-manpages-start $(MANPAGES) check-manpages-do check-manpages-end
check-manpages-do:
	@git diff --exit-code man/ || (echo "Man pages are out of date. Run 'make manpages' and commit the results." && exit 1)

##
## CLEANUP
## -------

## clean               » remove build and profile artifacts
clean: clean-start mostlyclean packaging-clean clean-do clean-end
clean-do:
	rm -f perf.data perf.data.old tarpaulin-report.html
	rm -rf target/wheels/

## mostlyclean         » remove build artifacts
mostlyclean: mostlyclean-start packaging-mostlyclean mostlyclean-do mostlyclean-end
mostlyclean-do:
	cargo clean

## distclean           » remove everything not part of the release
distclean: distclean-start clean packaging-distclean distclean-do distclean-end
distclean-do:

## maintainer-clean    » remove everything that can be generated
maintainer-clean: maintainer-clean-start distclean packaging-maintainer-clean maintainer-clean-do maintainer-clean-end
maintainer-clean-do:
	rm -rf vendor/
	rm -f .cargo/config.vendor.toml
	rm -f man/*.1 Cargo.lock
	rm -rf target/wheels/

## docclean            » remove generated API documentation
docclean: docclean-start docclean-do docclean-end
docclean-do:
	rm -rf target/doc

.PHONY: packaging-%
packaging-%:
	make -C packaging/rpm $*

##
## START/DO/END ANNOUNCEMENTS
## --------------------------

%-start:
	$(info >>>>>> making $*)

%-end:
	$(info <<<<<< finished $*)

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
	@echo "RPM_RELEASE      = $(RPM_RELEASE)"
	@echo "SOURCE_TAG       = $(SOURCE_TAG)"
	@echo "RPM_TAG          = $(RPM_TAG)"
	@echo "RELEASE_BRANCH   = $(RELEASE_BRANCH)"
	@echo "GPG_KEY          = $(GPG_KEY)"
	@echo "GH_REPO          = $(GH_REPO)"
	@echo "GH_REMOTE        = $(GH_REMOTE)"
	@echo "MATURIN          = $(MATURIN)"

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
## RPM_RELEASE          » RPM release number (from spec file)
## SOURCE_TAG           » git source tag (v.VERSION)
## RPM_TAG              » git RPM release tag (rpm/VERSION-RELEASE)
## RELEASE_BRANCH       » release branch name (release/MAJOR.MINOR.X)
## GPG_KEY              » GPG key ID for signing releases
## GH_REPO              » GitHub repository (owner/repo format)
## GH_REMOTE            » git remote name for GitHub (default: github)
## MATURIN              » maturin binary (default: maturin)
##
## VERBOSE				» print execute commands
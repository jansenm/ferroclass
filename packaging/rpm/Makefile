# SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
# SPDX-License-Identifier: MPL-2.0

THIS_MAKEFILE := $(lastword $(MAKEFILE_LIST))

VERSION := $(shell sed -n 's/^Version:\s*//p' ferroclass.spec)
NAME := $(shell sed -n 's/^Name:\s*//p' ferroclass.spec)
TARBALL := $(NAME)-$(VERSION).tar.gz
VENDOR_TARBALL := $(NAME)-$(VERSION)-vendor.tar.gz
BUILD_ROOT := /var/tmp/build-root

.PHONY: rpm rpm-local rpm-obs clean help

##
## RPM BUILD
## --------

# Build in a chroot using obs-build (preferred for CI/OBS).
# Validates BuildRequires in an isolated environment.
# NOTE: needs preference hints for libasan8/libtsan2 resolution on
# current Tumbleweed. Use rpm-local until that's fixed.
## rpm-obs             » build RPM in chroot using obs-build
rpm-obs: $(TARBALL) $(VENDOR_TARBALL)
	build --clean --stage=bb \
		--dist tumbleweed \
		--repo zypp:// \
		--root $(BUILD_ROOT) \
		$(NAME).spec

# Build using local rpmbuild without dependency validation.
# Requires cargo (and BuildRequires) already installed on the host.
## rpm-local           » build RPM locally using rpmbuild
rpm-local: $(TARBALL) $(VENDOR_TARBALL)
	rpmbuild -bb --nodeps $(NAME).spec \
		--define "_sourcedir $(CURDIR)" \
		--define "_specdir $(CURDIR)" \
		--define "_builddir $(CURDIR)/BUILD" \
		--define "_rpmdir $(CURDIR)/RPMS" \
		--define "_buildrootdir $(CURDIR)/BUILDROOT"

## rpm                 » build RPM locally (alias for rpm-local)
rpm: rpm-local

##
## CLEANUP
## -------

## clean               » remove RPM build artifacts
clean:
	rm -rf BUILD BUILDROOT RPMS $(BUILD_ROOT)

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
	@echo "  VENDOR_TARBALL   = $(VENDOR_TARBALL)"
	@echo "  BUILD_ROOT       = $(BUILD_ROOT)"
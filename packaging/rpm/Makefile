# SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
# SPDX-License-Identifier: MPL-2.0

THIS_MAKEFILE := $(lastword $(MAKEFILE_LIST))

VERSION := $(shell sed -n 's/^Version:\s*//p' ferroclass.spec)
NAME := $(shell sed -n 's/^Name:\s*//p' ferroclass.spec)
TARBALL := $(NAME)-$(VERSION).tar.gz
VENDOR_TARBALL := $(NAME)-$(VERSION)-vendor.tar.gz
BUILD_ROOT := /var/tmp/build-root

.PHONY: rpm rpm-local rpm-local-do rpm-obs rpm-obs-do rpm-rocky9 rpm-rocky9-do clean clean-do \
		.distclean distclean-do mostlyclean mostlyclean-do maintainer-clean maintainer-clean-do %-start \
		.%-end help

##
## RPM BUILD
## --------

# Build in a chroot using obs-build (preferred for CI/OBS).
# Validates BuildRequires in an isolated environment.
# NOTE: needs preference hints for libasan8/libtsan2 resolution on
# current Tumbleweed. Use rpm-local until that's fixed.
## rpm-obs             » build RPM in chroot using obs-build
rpm-obs: rpm-ob-start $(TARBALL) $(VENDOR_TARBALL) rpm-obs-do rpm-obs-end
rpm-obs-do:
	build --clean --stage=bb \
		--dist tumbleweed \
		--repo zypp:// \
		--root $(BUILD_ROOT) \
		$(NAME).spec

# Build using local rpmbuild without dependency validation.
# Requires cargo (and BuildRequires) already installed on the host.
## rpm-local           » build RPM locally using rpmbuild
rpm-local: rpm-local-start $(TARBALL) $(VENDOR_TARBALL) rpm-local-do rpm-local-end
rpm-local-do:
	rpmbuild -bb --nodeps $(NAME).spec \
		--define "_sourcedir $(CURDIR)" \
		--define "_specdir $(CURDIR)" \
		--define "_builddir $(CURDIR)/BUILD" \
		--define "_rpmdir $(CURDIR)/RPMS" \
		--define "_buildrootdir $(CURDIR)/BUILDROOT"

# Build in a chroot using obs-build targeting Rocky Linux 9.
# Requires obs-build with rockylinux-9 dist config installed.
## rpm-rocky9          » build RPM in chroot targeting Rocky Linux 9
rpm-rocky9: rpm-rocky9-start $(TARBALL) $(VENDOR_TARBALL) rpm-rocky9-do rpm-rocky9-end
rpm-rocky9-do:
	build --clean --stage=bb \
		--dist rockylinux-9 \
		--root $(BUILD_ROOT) \
		$(NAME).spec

## rpm                 » build RPM locally (alias for rpm-local)
rpm: rpm-local

##
## CLEANUP
## -------

## clean               » remove RPM build artifacts
clean: clean-start mostlyclean clean-do clean-end
clean-do:
	rm -rf BUILD BUILDROOT RPMS

## mostlyclean         » remove build artifacts
mostlyclean: mostlyclean-start mostlyclean-do mostlyclean-end
mostlyclean-do:
	rm -rf "$(BUILD_ROOT)"

## distclean           » remove everything not part of the release
distclean: distclean-start clean distclean-do distclean-end
distclean-do:
	rm -f *.tar.gz *.tar.gz.asc *.tar.gz.sha256

## maintainer-clean    » remove everything that can be generated
maintainer-clean: maintainer-clean-start distclean maintainer-clean-end

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

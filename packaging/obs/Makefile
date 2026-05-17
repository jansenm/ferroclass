# SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
# SPDX-License-Identifier: MPL-2.0

THIS_MAKEFILE := $(lastword $(MAKEFILE_LIST))

VERSION      := $(shell sed -n 's/^Version:\s*//p' ../rpm/ferroclass.spec)
NAME         := $(shell sed -n 's/^Name:\s*//p' ../rpm/ferroclass.spec)
# Override OBS_PROJECT to match your OBS project name.
# Example: make osc-build OBS_PROJECT=home:mjansen1972:ferroclass
OBS_PROJECT  ?= home:$(USER):$(NAME)
OBS_PACKAGE  ?= $(NAME)
OBS_DIR      ?= $(HOME)/obs/$(OBS_PROJECT)/$(OBS_PACKAGE)

# Build target configuration. Override these to build for a different
# repository or architecture. The repository name must match an <repository>
# entry in your OBS project meta. Common values:
#   openSUSE_Tumbleweed  RockyLinux_9  RockyLinux_10
OSC_REPO     ?= openSUSE_Tumbleweed
OSC_ARCH     ?= x86_64

RPM_DIR      := ../rpm

TARGETS      := osc-checkout osc-sync osc-add osc-commit osc-build osc-build-rocky9 osc-build-rocky10 osc-service-run help

ifndef DEBUG
.SILENT:
endif

.PHONY: $(TARGETS)

##
## OBS OPERATIONS
## -------------

## osc-checkout         » check out OBS package directory
osc-checkout: osc-checkout-start osc-checkout-do osc-checkout-end
osc-checkout-do:
	osc checkout $(OBS_PROJECT) $(OBS_PACKAGE)

## osc-sync             » sync spec, changes, and _service to OBS checkout
osc-sync: osc-sync-start osc-sync-do osc-sync-end
osc-sync-do:
	@test -d $(OBS_DIR)/.osc || (echo "ERROR: $(OBS_DIR) is not an OBS checkout. Run 'make osc-checkout' first or set OBS_PROJECT correctly." && exit 1)
	cp $(RPM_DIR)/ferroclass.spec $(OBS_DIR)/ferroclass.spec
	cp $(RPM_DIR)/ferroclass.changes $(OBS_DIR)/ferroclass.changes
	sed -e 's/@VERSION@/$(VERSION)/g' _service.in > $(OBS_DIR)/_service

## osc-add              » add/remove files in OBS checkout
osc-add: osc-add-start osc-add-do osc-add-end
osc-add-do:
	cd $(OBS_DIR) && osc addremove

## osc-commit           » commit changes to OBS
osc-commit: osc-commit-start osc-commit-do osc-commit-end
osc-commit-do:
	cd $(OBS_DIR) && osc commit

## osc-build            » build package using OBS (local chroot, default: openSUSE_Tumbleweed x86_64)
osc-build: osc-build-start osc-build-do osc-build-end
osc-build-do:
	cd $(OBS_DIR) && osc build $(OSC_REPO) $(OSC_ARCH)

## osc-build-rocky9     » build for Rocky Linux 9 x86_64
osc-build-rocky9: osc-build-rocky9-start osc-build-rocky9-do osc-build-rocky9-end
osc-build-rocky9-do:
	cd $(OBS_DIR) && osc build RockyLinux_9 x86_64

## osc-build-rocky10    » build for Rocky Linux 10 x86_64
osc-build-rocky10: osc-build-rocky10-start osc-build-rocky10-do osc-build-rocky10-end
osc-build-rocky10-do:
	cd $(OBS_DIR) && osc build RockyLinux_10 x86_64

## osc-service-run      » run OBS source services locally (needs obs-service-cargo)
osc-service-run: osc-service-run-start osc-service-run-do osc-service-run-end
osc-service-run-do:
	cd $(OBS_DIR) && osc service manualrun

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

## help                » show this help message
help: help-start help-do help-end
help-do:
	@sed -n -e "s/^## \?\(.*\)/\1/p" "$(THIS_MAKEFILE)"
	@echo ""
	@echo "VARIABLE VALUE"
	@echo "--------------"
	@echo "OBS_PROJECT  = $(OBS_PROJECT)"
	@echo "OBS_PACKAGE   = $(OBS_PACKAGE)"
	@echo "OBS_DIR       = $(OBS_DIR)"
	@echo "OSC_REPO      = $(OSC_REPO)"
	@echo "OSC_ARCH      = $(OSC_ARCH)"
	@echo "VERSION       = $(VERSION)"
	@echo "NAME          = $(NAME)"

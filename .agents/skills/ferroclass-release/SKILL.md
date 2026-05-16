<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

---
name: ferroclass-release
description: Release process for ferroclass. Use when preparing a new version of the ferroclass crate, building RPM packages, or creating source/vendored tarballs. Covers version bump consistency (Cargo.toml, spec, changelog), tarball creation, tag naming, and publication automation.
---

# Ferroclass Release Process

## Overview

Releasing ferroclass means bumping the version in all locations, verifying consistency,
building a release tarball, and optionally tagging. This release skill is used by the
`build-packaging` agent when the user says things like "cut a release" or "bump to 0.9.0".

The version is defined in **two** places that must stay in sync:

| File                           | What to Change                        |
|--------------------------------|---------------------------------------|
| `Cargo.toml`                   | `version = "X.Y.Z"`                   |
| `packaging/rpm/ferroclass.spec`| `Version: X.Y.Z`                      |

**Release**: `packaging/rpm/ferroclass.spec`
| `Version: X.Y.Z`                      |

**Note**: `Makefile` does NOT hardcode the version — it reads it from the spec file
via `$(shell sed -n 's/^Version:\s*//p' packaging/rpm/ferroclass.spec)`. Keep the
spec file as the **single source of truth** for version numbers.

---

## Checklist

Use this checklist for every release, in order:

- [ ] 1. **Set version in all release files**
  - `Cargo.toml`: `version = "X.Y.Z"`
  - `packaging/rpm/ferroclass.spec`: `Version: X.Y.Z`
  - Verify both match: `grep '^version' Cargo.toml` and `grep '^Version' packaging/rpm/ferroclass.spec`
  - Note: `packaging/obs/_service.in` uses `@VERSION@` which is auto-substituted from the spec file during `make osc-sync`

- [ ] 2. **Update `CHANGELOG.md`**
  - Add a new section with `[X.Y.Z] - YYYY-MM-DD` under `## [Unreleased]`
  - Follow [Keep a Changelog](https://keepachangelog.com) format
  - Include: Added / Changed / Deprecated / Removed / Fixed / Security

- [ ] 3. **Add SUSE changelog entry**
  - `packaging/rpm/ferroclass.changes`: add dated entry with bullet list
  - Format: `DATE Author <email> - X.Y.Z-N` (N = first release, usually 1)

- [ ] 4. **Bump spec Release** (if version is not changing, e.g. hotfix)
  - `Release: N%{?dist}` → `Release: N+1%{?dist}`
  - Add changelog entry in the spec `%changelog` section

- [ ] 5. **Regenerate man pages**
  - `make manpages`
  - This updates version embedded in `man/*.1`

- [ ] 6. **Run quality gates**
  - `make commit` — runs format, test, clippy, check-manpages, all must pass
  - `cargo build --release --locked` — verify release build succeeds
  - `reuse lint` — verify REUSE compliance

- [ ] 7. **Verify version consistency**
  - `grep '^version' Cargo.toml`
  - `grep '^Version' packaging/rpm/ferroclass.spec`
  - `sed -n 's/TH.*\([0-9]\+\.[0-9]\+\.[0-9]\+\).*/\1/p' man/reclass.1 | head -1`
  - All three must show the same version
  - Optionally verify: `make -C packaging/obs osc-sync V=1` shows correct version substitution

- [ ] 8. **Build release tarball**
  - `make dist` — creates `packaging/rpm/ferroclass-X.Y.Z.tar.gz` + vendor tarball
  - Verify tarball contents: `tar tzf packaging/rpm/ferroclass-X.Y.Z.tar.gz | head`
  - Verify vendor tarball: `tar tzf packaging/rpm/ferroclass-X.Y.Z-vendor.tar.gz | head`

- [ ] 9. **Test RPM build** (optional, SUSE/RHEL)
  - `make packaging` — builds RPM from tarballs
  - Verify RPMs exist in `packaging/rpm/RPMS/`

- [ ] 10. **Sync to OBS** (optional, for submitting to Open Build Service)
  - `make osc-sync` — copies spec, changes, and generates `_service` to OBS checkout
  - `cd $(OBS_DIR) && osc addremove && osc commit` — submit to OBS
  - `osc service manualrun` — trigger `cargo_vendor` to generate vendor tarball
  - Verify that OBS builds succeed on [build.opensuse.org](https://build.opensuse.org)

- [ ] 11. **Commit everything**
  - `git add -A`
  - `git commit -m "Release X.Y.Z"`

- [ ] 12. **Tag the release**
  - `git tag -a "vX.Y.Z" -m "Release X.Y.Z"`
  - Push tag: `git push && git push --tags` (or `git push origin vX.Y.Z`)

- [ ] 13. **Verify tarball integrity**
  - `make clean` — remove build artifacts
  - Extract tarball to a temporary directory
  - Run `cargo test` inside extracted directory — must pass
  - Run `cargo build --release --locked` — must succeed

---

## Version Number Rules

- **Semantic versioning**: `X.Y.Z` where:
  - `X` = major (breaking ABI changes, incompatible config formats)
  - `Y` = minor (new features, new adapters, new CLI flags)
  - `Z` = patch (bug fixes, doc fixes, dependency updates)
- **Pre-release tags**: `1.0.0-alpha.1`, `1.0.0-beta.2` — append to Cargo.toml version
  but NOT to the spec `Version:` (RPM spec does not support pre-release strings)
- **First release of a new version** always gets `Release: 1%{?dist}`
- **Hotfix/hot patch** bumps `Release` without changing `Version`

---

## Makefile Automation

The `Makefile` has four targets that are relevant to releases:

| Target          | What it does                                                         |
|-----------------|----------------------------------------------------------------------|
| `build-release` | `cargo build --release --locked` — creates release binaries          |
| `dist`          | Creates source tarball + vendor tarball in `packaging/rpm/`            |
| `packaging`     | Runs `dist` then builds RPM in packaging/rpm/                        |
| `osc-sync`      | Syncs spec, changes, and generates `_service` to OBS checkout        |

### Creating a New Release with Make

```shell
# 1. Update versions in Cargo.toml and packaging/rpm/ferroclass.spec
# 2. Update CHANGELOG.md and packaging/rpm/ferroclass.changes
# 3. Run quality gates
make commit           # format, test, clippy, check-manpages

# 4. Build release binaries + vendor + tarball
make dist             # creates tarballs in packaging/rpm/

# 5. Optionally test RPM build
make packaging        # builds RPM from tarball

# 6. Verify tarball integrity
tar tzf packaging/rpm/ferroclass-X.Y.Z.tar.gz | grep Makefile

# 7. Commit and tag
git add -A
git commit -m "Release X.Y.Z"
git tag -a vX.Y.Z -m "Release X.Y.Z"
git push && git push --tags
```

---

## Cargo.toml Release Fields

The `[package]` section already contains all required metadata. Nothing changes here
for a normal release, but review these fields if the release changes project metadata:

```toml
[package]
name = "ferroclass"
version = "X.Y.Z"          # CHANGE THIS
edition = "2024"
license = "MPL-2.0"
description = "Hierarchical inventory management tool (reclass compatible)"
authors = ["Michael Jansen <mike@michael-jansen.biz>"]
repository = "https://github.com/jansenm/ferroclass"
homepage = "https://github.com/jansenm/ferroclass"
readme = "README.md"
keywords = ["reclass", "inventory", "ansible", "salt", "configuration"]
categories = ["command-line-utilities", "config"]
```

---

## RPM Spec Version/Release Flow

The spec file is the **single source of truth** for version in Makefiles.

```spec
Name:           ferroclass
Version:        X.Y.Z
Release:        N%{?dist}
```

- **New feature release**: bump `Version`, set `Release: 1`
- **Rebuild for same version**: keep `Version`, bump `Release`
- **RPM changelog** (`%changelog` section): add entry for every Release bump

---

## Pre-Release Verification Script

Run this to catch version inconsistencies before committing:

```shell
#!/bin/bash
# Run from project root

set -e

echo "=== Version Check ==="
TOML_V=$(grep '^version' Cargo.toml | sed 's/.*"//;s/".*//')
SPEC_V=$(grep '^Version' packaging/rpm/ferroclass.spec | awk '{print $2}')

echo "Cargo.toml:   $TOML_V"
echo "spec file:    $SPEC_V"

if [ "$TOML_V" != "$SPEC_V" ]; then
    echo "ERROR: Versions do not match!"
    exit 1
fi

echo "✓ Versions match"

echo ""
echo "=== Running quality gates ==="
make commit

echo ""
echo "=== Building release tarball ==="
make dist

echo ""
echo "=== All checks passed. Ready to commit and tag. ==="
```

---

## What to NOT Do

1. **Do NOT bump version in only one file.** If Cargo.toml and the spec differ, `make dist`
   creates a tarball with the spec version but Cargo builds a binary with a different version.
2. **Do NOT skip `make commit` before tagging.** The tag represents the exact commit that
   passed all quality gates.
3. **Do NOT commit uncommitted Cargo.lock changes** unless `cargo.lock` changes are part
   of the release (dependency updates).
4. **Do NOT forget to regenerate man pages.** The `man/*.1` files embed the version.
5. **Do NOT publish a tag without pushing it.** `git push --tags` must succeed before
   the release is considered public.
6. **Do NOT ignore spec `%changelog`.** Even for internal releases, changelog entries are
   required for RPM builds and audit trails.

---

## Post-Release Checklist

After the tag is pushed:

- [ ] GitHub releases page updated with changelog
- [ ] OBS source service triggered if using `obs-service-cargo`
- [ ] crates.io published (if publishing to Rust registry)
  - `cargo publish --locked --dry-run` first
  - `cargo publish --locked` after
- [ ] Binary artifacts uploaded to release page (optional)
  - `cargo build --release` and upload `target/release/reclass{,-ansible,-salt}`

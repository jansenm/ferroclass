<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

---
name: ferroclass-release
description: Release process for ferroclass. Use when preparing a new version, building release artifacts, creating GitHub Releases, or syncing to OBS. Covers version bumping, tarball creation, checksums, GPG signing, tagging, and publication.
---

# Ferroclass Release Process

## Overview

Ferroclass uses a **hybrid release strategy**:

- **GitHub Releases** host source tarballs, vendor tarballs, SHA256 checksums,
  and GPG signatures.
- **Open Build Service (OBS)** builds and distributes binary RPM packages for
  openSUSE Tumbleweed, Rocky Linux 9, and Rocky Linux 10.

No GitHub Actions CI/CD is used. The entire release process is manual via
Makefile targets, giving full control over what gets published.

The version is defined in **two** places that must stay in sync:

| File                            | What to Change    |
|---------------------------------|-------------------|
| `Cargo.toml`                    | `version = "X.Y.Z"` |
| `packaging/rpm/ferroclass.spec` | `Version: X.Y.Z`    |

The Makefile reads the version from the spec file via
`$(shell sed -n 's/^Version:\s*//p' packaging/rpm/ferroclass.spec)`.
The spec file is the **single source of truth** for version numbers.

---

## Make Targets for Releases

| Target           | Purpose                                                    |
|------------------|------------------------------------------------------------|
| `bump-version`   | Update version in spec file and Cargo.toml (VERSION_NEW=) |
| `dist`           | Create source and vendor tarballs in `packaging/rpm/`        |
| `checksums`      | Generate `.sha256` checksums for tarballs (depends on `dist`) |
| `sign`           | Sign tarballs with GPG (requires `GPG_KEY`, depends on `dist`) |
| `tag`            | Create and push git tag `vX.Y.Z`                            |
| `release-gh`     | Create GitHub Release with artifacts and changelog           |
| `release`        | Full pipeline: commit → dist → checksums → tag → release-gh → osc-sync |
| `osc-sync`       | Sync spec/changes/_service to OBS checkout                   |

### Key Variables

| Variable     | Default                         | Purpose                                      |
|-------------|---------------------------------|----------------------------------------------|
| `VERSION`   | Read from spec file             | Current package version                       |
| `GPG_KEY`   | `mike@michael-jansen.biz`       | GPG key ID for signing release tarballs       |
| `GH_REPO`   | `jansenm/ferroclass`            | GitHub repository for `gh release create`     |
| `OBS_USER`   | Auto-detected from `~/.config/osc/oscrc` | OBS username                      |
| `OBS_PROJECT`| `home:$(OBS_USER):ferroclass`  | OBS project name                              |
| `OSC_REPO`  | `openSUSE_Tumbleweed`           | OBS build repository                          |
| `OSC_ARCH`  | `x86_64`                        | OBS build architecture                         |

Override any variable on the command line:
`make release GPG_KEY=0x12345678 GH_REPO=other/repo`

---

## Release Checklist

### 1. Bump version

```shell
make bump-version VERSION_NEW=X.Y.Z
```

This updates `packaging/rpm/ferroclass.spec` and `Cargo.toml`.

### 2. Update CHANGELOG.md

Add a new section `[X.Y.Z] - YYYY-MM-DD` following the Keep a Changelog format.
This is manual — the changelog must be written by a human.

### 3. Update the SUSE changelog

Add a new entry to `packaging/rpm/ferroclass.changes`:

```
-------------------------------------------------------------------
Day Mon DD YYYY Author <email> - X.Y.Z-1

- Summary of changes
```

### 4. Regenerate man pages

```shell
make manpages
```

### 5. Run quality gates

```shell
make commit
```

This runs `cargo fmt`, `cargo test`, `cargo clippy`, and `check-manpages`.

### 6. Create the release

```shell
make release
```

This runs the full pipeline:
1. `commit` — quality gates
2. `dist` — create source and vendor tarballs
3. `checksums` — generate SHA256 checksums
4. `tag` — create and push git tag `vX.Y.Z`
5. `release-gh` — create GitHub Release with artifacts and changelog from CHANGELOG.md
6. `osc-sync` — sync packaging files to OBS checkout

### 7. Sync to OBS and build

```shell
cd ~/obs/home:mjansen1972:ferroclass/ferroclass
osc addremove
osc commit
make -C packaging/obs osc-build OBS_PROJECT=home:mjansen1972:ferroclass
make -C packaging/obs osc-build-rocky9 OBS_PROJECT=home:mjansen1972:ferroclass
make -C packaging/obs osc-build-rocky10 OBS_PROJECT=home:mjansen1972:ferroclass
```

### 8. GPG sign (when key is available)

```shell
make sign GPG_KEY=<key-id>
gh release upload vX.Y.Z \
    packaging/rpm/ferroclass-X.Y.Z.tar.gz.asc \
    packaging/rpm/ferroclass-X.Y.Z-vendor.tar.gz.asc
```

---

## Release Artifacts

| Artifact                                          | Location         | Purpose                      |
|---------------------------------------------------|------------------|------------------------------|
| `ferroclass-X.Y.Z.tar.gz`                        | GitHub Releases  | Source tarball                |
| `ferroclass-X.Y.Z-vendor.tar.gz`                 | GitHub Releases  | Vendored Rust dependencies    |
| `ferroclass-X.Y.Z.tar.gz.sha256`                  | GitHub Releases  | SHA256 checksum               |
| `ferroclass-X.Y.Z-vendor.tar.gz.sha256`           | GitHub Releases  | SHA256 checksum               |
| `ferroclass-X.Y.Z.tar.gz.asc`                     | GitHub Releases  | GPG signature                 |
| `ferroclass-X.Y.Z-vendor.tar.gz.asc`              | GitHub Releases  | GPG signature                 |
| Binary RPMs for openSUSE, Rocky 9/10               | OBS repositories | Distro package installation   |

---

## Supply Chain Security

The source tarball is the single source of truth. Both GitHub Releases and
OBS build from the same signed, checksummed tarball:

```
You (local machine)
  └── create tarball (make dist)
  └── sign with GPG (make sign)
  └── upload to GitHub Releases
  └── upload to OBS
OBS
  └── builds from tarball → produces signed RPMs
GitHub
  └── stores tarball + SHA256 + GPG signature

Users verify:
  - GitHub: "This tarball was signed by Michael Jansen"
  - OBS: "These RPMs were built from that tarball (same SHA256)"
```

No CI/CD pipeline has write access to releases. The `release` target is run
manually on the maintainer's machine, and the `gh` CLI creates the GitHub
Release. This minimizes the supply chain attack surface.

---

## Version Number Rules

- **Semantic versioning**: `X.Y.Z` where:
  - `X` = major (breaking ABI changes, incompatible config formats)
  - `Y` = minor (new features, new adapters, new CLI flags)
  - `Z` = patch (bug fixes, doc fixes, dependency updates)
- **Pre-release tags**: `1.0.0-alpha.1`, `1.0.0-beta.2` — append to Cargo.toml
  version but NOT to the spec `Version:` (RPM spec does not support pre-release
  strings)
- **First release of a new version** always gets `Release: 1%{?dist}`
- **Hotfix/hot patch** bumps `Release` without changing `Version`

---

## What to NOT Do

1. **Do NOT bump version in only one file.** If Cargo.toml and the spec differ,
   `make dist` creates a tarball with the spec version but Cargo builds a binary
   with a different version.
2. **Do NOT skip `make commit` before tagging.** The tag represents the exact
   commit that passed all quality gates.
3. **Do NOT forget to regenerate man pages.** The `man/*.1` files embed the
   version.
4. **Do NOT publish a tag without pushing it.** `git push origin vX.Y.Z` must
   succeed before the release is considered public.
5. **Do NOT ignore spec `%changelog`.** Even for internal releases, changelog
   entries are required for RPM builds and audit trails.
6. **Do NOT use GitHub Actions to build release artifacts.** The release process
   is manual by design to minimize supply chain attack surface.

---

## OBS Project Configuration

The OBS project `home:mjansen1972:ferroclass` is configured with these
repositories:

| Repository            | Path                     | Architectures  |
|-----------------------|--------------------------|----------------|
| openSUSE_Tumbleweed  | openSUSE:Factory/standard | x86_64, aarch64 |
| RockyLinux_9          | RockyLinux:9/standard     | x86_64, aarch64 |
| RockyLinux_10         | RockyLinux:10/standard    | x86_64, aarch64 |

The OBS Makefile auto-detects your OBS username from `~/.config/osc/oscrc`.

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

## Pre-Release Verification

Run this to catch version inconsistencies before committing:

```shell
# Version consistency check
TOML_V=$(grep '^version' Cargo.toml | sed 's/.*"//;s/".*//')
SPEC_V=$(grep '^Version' packaging/rpm/ferroclass.spec | awk '{print $2}')
[ "$TOML_V" = "$SPEC_V" ] && echo "OK: v$TOML_V" || echo "MISMATCH: Cargo=$TOML_V Spec=$SPEC_V"

# Quality gates
make commit

# Build release tarball
make dist

# Verify tarball contents
tar tzf packaging/rpm/ferroclass-$SPEC_V.tar.gz | head
```
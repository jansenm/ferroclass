<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

---
name: ferroclass-release
description: Release process for ferroclass. Use when preparing a new version, building release artifacts, creating GitHub Releases, or syncing to OBS. Covers version bumping, tarball creation, checksums, GPG signing, tagging, publication, and RPM release branch workflow.
---

# Ferroclass Release Process

## Overview

Ferroclass uses a **hybrid release strategy** with **separate branches** for source
and RPM packaging:

- **`main` branch** — Source code development. Source releases (`vX.Y.Z` tags)
  are created here.
- **`release/X.Y.X` branches** — RPM packaging. Created from the source tag;
  contain spec/changes tweaks. RPM tags (`rpm/X.Y.Z-N`) are created here.

**GitHub Releases** host source tarballs, vendor tarballs, SHA256 checksums,
and GPG signatures. **Open Build Service (OBS)** builds and distributes binary
RPM packages for openSUSE Tumbleweed and Rocky Linux 9.

No GitHub Actions CI/CD is used. The entire release process is manual via
Makefile targets, giving full control over what gets published.

The version is defined in **three** places that must stay in sync:

| File                            | What to Change    |
|---------------------------------|-------------------|
| `Cargo.toml`                    | `version = "X.Y.Z"` |
| `packaging/rpm/ferroclass.spec` | `Version: X.Y.Z`    |
| `pyproject.toml`                | `version = "X.Y.Z"` |

The Makefile reads the version from the spec file via
`$(shell sed -n 's/^Version:\s*//p' packaging/rpm/ferroclass.spec)`.
The spec file is the **single source of truth** for version numbers.

---

## Make Targets for Releases

### Source Release (on `main` branch)

| Target           | Purpose                                                    |
|------------------|------------------------------------------------------------|
| `bump-version`   | Update version in spec file, Cargo.toml, and pyproject.toml |
| `dist`           | Create source and vendor tarballs in `packaging/rpm/`        |
| `checksums`      | Generate `.sha256` checksums for tarballs (depends on `dist`) |
| `sign`           | Sign tarballs with GPG (requires `GPG_KEY`, depends on `dist`) |
| `tag`            | Create and push git tag `vX.Y.Z`                            |
| `release-gh`     | Create GitHub Release with artifacts and changelog           |
| `publish-crates` | Publish the crate to crates.io                              |
| `publish-pypi`   | Publish the Python wheel to PyPI                            |
| `release`        | Full pipeline: commit → tag → dist → checksums → sign → release-gh → publish-crates → publish-pypi |
| `release-branch` | Create `release/X.Y.X` branch from source tag              |

### RPM Release (on `release/X.Y.X` branch)

| Target           | Purpose                                                    |
|------------------|------------------------------------------------------------|
| `rpm-tag`        | Create and push `rpm/X.Y.Z-N` tag on current branch        |
| `rpm-release`    | Full RPM pipeline: rpm-tag → dist → checksums → sign → osc-sync → osc-add → osc-commit |
| `osc-sync`       | Sync spec/changes to OBS checkout                           |
| `osc-add`        | Add/remove files in OBS checkout                            |
| `osc-commit`     | Commit changes to OBS                                       |

### Development & Testing

| Target           | Purpose                                                    |
|------------------|------------------------------------------------------------|
| `vendor`         | Vendor dependencies + save `.cargo/config.vendor.toml`      |
| `test-vendor`    | Build and test with vendored deps (no network access)      |
| `build`          | Build using crates.io (no vendor step needed)              |
| `test`           | Run tests using crates.io                                  |
| `commit`         | Quality gates: test, clippy, format, reuse, check-manpages  |
| `wheel`          | Build Python wheel with maturin                            |

### Key Variables

| Variable     | Default                         | Purpose                                      |
|-------------|---------------------------------|----------------------------------------------|
| `VERSION`   | Read from spec file             | Current package version                       |
| `GPG_KEY`   | `ferroclass@michael-jansen.biz`       | GPG key ID for signing release tarballs       |
| `GH_REPO`   | `jansenm/ferroclass`            | GitHub repository for `gh release create`     |
| `GH_REMOTE` | `github`                        | Git remote name for GitHub (used by `tag`)    |
| `MATURIN`   | `maturin`                       | maturin binary for Python wheel builds        |
| `OBS_USER`   | Auto-detected from `~/.config/osc/oscrc` | OBS username                      |
| `OBS_PROJECT`| `home:$(OBS_USER):ferroclass`  | OBS project name                              |

Override any variable on the command line:
`make release GPG_KEY=0x12345678 GH_REPO=other/repo`

---

## Vendored Sources Architecture

The `.cargo/config.toml` committed in the repo contains **only build flags**
(`[build] rustflags`), not the vendored-sources replacement. This allows
`cargo build`, `cargo test`, and `cargo publish` to work with crates.io
directly without a vendor step.

For RPM builds and offline builds:

1. `make vendor` — runs `cargo vendor` to create `vendor/` and saves the
   source replacement config to `.cargo/config.vendor.toml` (gitignored).
2. `make test-vendor` — builds and tests with `cargo --config .cargo/config.vendor.toml`.
3. `make dist` — creates the **vendor tarball** with a merged `.cargo/config.toml`
   that contains both the build flags (from committed config) and the vendored
   source replacement (from `config.vendor.toml`). The RPM `%autosetup -p1 -a1`
   overlays this complete config on top of the source tarball.

---

## Release Checklist

### 1. Bump version

```shell
make bump-version VERSION_NEW=X.Y.Z
```

This updates `packaging/rpm/ferroclass.spec`, `Cargo.toml`, and `pyproject.toml`.

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

### 4. Update the spec %changelog

Add a matching entry to the `%changelog` section of `packaging/rpm/ferroclass.spec`.

### 5. Regenerate man pages

```shell
make manpages
```

### 6. Run quality gates

```shell
make commit
```

This runs `cargo test`, `cargo clippy`, `cargo fmt`, `reuse lint`, and `check-manpages`.

### 7. Create the source release

```shell
make release
```

This runs the full pipeline (on `main` branch):
1. `commit` — quality gates
2. `tag` — create and push git tag `vX.Y.Z`
3. `dist` — create source and vendor tarballs (from the tag)
4. `checksums` — generate SHA256 checksums
5. `sign` — GPG-sign the tarballs
6. `release-gh` — create GitHub Release with artifacts and changelog
7. `publish-crates` — publish to crates.io
8. `publish-pypi` — publish Python wheel to PyPI

### 8. Create the release branch

```shell
make release-branch
```

This creates `release/X.Y.X` from the source tag and pushes it.

### 9. Switch to the release branch and publish RPMs

```shell
git checkout release/X.Y.X
make rpm-release
```

This runs the full RPM pipeline (on `release/X.Y.X` branch):
1. `rpm-tag` — create and push `rpm/X.Y.Z-1` tag
2. `dist` — create source and vendor tarballs (from the source tag, not HEAD)
3. `checksums` — generate SHA256 checksums
4. `sign` — GPG-sign the tarballs
5. `osc-sync` — sync files to OBS checkout
6. `osc-add` — add/remove files in OBS
7. `osc-commit` — commit to OBS

### 10. Build on OBS

```shell
cd ~/obs/home:mjansen1972:ferroclass/ferroclass
make -C packaging/obs osc-build OBS_PROJECT=home:mjansen1972:ferroclass
```

For Rocky 9 (local builds need `--no-verify` because Rocky GPG keys may not be
in the host rpmdb):

```shell
make -C packaging/obs osc-build-rocky9 OBS_PROJECT=home:mjansen1972:ferroclass
```

### 11. RPM Release bumps (if needed)

If the spec needs changes after the initial RPM release:

1. Edit spec/changes on the `release/X.Y.X` branch
2. Bump `Release:` in the spec (e.g., `1` → `2`)
3. Add changelog entries
4. Commit
5. `make rpm-release` (creates `rpm/X.Y.Z-N` tag)

### 12. Merge packaging changes back to main

When done with the release branch:

```shell
git checkout main
git merge release/X.Y.X
git branch -d release/X.Y.X
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
| Binary RPMs for openSUSE, Rocky 9                  | OBS repositories | Distro package installation   |
| Python wheel `ferroclass-X.Y.Z-cp3XX-linux_x86_64.whl` | PyPI         | Python package installation  |

---

## Tag Formats

| Tag Format          | Created By      | Branch          | Purpose                        |
|---------------------|-----------------|-----------------|--------------------------------|
| `vX.Y.Z`            | `make tag`      | `main`          | Source release                  |
| `rpm/X.Y.Z-N`       | `make rpm-tag`  | `release/X.Y.X` | RPM packaging release           |

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
  - `Y` = minor (new features, new adapters, new CLI flags, new Python API)
  - `Z` = patch (bug fixes, doc fixes, dependency updates)
- **Pre-release tags**: `1.0.0-alpha.1`, `1.0.0-beta.2` — append to Cargo.toml
  version but NOT to the spec `Version:` (RPM spec does not support pre-release
  strings)
- **First release of a new version** always gets `Release: 1%{?dist}`
- **Hotfix/hot patch** bumps `Release` without changing `Version`

---

## What to NOT Do

1. **Do NOT bump version in only one file.** If Cargo.toml, pyproject.toml, and
   the spec differ, the release will be inconsistent.
2. **Do NOT skip `make commit` before tagging.** The tag represents the exact
   commit that passed all quality gates.
3. **Do NOT forget to regenerate man pages.** The `man/*.1` files embed the
   version.
4. **Do NOT publish a tag without pushing it.** `git push $(GH_REMOTE) vX.Y.Z` must
   succeed before the release is considered public.
5. **Do NOT ignore spec `%changelog`.** Even for internal releases, changelog
   entries are required for RPM builds and audit trails.
6. **Do NOT use GitHub Actions to build release artifacts.** The release process
   is manual by design to minimize supply chain attack surface.
7. **Do NOT create RPM tags on `main`.** RPM tags (`rpm/X.Y.Z-N`) belong on
   `release/X.Y.X` branches only.
8. **Do NOT modify committed files during builds.** The `.cargo/config.toml`
   in the repo must not be overwritten. Vendor builds use `--config` or the
   vendor tarball's merged config.

---

## OBS Project Configuration

The OBS project `home:mjansen1972:ferroclass` is configured with these
repositories:

| Repository            | Path                     | Architectures  |
|-----------------------|--------------------------|----------------|
| openSUSE_Tumbleweed  | openSUSE:Factory/standard | x86_64        |
| RockyLinux_9          | RockyLinux:9/standard     | x86_64        |

aarch64 is disabled until snapshot test determinism is fixed.

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
PY_V=$(grep '^version' pyproject.toml | head -1 | sed 's/.*= * "//;s/".*//')
[ "$TOML_V" = "$SPEC_V" ] && [ "$TOML_V" = "$PY_V" ] && echo "OK: v$TOML_V" || echo "MISMATCH: Cargo=$TOML_V Spec=$SPEC_V Py=$PY_V"

# Quality gates
make commit

# Build release tarball
make dist

# Verify tarball contents
tar tzf packaging/rpm/ferroclass-$SPEC_V.tar.gz | head

# Verify vendor tarball contains merged config
tar tzf packaging/rpm/ferroclass-$SPEC_V-vendor.tar.gz | head
```
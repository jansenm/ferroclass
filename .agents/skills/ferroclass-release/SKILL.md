<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

---
name: ferroclass-release
description: Release process for ferroclass. Use when preparing a new version, building release artifacts, creating GitHub Releases, or syncing to OBS. Covers version bumping, tarball creation, checksums, GPG signing, tagging, publication, and RPM release branch workflow including post-tag change handling.
---

# Ferroclass Release Process

## Overview

Ferroclass uses a **hybrid release strategy** with **separate branches** for source
and RPM packaging:

- **`main` branch** — Source code development. Source releases (`vX.Y.Z` tags)
  are created here.
- **`release/X.Y.Z` branches** — RPM packaging. Created from the source tag;
  contain spec/changes tweaks, Makefile fixes, and any other post-tag changes.
  RPM tags (`rpm/X.Y.Z-N`) are created here.

Branch naming uses the **exact version** (`release/0.12.0`, not `release/0.12.X`)
so that patch releases get distinct branches (`release/0.12.0` vs
`release/0.12.1`).

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

## The Immutability Rule

**Once a source tag (`vX.Y.Z`) is pushed, it is immutable.** Never delete and
re-push a source tag. If a bug is discovered after pushing the tag:

1. The bug fix goes on the **release branch** (`release/X.Y.Z`), not `main`.
2. The source tarball in the GitHub Release is rebuilt from the tag
   (source code is correct; only packaging/tooling needed fixing).
3. The vendor tarball is rebuilt using the fixed working tree on the release
   branch.
4. The RPM tag (`rpm/X.Y.Z-N`) captures the exact packaging state.

This ensures the `vX.Y.Z` tag always points to the same commit, giving a
stable reference for the source tarball and enabling traceability.

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
| `release-branch` | Create `release/X.Y.Z` branch from source tag              |

### RPM Release (on `release/X.Y.Z` branch)

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

### Phase 1: Source Release (on `main`)

#### 1. Bump version

```shell
make bump-version VERSION_NEW=X.Y.Z
```

This updates `packaging/rpm/ferroclass.spec`, `Cargo.toml`, and `pyproject.toml`.

#### 2. Update CHANGELOG.md

Add a new section `[X.Y.Z] - YYYY-MM-DD` following the Keep a Changelog format.
This is manual — the changelog must be written by a human.

#### 3. Update the SUSE changelog

Add a new entry to `packaging/rpm/ferroclass.changes`:

```
-------------------------------------------------------------------
Day Mon DD YYYY Author <email> - X.Y.Z-1

- Summary of changes
```

#### 4. Update the spec %changelog

Add a matching entry to the `%changelog` section of `packaging/rpm/ferroclass.spec`.

#### 5. Regenerate man pages

```shell
make manpages
```

#### 6. Run quality gates

```shell
make commit
```

This runs `cargo test`, `cargo clippy`, `cargo fmt`, `reuse lint`, and `check-manpages`.

#### 7. Commit and push

```shell
git add -A
git commit -m "Release vX.Y.Z"
git push github main
```

#### 8. Create and push the source tag

```shell
make tag
```

This creates and pushes `vX.Y.Z`.

#### 9. Create the release branch

```shell
make release-branch
```

This creates `release/X.Y.Z` from the source tag and pushes it.

**STOP HERE if `make dist` fails or a bug is discovered.** The source tag is
immutable. Any fixes go on the release branch — see Phase 2.

#### 10. Create tarballs

```shell
make dist
```

This creates source and vendor tarballs. The source tarball is archived from
the tag; the vendor tarball uses the working tree.

#### 11. Generate checksums and sign

```shell
make checksums
make sign
```

#### 12. Create GitHub Release

```shell
make release-gh
```

#### 13. Publish to crates.io and PyPI

```shell
make publish-crates
make publish-pypi
```

### Phase 2: RPM Release (on `release/X.Y.Z` branch)

#### 1. Switch to the release branch

```shell
git checkout release/X.Y.Z
```

#### 2. Cherry-pick any post-tag fixes from main

If bugs were discovered after pushing the source tag (e.g., `make dist` failed,
Makefile had a bug, spec needed a tweak), the fix was committed on `main` and
needs to be cherry-picked onto the release branch:

```shell
git cherry-pick <commit-hash>
```

Alternatively, make the fix directly on the release branch if it wasn't needed
on `main`.

**Important:** All changes after the source tag belong on the release branch,
not on `main`. The only exception is if the bug also affects `main` (e.g., a
Makefile bug that would break future releases too) — in that case, fix on
`main` first, then cherry-pick to the release branch.

#### 3. Run the RPM release

```shell
make rpm-release
```

This runs the full RPM pipeline:
1. `rpm-tag` — create and push `rpm/X.Y.Z-1` tag
2. `dist` — create source and vendor tarballs (source from the tag, vendor from working tree)
3. `checksums` — generate SHA256 checksums
4. `sign` — GPG-sign the tarballs
5. `osc-sync` — sync files to OBS checkout
6. `osc-add` — add/remove files in OBS
7. `osc-commit` — commit to OBS

#### 4. Build on OBS

```shell
cd ~/obs/home:mjansen1972:ferroclass/ferroclass
make -C packaging/obs osc-build OBS_PROJECT=home:mjansen1972:ferroclass
```

For Rocky 9 (local builds need `--no-verify` because Rocky GPG keys may not be
in the host rpmdb):

```shell
make -C packaging/obs osc-build-rocky9 OBS_PROJECT=home:mjansen1972:ferroclass
```

#### 5. RPM Release bumps (if needed)

If the spec needs changes after the initial RPM release:

1. Edit spec/changes on the `release/X.Y.Z` branch
2. Bump `Release:` in the spec (e.g., `1` → `2`)
3. Add changelog entries
4. Commit
5. `make rpm-release` (creates `rpm/X.Y.Z-N` tag)

#### 6. Merge packaging changes back to main

When done with the release branch:

```shell
git checkout main
git merge release/X.Y.Z
git push github main
git branch -d release/X.Y.Z
git push github --delete release/X.Y.Z   # optional: delete remote branch
```

---

## Post-Tag Change Workflow

This is the critical workflow for handling bugs discovered **after the source
tag has been pushed** but **before the RPM build succeeds**.

### The Problem

You pushed `vX.Y.Z`, then discovered:
- `make dist` fails (e.g., Makefile bug)
- The vendor tarball is wrong
- The spec has a typo
- A dependency needs patching

You cannot move the source tag (immutability rule). But the RPM build needs
the fix.

### The Solution

```
1. Fix the bug on main (if it affects main too) and commit.
2. Create release/X.Y.Z from the tag:  make release-branch
3. Switch to release branch:           git checkout release/X.Y.Z
4. Cherry-pick the fix:                git cherry-pick <hash>
5. Rebuild tarballs:                   make dist
6. Continue with RPM release:           make rpm-release
```

The source tarball (`git archive vX.Y.Z`) is from the immutable tag — the
source code is correct. The vendor tarball is rebuilt using the fixed working
tree on the release branch. The RPM tag (`rpm/X.Y.Z-1`) captures the exact
packaging state including all fixes.

### Example: 0.12.0 Release

```
1. Committed "Release v0.12.0" → aa7015c
2. Pushed tag v0.12.0 → points to aa7015c (IMMUTABLE)
3. make dist → FAILED (Makefile used '.' instead of $(CURDIR) for tar -C)
4. Fixed Makefile on main → eed7672
5. Created release/0.12.0 from v0.12.0
6. Cherry-picked eed7672 onto release/0.12.0
7. make dist → SUCCESS (vendor tarball now correct)
8. Continued with make rpm-release
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

| Tag Format          | Created By      | Branch            | Purpose                        |
|---------------------|-----------------|-------------------|--------------------------------|
| `vX.Y.Z`            | `make tag`      | `main`            | Source release (IMMUTABLE)     |
| `rpm/X.Y.Z-N`       | `make rpm-tag`  | `release/X.Y.Z`   | RPM packaging release          |

---

## Branch Formats

| Branch Format       | Created By          | Parent           | Purpose                            |
|---------------------|---------------------|------------------|------------------------------------|
| `release/X.Y.Z`     | `make release-branch` | `vX.Y.Z` tag   | RPM packaging and post-tag fixes   |

Patch releases get distinct branches: `release/0.12.0`, `release/0.12.1`, etc.

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
   `release/X.Y.Z` branches only.
8. **Do NOT modify committed files during builds.** The `.cargo/config.toml`
   in the repo must not be overwritten. Vendor builds use `--config` or the
   vendor tarball's merged config.
9. **Do NOT delete and re-push a source tag.** Source tags are immutable.
   If a bug is found after tagging, fix it on the release branch.
10. **Do NOT make post-tag changes on `main` for packaging-only issues.**
    Changes that only affect RPM packaging (spec, Makefile dist target, vendor
    config) belong on the release branch. Only fix on `main` if the bug also
    affects non-packaging workflows (e.g., `make build` would also break).

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

# Verify the merged config in the vendor tarball
tar xzf packaging/rpm/ferroclass-$SPEC_V-vendor.tar.gz -O .cargo/config.toml
```
<!-- SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz> -->
<!-- SPDX-License-Identifier: MPL-2.0 -->

---
name: rust-rpm-packaging
description: Package Rust projects as RPMs for openSUSE/SUSE and RHEL/Fedora. Covers spec file authoring, vendor tarball creation, obs-build (chroot) and rpmbuild (local) workflows, OBS service integration, and SUSE-specific macros like %cargo_build/%cargo_test. Use when creating or fixing RPM specs, Makefiles for packaging, setting up CI builds, or troubleshooting RPM build failures for Rust crates.
---

# Rust RPM Packaging for openSUSE/SUSE & RHEL/Fedora

## Overview

This skill covers packaging Rust projects as RPMs targeting:
- **openSUSE Tumbleweed/Leap** (primary target)
- **SUSE SLE** (secondary)
- **RHEL/Fedora** (tertiary, conditional support)

The key challenge with Rust RPMs is vendoring dependencies for offline/reproducible builds. Two approaches are supported:

1. **Manual vendoring** — `cargo vendor` + tarball (current approach, simple, works everywhere)
2. **OBS service** — `obs-service-cargo` automates vendoring in OBS (recommended for submission)

---

## Spec File Template

```spec
# SPDX-FileCopyrightText: YEAR Copyright Holder <email>
# SPDX-License-Identifier: MPL-2.0

Name:           package-name
Version:        X.Y.Z
Release:        1%{?dist}
Summary:        One-line description

License:        MPL-2.0
URL:            https://github.com/org/repo
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.gz

# Fallback macros when cargo-packaging / rust-packaging is not installed.
# These are defined by cargo-packaging (SUSE) or rust-packaging (Fedora/RHEL).
%if !0%{?rust_arches:1}
%define rust_arches x86_64 i586 i686 armv6hl armv7hl aarch64 ppc64 powerpc64 ppc64le powerpc64le riscv64 s390x
%endif

ExclusiveArch:  %{rust_arches}

%if 0%{?suse_version}
BuildRequires:  cargo-packaging
BuildRequires:  cargo
%else
BuildRequires:  rust-packaging
BuildRequires:  cargo
%endif

%description
Longer description of the package.

%prep
%autosetup -a1

%build
%if 0%{?cargo_build:1}
%cargo_build
%else
cargo build --release --frozen %{?_smp_mflags}
%endif

%check
%if 0%{?cargo_test:1}
%cargo_test --release
%else
cargo test --release --frozen
%endif

%install
install -d %{buildroot}%{_bindir}
install -m 0755 target/release/binary-name %{buildroot}%{_bindir}/binary-name

%files
%license LICENSES/MPL-2.0.txt
%doc README.md
%{_bindir}/binary-name
```

---

## Key Spec Patterns Explained

### Conditional Macros (`%if 0%{?cargo_build:1}`)

SUSE provides `%cargo_build`, `%cargo_test`, `%cargo_install`, and `%rust_arches` via the `cargo-packaging` package. Fedora/RHEL provides similar macros via `rust-packaging`. When building locally with `rpmbuild --nodeps` (or on a machine without these packages installed), the macros are unavailable. The conditional pattern provides a fallback:

```spec
%if 0%{?cargo_build:1}
%cargo_build
%else
cargo build --release --frozen %{?_smp_mflags}
%endif
```

**How it works:** `%{?cargo_build:1}` expands to `1` if the macro is defined, otherwise to nothing. `0%{?cargo_build:1}` then becomes `01` (truthy) or `0` (falsy).

### `%rust_arches` Fallback

Same pattern — define it if missing:

```spec
%if !0%{?rust_arches:1}
%define rust_arches x86_64 i586 i686 armv6hl armv7hl aarch64 ppc64 powerpc64 ppc64le riscv64 s390x
%endif
ExclusiveArch: %{rust_arches}
```

The value matches what `cargo-packaging` defines on SUSE Tumbleweed.

### `ExclusiveArch`

Rust edition 2024+ requires specific architectures. Use `%rust_arches` when available, fallback to an explicit list. The list should match the architectures where Rust toolchains are available in the target distribution.

### BuildRequires Conditionals

```spec
%if 0%{?suse_version}
BuildRequires:  cargo-packaging
BuildRequires:  cargo
%else
BuildRequires:  rust-packaging
BuildRequires:  cargo
%endif
```

- **SUSE** (`%suse_version` defined): `cargo-packaging` provides RPM macros
- **RHEL/Fedora** (no `%suse_version`): `rust-packaging` provides equivalent macros
- Both pull in `cargo` as a dependency

### Vendor Tarball Setup

`Source1` is the vendor tarball. `%autosetup -a1` unpacks both `Source0` and `Source1`:
- Source0 extracts to `%{name}-%{version}/`
- Source1 (`-a1`) extracts on top, adding `vendor/` and `.cargo/config.toml`

The `.cargo/config.toml` must contain:
```toml
[source.crates-io]
replace-with = "vendored-sources"

[source.vendored-sources]
directory = "vendor/"
```

### Snapshot Tests and Absolute Paths

If the project uses snapshot tests (e.g., `insta`) that contain absolute file paths, they will fail in RPM builds because the build directory differs from the source directory. Fix this by normalizing paths in test code:

```rust
fn normalize_uris(output: &str) -> String {
    output.replace(env!("CARGO_MANIFEST_DIR"), "$PROJECT_ROOT")
}

#[test]
fn test_something() {
    let output = generate_output();
    insta::assert_snapshot!("something", normalize_uris(&output));
}
```

### Sub-packages

For multi-binary Rust projects:

```spec
%package -n sub-package-name
Summary:        Short description
Requires:       %{name} = %{version}-%{release}
Supplements:    %{name}

%description -n sub-package-name
Longer description.

%files -n sub-package-name
%{_bindir}/sub-binary
```

Use `Supplements: %{name}` so the sub-package is automatically installed with the main package but can also be installed independently.

---

## Macro Reference (from `cargo-packaging` on SUSE)

| Macro | Expansion |
|-------|-----------|
| `%rust_arches` | `x86_64 i586 i686 armv6hl armv7hl aarch64 ppc64 powerpc64 ppc64le powerpc64le riscv64 s390x` |
| `%rust_tier1_arches` | `x86_64 aarch64` |
| `%__cargo` | `CARGO_INCREMENTAL=0 CARGO_FEATURE_VENDORED=1 RUSTFLAGS="..." CARGO_TARGET_DIR=... cargo` |
| `%cargo_build` | `%{__cargo} build --offline --locked --release` |
| `%cargo_test` | `%{__cargo} test --offline --locked --no-fail-fast` |
| `%cargo_install` | `%{__cargo} install --offline --locked --no-track --root=...` |

All macros set `CARGO_FEATURE_VENDORED=1` and build with `--offline --locked`.

---

## Makefile for Local RPM Builds

```makefile
VERSION := X.Y.Z
NAME := package-name
TARBALL := $(NAME)-$(VERSION).tar.gz
VENDOR_TARBALL := $(NAME)-$(VERSION)-vendor.tar.gz
SRC_DIR := ../..

.PHONY: tarball vendor tarball-clean rpm rpm-local rpm-obs clean

tarball: $(TARBALL) $(VENDOR_TARBALL)

vendor:
	cd $(SRC_DIR) && cargo vendor --versioned-dirs vendor/
	mkdir -p .cargo
	cd $(SRC_DIR) && cargo vendor --versioned-dirs vendor/ > .cargo/config.toml

$(TARBALL): vendor
	git -C $(SRC_DIR) archive --format=tar.gz --prefix=$(NAME)-$(VERSION)/ HEAD > $(TARBALL)

$(VENDOR_TARBALL): vendor
	cd $(SRC_DIR) && tar czf $(CURDIR)/$(VENDOR_TARBALL) vendor/ .cargo/config.toml

# Build in a chroot using obs-build (preferred for CI/OBS).
# Validates BuildRequires in an isolated environment.
rpm-obs: $(TARBALL) $(VENDOR_TARBALL)
	build --clean --stage=bb \
		--dist tumbleweed \
		--repo zypp:// \
		--root /var/tmp/build-root \
		$(NAME).spec

# Build using local rpmbuild without dependency validation.
# Requires cargo (and BuildRequires) already installed on the host.
rpm-local: $(TARBALL) $(VENDOR_TARBALL)
	rpmbuild -bb --nodeps $(NAME).spec \
		--define "_sourcedir $(CURDIR)" \
		--define "_specdir $(CURDIR)" \
		--define "_builddir $(CURDIR)/BUILD" \
		--define "_rpmdir $(CURDIR)/RPMS" \
		--define "_buildrootdir $(CURDIR)/BUILDROOT"

# Default: local rpmbuild
rpm: rpm-local

tarball-clean:
	rm -f $(TARBALL) $(VENDOR_TARBALL)
	rm -rf $(SRC_DIR)/vendor/ $(SRC_DIR)/.cargo/config.toml
	rm -rf BUILD BUILDROOT RPMS

clean: tarball-clean
	rm -rf /var/tmp/build-root
```

### Key Makefile Notes

- **`vendor` target**: Runs `cargo vendor --versioned-dirs` and generates `.cargo/config.toml`. The `--versioned-dirs` flag creates versioned directory names in `vendor/` (e.g., `serde-1.0.200/`) which is the SUSE convention.
- **`rpm-local`**: Uses `rpmbuild --nodeps` because the host has cargo via rustup, not as an RPM. Bypasses BuildRequires checking.
- **`rpm-obs`**: Uses the SUSE `build` tool (from the `obs-build` package) which creates a chroot, installs all BuildRequires from repos, and builds in isolation. This validates that the spec's BuildRequires are complete and correct.
- **`--repo zypp://`**: Uses the local zypp repositories (all enabled repos on the build host). Alternative: use `--dist tumbleweed` to use the official Tumbleweed repo only.
- **`--nodeps`**: Required for local builds because `cargo-packaging` may not be installed as an RPM (common with rustup installs).

### `.gitignore` Additions

```
# RPM packaging artifacts
packaging/rpm/BUILD/
packaging/rpm/BUILDROOT/
packaging/rpm/RPMS/
packaging/rpm/*.tar.gz
packaging/rpm/.cargo/
```

---

## OBS Service Integration (for submission)

When submitting to OBS, replace the manual vendor tarball with `obs-service-cargo`:

### `_service` file

```xml
<services>
  <service name="download_files" mode="manual" />
  <service name="cargo_vendor" mode="manual">
     <param name="src">ferroclass-X.Y.Z.tar.gz</param>
     <param name="compression">gz</param>
     <param name="update">true</param>
  </service>
</services>
```

**Note:** The project uses a template file (`packaging/obs/_service.in`) with `@VERSION@`
instead of a hardcoded version. The `osc-sync` Makefile target reads the version from the
spec file and generates the actual `_service` from this template. See "In-Tree OBS
Configuration" below.

### Spec changes for OBS

When using `obs-service-cargo`, the spec changes are minimal:

1. **Remove `Source1`** — the vendor tarball is generated by OBS
2. **Keep `%autosetup -a1`** — OBS produces the same structure (`vendor/` + `.cargo/config.toml`)
3. **Add `_service` file** to the OBS package

The `obs-service-cargo` package provides:
- `cargo_vendor` — runs `cargo vendor`, generates vendor tarball with `.cargo/config` and `Cargo.lock`
- `cargo_audit` — audits dependencies for RUSTSEC advisories
- Automatic lockfile management
- `i-accept-the-risk` parameter for known advisories

### Vendor vs Registry method

| Aspect | Vendor | Registry |
|--------|--------|----------|
| Tarball name | `vendor.tar.zst` (or `.gz`) | `registry.tar.zst` |
| Contents | `vendor/` + `.cargo/config` + locks | `.cargo/registry/` + locks |
| `%prep` | `%autosetup -a1` | `%autosetup -a1` |
| `%build` | `%cargo_build` | `export CARGO_HOME=$PWD/.cargo && %cargo_build` |
| Patching deps | Easy (versioned dirs) | Requires extracting `.crate` files |
| Multi-crate | Multiple tarballs with `--tag` | Single tarball |

### In-Tree OBS Configuration (`packaging/obs/`)

The project maintains OBS configuration in `packaging/obs/` alongside the
RPM spec in `packaging/rpm/`. This avoids manually editing `_service` files
in an OBS checkout directory.

| File                            | Purpose                                                           |
| ------------------------------ | ----------------------------------------------------------------- |
| `packaging/obs/_service.in`      | Template for OBS `_service` file (version substituted from spec)  |
| `packaging/obs/Makefile`         | `osc-*` targets for checkout, sync, build, and commit            |
| `packaging/obs/.gitignore`      | Ignores generated `_service` file                                 |

The `_service.in` template uses `@VERSION@` as a placeholder. The
`osc-sync` Makefile target reads the version from `packaging/rpm/ferroclass.spec`
and generates the actual `_service` file with the correct tarball name:

```shell
# From packaging/obs/:
make osc-sync

# Or from the project root:
make osc-sync
```

**Workflow:**

1. `make osc-sync` — copies `ferroclass.spec` and `ferroclass.changes` from
   `packaging/rpm/`, generates `_service` from `_service.in`, and writes all
   three files to `$(OBS_DIR)`.
2. `cd $(OBS_DIR) && osc addremove && osc commit` — submit to OBS.
3. OBS runs `cargo_vendor` automatically, or trigger manually with
   `osc service manualrun`.

**OBS directory variables** (configurable in `packaging/obs/Makefile`):

| Variable       | Default                           | Description                      |
| -------------- | --------------------------------- | -------------------------------- |
| `OBS_PROJECT`  | `home:$USER:ferroclass`           | OBS project name                 |
| `OBS_PACKAGE`  | `ferroclass`                      | OBS package name                 |
| `OBS_DIR`      | `~/obs/$OBS_PROJECT/$OBS_PACKAGE` | Local OBS checkout directory     |

Override on the command line: `make osc-sync OBS_PROJECT=home:jansenm:rust`

---

## Common Build Failures and Fixes

### `Architecture is not included: x86_64`

The `%rust_arches` macro is undefined because `cargo-packaging` is not installed.

**Fix:** Use the fallback pattern (see spec template above).

### `Bad exit status from ... (%build)` with `%cargo_build`

The `%cargo_build` macro is undefined because `cargo-packaging` is not installed.

**Fix:** Use the conditional pattern with fallback to plain `cargo` commands.

### `have choice for libasan8/libasan8-gcc15`

The `build` tool (obs-build) cannot resolve ambiguous package dependencies. This happens when the Tumbleweed dist config lacks preference hints for the latest gcc version.

**Fix:** This is a Tumbleweed config issue, not a package issue. Workarounds:
1. Use `rpm-local` (`rpmbuild --nodeps`) which bypasses dependency resolution
2. Wait for Tumbleweed to update their config with `Prefer: -libasan8-gcc15 -libtsan2-gcc15`
3. Create a local config overlay and pass it with `--configdir`

### Snapshot test failures with absolute paths

RPM builds extract source to a different directory (e.g., `/home/user/rpmbuild/BUILD/pkg-version/`), causing snapshot tests that embed `CARGO_MANIFEST_DIR` to fail.

**Fix:** Normalize paths in test code by replacing `env!("CARGO_MANIFEST_DIR")` with a fixed token before snapshot comparison.

### `cargo build --frozen` fails with version conflict

The `Cargo.lock` in the tarball may reference versions not in the vendor directory.

**Fix:** Run `cargo vendor` with `--versioned-dirs` after ensuring `Cargo.lock` is committed and up to date. Always use `--frozen` (not `--locked`) in the spec — `--frozen` prevents any network access.

---

## Licensing and REUSE Compliance (SPDX)

All `.rs` files must have SPDX headers:

```rust
// SPDX-FileCopyrightText: YEAR Copyright Holder <email>
// SPDX-License-Identifier: MPL-2.0
```

Non-source files (Cargo.toml, snapshots, etc.) are covered by the wildcard rule in `REUSE.toml`. The `LICENSES/` directory must contain the full license text.

---

## Checklist for New Rust RPM Package

- [ ] Spec file with conditional macros and SUSE/RHEL BuildRequires
- [ ] `ExclusiveArch: %{rust_arches}` with fallback definition
- [ ] Vendor tarball with `cargo vendor --versioned-dirs`
- [ ] `.cargo/config.toml` with vendored-sources redirect
- [ ] `%autosetup -a1` in `%prep`
- [ ] `%build` with `%cargo_build` fallback
- [ ] `%check` with `%cargo_test` fallback
- [ ] `%install` with explicit `install` commands (not `%cargo_install` for custom binaries)
- [ ] `%files` section with `%license` and `%doc`
- [ ] Makefile with `rpm-local` and `rpm-obs` targets
- [ ] `.gitignore` for build artifacts
- [ ] `.changes` file for SUSE changelog
- [ ] Snapshot tests normalized for portable paths (if applicable)
- [ ] SPDX headers on all `.rs` files
- [ ] `REUSE.toml` for non-source file licensing
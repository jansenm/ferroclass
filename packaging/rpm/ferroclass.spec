# SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
# SPDX-License-Identifier: MPL-2.0

Name:           ferroclass
Version:        0.11.1
Release:        1%{?dist}
Summary:        Hierarchical inventory management tool (reclass compatible)

# Ferroclass is MPL-2.0. Vendored dependencies have their own licenses;
# see cargo-vendor.txt for the full breakdown.
License:        MPL-2.0 AND Apache-2.0 AND BSD-3-Clause AND MIT
URL:            https://github.com/jansenm/ferroclass
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.gz

# Ferroclass requires Rust edition 2024 (Rust >= 1.85.0).
# Only x86_64 is enabled for now. aarch64 builds fail on snapshot
# tests due to HashMap iteration order differences (peers map ordering).
# Enable aarch64 only after sorting snapshot tests with sorted=true.
ExclusiveArch:  x86_64

# Build requirements: cargo is available in all target distros.
# SUSE additionally needs cargo-packaging which sets up vendored sources
# via .cargo/config.toml. On other distros, the vendor tarball (Source1)
# already contains .cargo/config.toml, so no extra setup is needed.
# cargo-rpm-macros and rust-packaging are NOT required — the spec uses
# plain cargo commands as fallbacks when the macros are unavailable.
%if 0%{?suse_version}
BuildRequires:  cargo-packaging
BuildRequires:  cargo
%else
BuildRequires:  cargo
%endif

# The Python subpackage (python3-ferroclass) requires maturin to build the
# PyO3 wheel. On SUSE, maturin is shipped as versioned python3XX-maturin
# packages — use the %%{python_module} macro which resolves to the correct
# flavor. On other distros, maturin may not be available in the base repos
# (e.g. Rocky 9 EPEL does not carry it). Define --with python_subpackage
# to opt in on platforms where maturin is available; the build will fail
# with unresolvable BuildRequires if maturin is missing.
%if 0%{?suse_version}
%define _with_python_subpackage 1
BuildRequires:  %{python_module devel}
BuildRequires:  %{python_module maturin}
BuildRequires:  %{python_module pip}
BuildRequires:  python-rpm-macros
%endif
%if 0%{?_with_python_subpackage}
%if 0%{!?suse_version}
BuildRequires:  python3-devel
BuildRequires:  python3-pip
BuildRequires:  maturin
%endif
%endif

# Debug packages are disabled because Cargo's release profile strips
# DWARF debug info by default (implicit strip = "debuginfo"). The
# binaries have no .debug_* sections, so find-debuginfo would produce
# empty packages. Additionally, both the Rust binaries and the maturin
# .so extension would collide on the same ferroclass-debuginfo package
# name. To enable proper debuginfo packages, add debug = 2 to the
# Cargo release profile and split debuginfo into per-binary subpackages.
%define debug_package %{nil}

# The ferroclass binary provides the core inventory and node-info
# functionality. All three binaries (ferroclass, ferroclass-ansible,
# ferroclass-salt) are built from the same crate and share the same library
# code, but the adapter binaries are packaged separately so they can be
# installed independently on systems that only need Ansible or Salt
# integration.

%description
Ferroclass is a Rust reimplementation of Python reclass. It provides
hierarchical inventory management with support for class merging,
variable interpolation, inventory queries ($[...]), and multiple
output adapters for Ansible and Salt.

This package provides the ferroclass binary for inventory inspection
and node info queries. Install ferroclass-ansible and/or ferroclass-salt
for the Ansible and Salt adapters respectively.

%package -n ferroclass-ansible
Summary:        Ansible dynamic inventory adapter for ferroclass
Supplements:    %{name}

%description -n ferroclass-ansible
Ansible dynamic inventory adapter that provides --list and --host
output compatible with ansible-inventory. This is the ferroclass
binary; it can coexist with the Python reclass-ansible package.

This package contains only the ferroclass-ansible binary. It can be
installed independently — the main ferroclass package is not required.

%package -n ferroclass-salt
Summary:        Salt pillar and top data adapter for ferroclass
Supplements:    %{name}

%description -n ferroclass-salt
Salt adapter that provides --top (state top data) and --pillar
(pillar data) output. This is the ferroclass binary; it can coexist
with the Python reclass-salt package.

This package contains only the ferroclass-salt binary. It can be
installed independently — the main ferroclass package is not required.

%if 0%{?_with_python_subpackage}
%package -n python3-ferroclass
Summary:        Python bindings for ferroclass (PyO3 native extension)
Requires:       python3 >= 3.9
Supplements:    %{name}

%description -n python3-ferroclass
Native Python extension module for ferroclass, built with PyO3.
Provides the ferroclass Python package with ext_pillar() and top()
functions for Salt integration, plus load() for direct inventory
access.

To use ferroclass as a Salt ext_pillar or master_tops plugin, copy
the adapter modules from %{_datadir}/ferroclass/contrib/ to Salt's
extension_modules directory (see %{_datadir}/ferroclass/contrib/README
for details).

%package -n ferroclass-salt-adapter
Summary:        Salt adapter modules for ferroclass
Requires:       python3-ferroclass
Supplements:    (ferroclass-salt and python3-ferroclass)
BuildArch:      noarch

%description -n ferroclass-salt-adapter
Pure-Python Salt adapter modules that delegate to the ferroclass
native Python extension. Provides ext_pillar (pillar data) and
master_tops (top data) integration with Salt.

These adapter modules must be installed into Salt's extension_modules
directory. They are shipped as reference files in
%{_datadir}/ferroclass/contrib/ — copy or symlink them to the
pillar/ and tops/ subdirectories of your Salt extension_modules path
(typically /var/cache/salt/master/extmods/).

See %{_datadir}/ferroclass/contrib/README for installation instructions.
%endif

%prep
%autosetup -p1 -a1

%build
cargo build --release --frozen %{?_smp_mflags}

%if 0%{?_with_python_subpackage}
# Build Python wheel for the python3-ferroclass subpackage.
# Uses the same vendored sources as the Rust binary build.
maturin build --release --features python --skip-auditwheel --interpreter python3
%endif

# Generate vendored dependency manifest for license compliance.
# Fedora guidelines require cargo-vendor.txt as %%license.
%if 0%{?cargo_vendor_manifest:1}
%cargo_vendor_manifest
%endif

%check
cargo test --release --frozen

%install
install -d %{buildroot}%{_bindir}
install -m 0755 target/release/ferroclass %{buildroot}%{_bindir}/ferroclass
install -m 0755 target/release/ferroclass-ansible %{buildroot}%{_bindir}/ferroclass-ansible
install -m 0755 target/release/ferroclass-salt %{buildroot}%{_bindir}/ferroclass-salt
install -d %{buildroot}%{_mandir}/man1
install -m 0644 man/ferroclass.1 %{buildroot}%{_mandir}/man1/ferroclass.1
install -m 0644 man/ferroclass-ansible.1 %{buildroot}%{_mandir}/man1/ferroclass-ansible.1
install -m 0644 man/ferroclass-salt.1 %{buildroot}%{_mandir}/man1/ferroclass-salt.1

%if 0%{?_with_python_subpackage}
# pip install: --no-compile prevents stale bytecode timestamps (rpmlint
# E: python-bytecode-inconsistent-mtime). RPM's brp-python-bytecompile
# produces consistent .pyc files after all files are in place.
pip install --no-deps --no-compile --root %{buildroot} --prefix %{_prefix} target/wheels/ferroclass-*.whl

# Install Salt adapter modules as reference files for the
# ferroclass-salt-adapter subpackage. Users copy/symlink these
# into Salt's extension_modules directory.
install -d %{buildroot}%{_datadir}/ferroclass/contrib/pillar
install -d %{buildroot}%{_datadir}/ferroclass/contrib/tops
install -m 0644 contrib/pillar/ferroclass_adapter.py %{buildroot}%{_datadir}/ferroclass/contrib/pillar/
install -m 0644 contrib/tops/ferroclass_adapter.py %{buildroot}%{_datadir}/ferroclass/contrib/tops/
install -m 0644 contrib/README %{buildroot}%{_datadir}/ferroclass/contrib/
%endif

%files
%license LICENSES/MPL-2.0.txt
%if 0%{?cargo_vendor_manifest:1}
%license cargo-vendor.txt
%endif
%doc README.md
%{_bindir}/ferroclass
%{_mandir}/man1/ferroclass.1*

%files -n ferroclass-ansible
%license LICENSES/MPL-2.0.txt
%{_bindir}/ferroclass-ansible
%{_mandir}/man1/ferroclass-ansible.1*

%files -n ferroclass-salt
%license LICENSES/MPL-2.0.txt
%{_bindir}/ferroclass-salt
%{_mandir}/man1/ferroclass-salt.1*

%if 0%{?_with_python_subpackage}
%files -n python3-ferroclass
%license LICENSES/MPL-2.0.txt
%{python3_sitearch}/ferroclass/
%{python3_sitearch}/ferroclass-*.dist-info/

%files -n ferroclass-salt-adapter
%license LICENSES/MPL-2.0.txt
%dir %{_datadir}/ferroclass
%{_datadir}/ferroclass/contrib/
%endif

%changelog
* Tue May 26 2026 Michael Jansen <ferroclass@michael-jansen.biz> - 0.11.1-1
- Version bump to 0.11.1
- Add python3-ferroclass subpackage with PyO3 native extension (SUSE only;
  maturin not available in Rocky 9 EPEL; opt in with --with python_subpackage)
- Add ferroclass-salt-adapter subpackage (noarch) with Salt adapter
  reference files in /usr/share/ferroclass/contrib/
- Simplify RPM build: remove cargo-rpm-macros/rust-packaging BuildRequires,
  remove %%cargo_prep; use plain cargo commands everywhere
- Fix rpmlint python-bytecode-inconsistent-mtime (--no-compile in pip install)
- Fix rpmlint %%license macro escape in comment
- Disable debug_package (Cargo strips by default; name collision between
  Rust binary and Python .so debuginfo)

* Tue May 19 2026 Michael Jansen <ferroclass@michael-jansen.biz> - 0.10.1-1
- Improve error messages by eliminating pass-through error layers
- Add docs/conventions.md documenting error handling patterns

* Mon May 18 2026 Michael Jansen <ferroclass@michael-jansen.biz> - 0.10.0-1
- Rename binaries to ferroclass/ferroclass-ansible/ferroclass-salt
- Rename RPM sub-packages to ferroclass-ansible and ferroclass-salt

* Sat May 16 2026 Michael Jansen <ferroclass@michael-jansen.biz> - 0.9.0-1
- Initial package (SUSE Tumbleweed, Rocky Linux 9/10, Fedora)
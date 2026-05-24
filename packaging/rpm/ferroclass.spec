# SPDX-FileCopyrightText: 2026 Michael Jansen <ferroclass@michael-jansen.biz>
# SPDX-License-Identifier: MPL-2.0

Name:           ferroclass
Version:        0.11.0
Release:        4%{?dist}
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

%if 0%{?suse_version}
BuildRequires:  cargo-packaging
BuildRequires:  cargo
%else
# RHEL, Rocky Linux, AlmaLinux, Fedora
BuildRequires:  cargo-rpm-macros
BuildRequires:  rust-packaging
BuildRequires:  cargo
%endif

# Python subpackage build requirements
BuildRequires:  python3-devel
BuildRequires:  python3-pip
BuildRequires:  maturin

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

%prep
%autosetup -p1 -a1
%if 0%{?suse_version}
# SUSE: cargo-packaging sets up vendored sources via .cargo/config.toml
%else
# RHEL/Fedora: cargo-rpm-macros needs explicit vendor prep
%cargo_prep -v vendor
%endif

%build
%if 0%{?cargo_build:1}
%cargo_build
%else
cargo build --release --frozen %{?_smp_mflags}
%endif

# Build Python wheel for the python3-ferroclass subpackage.
# Uses the same vendored sources as the Rust binary build.
maturin build --release --features python --skip-auditwheel --interpreter python3

# Generate vendored dependency manifest for license compliance.
# Fedora guidelines require cargo-vendor.txt as %%license.
%if 0%{?cargo_vendor_manifest:1}
%cargo_vendor_manifest
%endif

%check
%if 0%{?cargo_test:1}
%cargo_test
%else
cargo test --release --frozen
%endif

%install
install -d %{buildroot}%{_bindir}
install -m 0755 target/release/ferroclass %{buildroot}%{_bindir}/ferroclass
install -m 0755 target/release/ferroclass-ansible %{buildroot}%{_bindir}/ferroclass-ansible
install -m 0755 target/release/ferroclass-salt %{buildroot}%{_bindir}/ferroclass-salt
install -d %{buildroot}%{_mandir}/man1
install -m 0644 man/ferroclass.1 %{buildroot}%{_mandir}/man1/ferroclass.1
install -m 0644 man/ferroclass-ansible.1 %{buildroot}%{_mandir}/man1/ferroclass-ansible.1
install -m 0644 man/ferroclass-salt.1 %{buildroot}%{_mandir}/man1/ferroclass-salt.1

# Install Python wheel for the python3-ferroclass subpackage.
pip install --no-deps --root %{buildroot} --prefix %{_prefix} target/wheels/ferroclass-*.whl

# Install Salt adapter modules as reference files for the
# ferroclass-salt-adapter subpackage. Users copy/symlink these
# into Salt's extension_modules directory.
install -d %{buildroot}%{_datadir}/ferroclass/contrib/pillar
install -d %{buildroot}%{_datadir}/ferroclass/contrib/tops
install -m 0644 contrib/pillar/ferroclass_adapter.py %{buildroot}%{_datadir}/ferroclass/contrib/pillar/
install -m 0644 contrib/tops/ferroclass_adapter.py %{buildroot}%{_datadir}/ferroclass/contrib/tops/
install -m 0644 contrib/README %{buildroot}%{_datadir}/ferroclass/contrib/

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

%files -n python3-ferroclass
%license LICENSES/MPL-2.0.txt
%{python3_sitearch}/ferroclass/
%{python3_sitearch}/ferroclass-*.dist-info/

%files -n ferroclass-salt-adapter
%license LICENSES/MPL-2.0.txt
%{_datadir}/ferroclass/contrib/

%changelog
* Sun May 24 2026 Michael Jansen <ferroclass@michael-jansen.biz> - 0.11.0-4
- Disable debug_package to fix conflict between stripped Rust binaries
  and stripped Python .so extension (ferroclass-debuginfo already exists)
- Move Salt adapters from python3-ferroclass to ferroclass-salt-adapter
  (noarch); Salt discovers plugins via extension_modules, not Python paths
- Add contrib/README with Salt adapter installation instructions

* Sun May 24 2026 Michael Jansen <ferroclass@michael-jansen.biz> - 0.11.0-3
- Add python3-ferroclass subpackage with PyO3 native extension
- Add ferroclass-salt-adapter subpackage (noarch) with Salt adapter
  reference files; users copy/symlink into Salt extension_modules
- Build Python wheel with maturin during %%build

* Thu May 21 2026 Michael Jansen <ferroclass@michael-jansen.biz> - 0.11.0-2
- Add %%debug_package for proper binary stripping and debuginfo packages
- Escape %%license macro in comment to fix rpmlint warning

* Tue May 19 2026 Michael Jansen <ferroclass@michael-jansen.biz> - 0.10.1-1
- Improve error messages by eliminating pass-through error layers
- Add docs/conventions.md documenting error handling patterns

* Mon May 18 2026 Michael Jansen <ferroclass@michael-jansen.biz> - 0.10.0-1
- Rename binaries to ferroclass/ferroclass-ansible/ferroclass-salt
- Rename RPM sub-packages to ferroclass-ansible and ferroclass-salt

* Sat May 16 2026 Michael Jansen <ferroclass@michael-jansen.biz> - 0.9.0-1
- Initial package (SUSE Tumbleweed, Rocky Linux 9/10, Fedora)
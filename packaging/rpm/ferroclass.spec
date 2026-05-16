# SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
# SPDX-License-Identifier: MPL-2.0

Name:           ferroclass
Version:        0.9.0
Release:        1%{?dist}
Summary:        Hierarchical inventory management tool (reclass compatible)

License:        MPL-2.0
URL:            https://github.com/jansenm/ferroclass
Source0:        %{name}-%{version}.tar.gz
Source1:        %{name}-%{version}-vendor.tar.gz

# Ferroclass requires Rust edition 2024 (Rust >= 1.85.0).
# Fallback if %rust_arches is not defined (e.g. cargo-packaging not installed).
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

# The reclass binary is a drop-in replacement for the Python reclass tool.
# It provides the core inventory and node-info functionality. All three
# binaries (reclass, reclass-ansible, reclass-salt) are built from the
# same crate and share the same library code, but the adapter binaries
# are packaged separately so they can be installed independently on systems
# that only need Ansible or Salt integration.

%description
Ferroclass is a Rust reimplementation of Python reclass. It provides
hierarchical inventory management with support for class merging,
variable interpolation, inventory queries ($[...]), and multiple
output adapters for Ansible and Salt.

This package provides the reclass binary for inventory inspection
and node info queries. Install reclass-ansible and/or reclass-salt
for the Ansible and Salt adapters respectively.

%package -n reclass-ansible
Summary:        Ansible dynamic inventory adapter for ferroclass
Supplements:    %{name}

%description -n reclass-ansible
Ansible dynamic inventory adapter that provides --list and --host
output compatible with ansible-inventory. Drop-in replacement for
the Python reclass-ansible adapter.

This package contains only the reclass-ansible binary. It can be
installed independently — the main ferroclass package is not required.

%package -n reclass-salt
Summary:        Salt pillar and top data adapter for ferroclass
Supplements:    %{name}

%description -n reclass-salt
Salt adapter that provides --top (state top data) and --pillar
(pillar data) output. Drop-in replacement for the Python
reclass-salt adapter.

This package contains only the reclass-salt binary. It can be
installed independently — the main ferroclass package is not required.

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
%cargo_test
%else
cargo test --release --frozen
%endif

%install
install -d %{buildroot}%{_bindir}
install -m 0755 target/release/reclass %{buildroot}%{_bindir}/reclass
install -m 0755 target/release/reclass-ansible %{buildroot}%{_bindir}/reclass-ansible
install -m 0755 target/release/reclass-salt %{buildroot}%{_bindir}/reclass-salt
install -d %{buildroot}%{_mandir}/man1
install -m 0644 man/reclass.1 %{buildroot}%{_mandir}/man1/reclass.1
install -m 0644 man/reclass-ansible.1 %{buildroot}%{_mandir}/man1/reclass-ansible.1
install -m 0644 man/reclass-salt.1 %{buildroot}%{_mandir}/man1/reclass-salt.1

%files
%license LICENSES/MPL-2.0.txt
%doc README.md
%{_bindir}/reclass
%{_mandir}/man1/reclass.1*

%files -n reclass-ansible
%license LICENSES/MPL-2.0.txt
%{_bindir}/reclass-ansible
%{_mandir}/man1/reclass-ansible.1*

%files -n reclass-salt
%license LICENSES/MPL-2.0.txt
%{_bindir}/reclass-salt
%{_mandir}/man1/reclass-salt.1*

%changelog
* Sat May 16 2026 Michael Jansen <mike@michael-jansen.biz> - 0.9.0-1
- Initial package

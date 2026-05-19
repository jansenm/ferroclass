# SPDX-FileCopyrightText: 2026 Michael Jansen <mike@michael-jansen.biz>
# SPDX-License-Identifier: MPL-2.0

Name:           ferroclass
Version:        0.10.1
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
# RHEL, Rocky Linux, AlmaLinux, Fedora
BuildRequires:  rust-packaging
BuildRequires:  cargo
%endif

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
install -m 0755 target/release/ferroclass %{buildroot}%{_bindir}/ferroclass
install -m 0755 target/release/ferroclass-ansible %{buildroot}%{_bindir}/ferroclass-ansible
install -m 0755 target/release/ferroclass-salt %{buildroot}%{_bindir}/ferroclass-salt
install -d %{buildroot}%{_mandir}/man1
install -m 0644 man/ferroclass.1 %{buildroot}%{_mandir}/man1/ferroclass.1
install -m 0644 man/ferroclass-ansible.1 %{buildroot}%{_mandir}/man1/ferroclass-ansible.1
install -m 0644 man/ferroclass-salt.1 %{buildroot}%{_mandir}/man1/ferroclass-salt.1

%files
%license LICENSES/MPL-2.0.txt
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

%changelog
* Tue May 19 2026 Michael Jansen <mike@michael-jansen.biz> - 0.10.1-1
- Improve error messages by eliminating pass-through error layers
- Add docs/conventions.md documenting error handling patterns

* Mon May 18 2026 Michael Jansen <mike@michael-jansen.biz> - 0.10.0-1
- Rename binaries to ferroclass/ferroclass-ansible/ferroclass-salt
- Rename RPM sub-packages to ferroclass-ansible and ferroclass-salt

* Sat May 16 2026 Michael Jansen <mike@michael-jansen.biz> - 0.9.0-1
- Initial package (SUSE Tumbleweed, Rocky Linux 9/10, Fedora)

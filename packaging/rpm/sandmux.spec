# Pacote binário: os executáveis estáticos do tarball da release.
Name:           sandmux
Version:        %{sandmux_version}
Release:        1
Summary:        Multiplexador de terminal para agentes de código em sandbox
License:        MIT OR Apache-2.0
URL:            https://github.com/alanyoshida/sandmux
Source0:        sandmux-%{version}-x86_64-linux.tar.gz
ExclusiveArch:  x86_64
AutoReqProv:    no
%global debug_package %{nil}
%global __strip /bin/true

%description
Roda harnesses de agente (opencode, hermes) e shells confinados por
namespaces, Landlock e seccomp, com abas, daemon de sessões, rede pelo
gateway, cofre de segredos e chat com os modelos cadastrados.

%prep
%setup -q -n sandmux-%{version}-x86_64-linux

%install
mkdir -p %{buildroot}%{_bindir} %{buildroot}%{_sysconfdir}/apparmor.d
for b in sandmux sandmuxd sandmux-run sandmux-chat sandmux-db-mcp; do
    install -m 0755 bin/$b %{buildroot}%{_bindir}/$b
done
sed 's|@SANDMUX_RUN@|%{_bindir}/sandmux-run|' apparmor/sandmux-run \
    > %{buildroot}%{_sysconfdir}/apparmor.d/sandmux-run

%post
if command -v apparmor_parser >/dev/null 2>&1 && [ -d /sys/kernel/security/apparmor ]; then
    apparmor_parser -r %{_sysconfdir}/apparmor.d/sandmux-run || :
fi

%files
%license LICENSE-MIT LICENSE-APACHE
%doc README.md
%{_bindir}/sandmux
%{_bindir}/sandmuxd
%{_bindir}/sandmux-run
%{_bindir}/sandmux-chat
%{_bindir}/sandmux-db-mcp
%config(noreplace) %{_sysconfdir}/apparmor.d/sandmux-run

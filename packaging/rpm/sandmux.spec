# Pacote binário: os executáveis estáticos do tarball da release.
Name:           sandmux
Version:        %{sandmux_version}
Release:        1
Summary:        Multiplexador de terminal para agentes de código em sandbox
License:        MIT
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
mkdir -p %{buildroot}%{_bindir} %{buildroot}%{_sysconfdir}/apparmor.d %{buildroot}/usr/lib/systemd/system
for b in sandmux sandmuxd sandmux-run sandmux-chat sandmux-db-mcp sandmux-helper; do
    install -m 0755 bin/$b %{buildroot}%{_bindir}/$b
done
sed 's|@SANDMUX_RUN@|%{_bindir}/sandmux-run|' apparmor/sandmux-run \
    > %{buildroot}%{_sysconfdir}/apparmor.d/sandmux-run
install -m 0644 systemd/sandmux-helper.socket %{buildroot}/usr/lib/systemd/system/sandmux-helper.socket
sed 's|@SANDMUX_HELPER@|%{_bindir}/sandmux-helper|' systemd/sandmux-helper.service.in \
    > %{buildroot}/usr/lib/systemd/system/sandmux-helper.service

%post
if command -v apparmor_parser >/dev/null 2>&1 && [ -d /sys/kernel/security/apparmor ]; then
    apparmor_parser -r %{_sysconfdir}/apparmor.d/sandmux-run || :
fi
if [ -d /run/systemd/system ]; then
    systemctl daemon-reload || :
    systemctl enable --now sandmux-helper.socket || :
    systemctl try-restart sandmux-helper.service || :
fi

%preun
if [ "$1" -eq 0 ] && [ -d /run/systemd/system ]; then
    systemctl disable --now sandmux-helper.socket || :
    systemctl stop sandmux-helper.service || :
fi

%files
%license LICENSE
%doc README.md
%{_bindir}/sandmux
%{_bindir}/sandmuxd
%{_bindir}/sandmux-run
%{_bindir}/sandmux-chat
%{_bindir}/sandmux-db-mcp
%{_bindir}/sandmux-helper
/usr/lib/systemd/system/sandmux-helper.socket
/usr/lib/systemd/system/sandmux-helper.service
%config(noreplace) %{_sysconfdir}/apparmor.d/sandmux-run

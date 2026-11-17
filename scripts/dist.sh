#!/bin/sh
# Gera a distribuição em dist/: binários estáticos (musl), tarballs com
# SHA-256, .deb, .rpm (com rpmbuild), PKGBUILD do AUR e fórmula do Homebrew.
#
#   scripts/dist.sh             versão do Cargo.toml, compilando
#   SANDMUX_DIST_NO_BUILD=1     usa os binários já compilados
set -eu

cd "$(dirname "$0")/.."
TARGET=x86_64-unknown-linux-musl
BINS="sandmux sandmuxd sandmux-run sandmux-chat sandmux-db-mcp sandmux-helper"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
[ -n "$VERSION" ] || { echo "versão não encontrada no Cargo.toml" >&2; exit 1; }
NAME="sandmux-$VERSION-x86_64-linux"
OUT=dist

if [ -z "${SANDMUX_DIST_NO_BUILD:-}" ]; then
    echo "== cargo build --release --target $TARGET"
    # O C do ring compila com o gcc do sistema, sem o musl-gcc.
    CC_x86_64_unknown_linux_musl=${CC_x86_64_unknown_linux_musl:-gcc} \
        cargo build --release --locked --target "$TARGET" -p sandmux \
        --bin sandmux --bin sandmuxd --bin sandmux-run --bin sandmux-chat --bin sandmux-db-mcp \
        --bin sandmux-helper
fi

rm -rf "$OUT"
mkdir -p "$OUT/$NAME/bin" "$OUT/$NAME/apparmor" "$OUT/$NAME/systemd"
for b in $BINS; do
    cp "target/$TARGET/release/$b" "$OUT/$NAME/bin/$b"
    strip "$OUT/$NAME/bin/$b"
done
cp packaging/install.sh "$OUT/$NAME/install.sh"
cp packaging/apparmor/sandmux-run "$OUT/$NAME/apparmor/sandmux-run"
cp packaging/systemd/sandmux-helper.socket packaging/systemd/sandmux-helper.service.in "$OUT/$NAME/systemd/"
cp LICENSE-MIT LICENSE-APACHE README.md "$OUT/$NAME/"
chmod 0755 "$OUT/$NAME/install.sh"

echo "== tarballs"
tar -C "$OUT" --owner=0 --group=0 --sort=name -czf "$OUT/$NAME.tar.gz" "$NAME"
cp "$OUT/$NAME.tar.gz" "$OUT/sandmux-x86_64-linux.tar.gz"
for f in "$NAME.tar.gz" sandmux-x86_64-linux.tar.gz; do
    (cd "$OUT" && sha256sum "$f" >"$f.sha256")
done
cp packaging/install.sh "$OUT/install.sh"
SHA=$(cut -d' ' -f1 <"$OUT/$NAME.tar.gz.sha256")

echo "== .deb"
DEB="$OUT/deb/sandmux_${VERSION}_amd64"
mkdir -p "$DEB/DEBIAN" "$DEB/usr/bin" "$DEB/etc/apparmor.d" "$DEB/usr/share/doc/sandmux" "$DEB/usr/lib/systemd/system"
for b in $BINS; do
    install -m 0755 "$OUT/$NAME/bin/$b" "$DEB/usr/bin/$b"
done
sed 's|@SANDMUX_RUN@|/usr/bin/sandmux-run|' packaging/apparmor/sandmux-run >"$DEB/etc/apparmor.d/sandmux-run"
install -m 0644 packaging/systemd/sandmux-helper.socket "$DEB/usr/lib/systemd/system/"
sed 's|@SANDMUX_HELPER@|/usr/bin/sandmux-helper|' packaging/systemd/sandmux-helper.service.in >"$DEB/usr/lib/systemd/system/sandmux-helper.service"
chmod 0644 "$DEB/usr/lib/systemd/system/sandmux-helper.service"
cp LICENSE-MIT LICENSE-APACHE "$DEB/usr/share/doc/sandmux/"
sed "s/@VERSION@/$VERSION/" packaging/deb/control.in >"$DEB/DEBIAN/control"
echo "/etc/apparmor.d/sandmux-run" >"$DEB/DEBIAN/conffiles"
install -m 0755 packaging/deb/postinst "$DEB/DEBIAN/postinst"
install -m 0755 packaging/deb/postrm "$DEB/DEBIAN/postrm"
install -m 0755 packaging/deb/prerm "$DEB/DEBIAN/prerm"
dpkg-deb --root-owner-group --build "$DEB" "$OUT/sandmux_${VERSION}_amd64.deb" >/dev/null
rm -rf "$OUT/deb"
(cd "$OUT" && sha256sum "sandmux_${VERSION}_amd64.deb" >"sandmux_${VERSION}_amd64.deb.sha256")

if command -v rpmbuild >/dev/null 2>&1; then
    echo "== .rpm"
    R=$(mktemp -d)
    mkdir -p "$R/SOURCES"
    cp "$OUT/$NAME.tar.gz" "$R/SOURCES/"
    rpmbuild --quiet -bb --define "_topdir $R" --define "sandmux_version $VERSION" packaging/rpm/sandmux.spec
    cp "$R"/RPMS/x86_64/sandmux-"$VERSION"-1.x86_64.rpm "$OUT/"
    rm -rf "$R"
    (cd "$OUT" && sha256sum "sandmux-$VERSION-1.x86_64.rpm" >"sandmux-$VERSION-1.x86_64.rpm.sha256")
else
    echo "== .rpm pulado: falta rpmbuild"
fi

echo "== AUR e Homebrew"
mkdir -p "$OUT/aur" "$OUT/homebrew"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHA256@/$SHA/g" packaging/aur/PKGBUILD.in >"$OUT/aur/PKGBUILD"
cp packaging/aur/sandmux.install "$OUT/aur/"
cat >"$OUT/aur/.SRCINFO" <<EOF
pkgbase = sandmux-bin
	pkgdesc = Multiplexador de terminal para agentes de código em sandbox
	pkgver = $VERSION
	pkgrel = 1
	url = https://github.com/alanyoshida/sandmux
	install = sandmux.install
	arch = x86_64
	license = MIT
	license = Apache-2.0
	provides = sandmux
	conflicts = sandmux
	source = https://github.com/alanyoshida/sandmux/releases/download/v$VERSION/$NAME.tar.gz
	sha256sums = $SHA

pkgname = sandmux-bin
EOF
sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHA256@/$SHA/g" packaging/homebrew/sandmux.rb.in >"$OUT/homebrew/sandmux.rb"

rm -rf "${OUT:?}/$NAME"
echo "== pronto: $OUT"
ls -1 "$OUT"

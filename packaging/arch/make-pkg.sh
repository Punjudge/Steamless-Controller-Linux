#!/usr/bin/env bash
# Build an Arch Linux / CachyOS package (.pkg.tar.zst) from a release tarball,
# using the steamless-controller-bin PKGBUILD. No AUR account is needed: the
# package can be attached to a GitHub release and installed with pacman -U.
#
#   packaging/arch/make-pkg.sh <steamless-controller-X.Y.Z-x86_64-linux.tar.gz> <X.Y.Z> <output-dir>
#
# Needs makepkg (base-devel). Runs as a normal user, never as root.
set -euo pipefail

if [ $# -ne 3 ]; then
    echo "usage: $0 <release.tar.gz> <version> <output-dir>" >&2
    exit 2
fi
tarball=$(realpath "$1")
version=$2
mkdir -p "$3"
out=$(realpath "$3")
here=$(cd "$(dirname "$0")/.." && pwd)
recipe="$here/aur/steamless-controller-bin"

case "$version" in
    *[!0-9A-Za-z._+]*|"") echo "version '$version' is not a valid pkgver" >&2; exit 2 ;;
esac
expected="steamless-controller-$version-x86_64-linux.tar.gz"
if [ "$(basename "$tarball")" != "$expected" ]; then
    echo "expected a file named $expected" >&2
    exit 2
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$recipe/PKGBUILD" "$recipe/steamless-controller.install" \
   "$recipe/70-steamless-controller.rules" "$work/"
# makepkg uses a source file that is already beside the PKGBUILD instead of
# downloading it, so the tarball does not have to be on GitHub yet.
cp "$tarball" "$work/"
sum=$(sha256sum "$tarball" | cut -d' ' -f1)
sed -i -e "s/^pkgver=.*/pkgver=$version/" -e "s/^pkgrel=.*/pkgrel=1/" \
       -e "s/^sha256sums=('[0-9a-f]*'/sha256sums=('$sum'/" "$work/PKGBUILD"

# --nodeps: nothing is compiled, so the run-time libraries need not be here.
(cd "$work" && PKGDEST="$out" makepkg -f --nodeps)
ls "$out"/steamless-controller-bin-"$version"-*.pkg.tar.zst

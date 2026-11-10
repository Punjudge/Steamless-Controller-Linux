#!/usr/bin/env bash
# Sign the pacman packages in a folder and turn the folder into a small pacman
# repository named "steamless-controller", ready to attach to a GitHub release.
#
#   packaging/arch/sign-repo.sh <folder-with-packages>
#
# The signing key must already be in gpg (CI imports it from the
# PACMAN_SIGNING_KEY secret). Its public half is steamless-controller.asc next
# to this script; the fingerprint is read from that file, so a different key
# in gpg is refused instead of signing with it.
#
# Afterwards the folder holds, for each package, <pkg> and <pkg>.sig, plus
# steamless-controller.db / .files (and their .tar.gz and .sig files) as real
# files, because a release cannot hold symlinks.
set -euo pipefail

if [ $# -ne 1 ]; then
    echo "usage: $0 <folder-with-packages>" >&2
    exit 2
fi
dir=$(realpath "$1")
here=$(cd "$(dirname "$0")" && pwd)
pub="$here/steamless-controller.asc"
repo=steamless-controller

fpr=$(gpg --batch --with-colons --import-options show-only --import "$pub" 2>/dev/null \
      | awk -F: '/^fpr:/{print $10; exit}')
if [ -z "$fpr" ]; then
    echo "no key in $pub" >&2
    exit 1
fi
if ! gpg --batch --list-secret-keys "$fpr" >/dev/null 2>&1; then
    echo "the secret key for $fpr is not in gpg" >&2
    exit 1
fi

cd "$dir"
shopt -s nullglob
pkgs=(*.pkg.tar.zst)
if [ ${#pkgs[@]} -eq 0 ]; then
    echo "no packages in $dir" >&2
    exit 1
fi
for p in "${pkgs[@]}"; do
    gpg --batch --yes --pinentry-mode loopback --passphrase '' \
        --local-user "$fpr" --detach-sign --output "$p.sig" "$p"
done

# Only the versioned package goes into the repository; the fixed-name copy is
# for "pacman -U <address>".
versioned=()
for p in "${pkgs[@]}"; do
    case "$p" in
        *-bin-x86_64.pkg.tar.zst) ;;
        *) versioned+=("$p") ;;
    esac
done
rm -f "$repo".db* "$repo".files*
GPGKEY="$fpr" repo-add --sign --key "$fpr" "$repo.db.tar.gz" "${versioned[@]}"

# Replace repo-add's symlinks (steamless-controller.db -> .db.tar.gz, ...)
# with copies.
for l in "$repo".db "$repo".db.sig "$repo".files "$repo".files.sig; do
    if [ -L "$l" ]; then
        cp --remove-destination "$(readlink "$l")" "$l"
    fi
done
rm -f "$repo".db.tar.gz.old* "$repo".files.tar.gz.old*

# Check every signature before anything is uploaded.
for s in *.sig; do
    gpg --batch --verify "$s" "${s%.sig}" 2>/dev/null \
        || { echo "bad signature: $s" >&2; exit 1; }
done
echo "signed with $fpr:"
ls -l

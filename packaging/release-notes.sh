#!/usr/bin/env bash
# Print the GitHub release notes for one version: its section of CHANGELOG.md
# followed by short install lines. Fails when the changelog has no section for
# the version, so a release is never published without notes.
#
#   packaging/release-notes.sh <X.Y.Z> <CHANGELOG.md> [previous-tag]
set -euo pipefail

if [ $# -lt 2 ] || [ $# -gt 3 ]; then
    echo "usage: $0 <X.Y.Z> <CHANGELOG.md> [previous-tag]" >&2
    exit 2
fi
version=$1
changelog=$2
previous=${3:-}
repo=https://github.com/Punjudge/SteamlessController-Linux

section=$(awk -v v="## $version" '
    $0 == v { on = 1; next }
    on && /^## / { exit }
    on { print }
' "$changelog" | sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}')

if [ -z "$section" ]; then
    echo "CHANGELOG.md has no section '## $version'; write one before tagging" >&2
    exit 1
fi

printf '%s\n\n' "$section"
cat <<EOF
### Install

* **Arch Linux / CachyOS:** \`sudo pacman -Syu steamless-controller-bin\` once the
  \`[steamless-controller]\` repository is set up (see
  [Install]($repo/tree/main#install)).
* **Other distributions:** download \`steamless-controller-$version-x86_64-linux.tar.gz\`
  below, unpack it and run \`./install.sh\`.
EOF
if [ -n "$previous" ]; then
    printf '\n**All changes:** %s/compare/%s...v%s\n' "$repo" "$previous" "$version"
fi

#!/bin/sh
# Installs Steamless Controller for the current user (no root needed):
#   ~/.local/bin/steamless-controller
#   ~/.local/share/applications/steamless-controller.desktop
# Run it from the unpacked release folder. To remove: ./install.sh --uninstall
set -eu
here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin"
apps="${XDG_DATA_HOME:-$HOME/.local/share}/applications"

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$bin/steamless-controller" "$apps/steamless-controller.desktop"
    echo "Removed. Your settings stay in ~/.config/steamless-controller."
    exit 0
fi

mkdir -p "$bin" "$apps"
install -m 755 "$here/steamless-controller" "$bin/steamless-controller"
sed "s|^Exec=.*|Exec=$bin/steamless-controller|" "$here/steamless-controller.desktop" > "$apps/steamless-controller.desktop"
echo "Installed to $bin/steamless-controller."
echo "Start it from the application menu (Steamless Controller)."

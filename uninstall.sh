#!/bin/sh
# waddle — uninstaller
#
# Removes the waddle binary installed by install.sh. waddle stores nothing else
# on disk (no config, no history, no cache), so this is the entire cleanup.
#
#     curl --proto '=https' --tlsv1.2 -LsSf https://raw.githubusercontent.com/excelano/waddle/main/uninstall.sh | sh

set -eu

if [ -n "${CARGO_HOME:-}" ]; then
    install_dir="$CARGO_HOME/bin"
else
    install_dir="$HOME/.cargo/bin"
fi

target="$install_dir/waddle"

if [ -e "$target" ]; then
    rm -f "$target"
    echo "Removed $target"
elif command -v waddle >/dev/null 2>&1; then
    found="$(command -v waddle)"
    echo "waddle is installed at $found, not the expected location ($target)."
    echo "Remove it manually if you want it gone."
    exit 1
else
    echo "waddle is not installed."
fi

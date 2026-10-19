#!/bin/sh
# Install b9x and b9x-gui with the desktop entry, icon and AppStream metadata.
#
#   ./install.sh [--prefix DIR] [--uninstall]
#
# The prefix defaults to ~/.local (/usr/local as root); DESTDIR is honoured for packaging.
# Binaries come from bin/ in a release tarball, or from target/release/ in a source
# checkout after `cargo build --release`.
set -eu

APP_ID=io.github.joonhoekim.FlydigiB9X
here=$(cd "$(dirname "$0")" && pwd)
if [ "$(id -u)" = 0 ]; then prefix=/usr/local; else prefix=$HOME/.local; fi
uninstall=0

while [ $# -gt 0 ]; do
    case $1 in
        --prefix) prefix=$2; shift 2 ;;
        --prefix=*) prefix=${1#--prefix=}; shift ;;
        --uninstall) uninstall=1; shift ;;
        -h|--help) sed -n '2,9s/^# \{0,1\}//p' "$0"; exit 0 ;;
        *) echo "install.sh: unknown argument $1" >&2; exit 2 ;;
    esac
done

root=${DESTDIR:-}$prefix
bin=$root/bin
apps=$root/share/applications
icons=$root/share/icons/hicolor
metainfo=$root/share/metainfo

refresh() {
    # Without DESTDIR the files are live: let the desktop pick them up.
    [ -n "${DESTDIR:-}" ] && return
    command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t -f "$icons" 2>/dev/null || true
    command -v update-desktop-database >/dev/null && update-desktop-database -q "$apps" 2>/dev/null || true
}

if [ $uninstall = 1 ]; then
    rm -f "$bin/b9x" "$bin/b9x-gui" "$apps/$APP_ID.desktop" \
        "$icons/scalable/apps/$APP_ID.svg" "$metainfo/$APP_ID.metainfo.xml"
    refresh
    echo "removed from $root"
    exit 0
fi

if [ -x "$here/bin/b9x" ]; then
    built=$here/bin
elif [ -x "$here/target/release/b9x" ]; then
    built=$here/target/release
else
    echo "install.sh: no binaries; run 'cargo build --release' first" >&2
    exit 1
fi

install -Dm755 "$built/b9x" "$bin/b9x"
install -Dm755 "$built/b9x-gui" "$bin/b9x-gui"
# Absolute Exec so the launcher works even when $prefix/bin is not on PATH.
mkdir -p "$apps"
sed "s|^Exec=b9x-gui|Exec=$prefix/bin/b9x-gui|" "$here/data/$APP_ID.desktop" > "$apps/$APP_ID.desktop"
chmod 644 "$apps/$APP_ID.desktop"
install -Dm644 "$here/data/icons/hicolor/scalable/apps/$APP_ID.svg" "$icons/scalable/apps/$APP_ID.svg"
install -Dm644 "$here/data/$APP_ID.metainfo.xml" "$metainfo/$APP_ID.metainfo.xml"
refresh
echo "installed into $root"
case :$PATH: in *:"$prefix/bin":*) ;; *) echo "note: $prefix/bin is not on PATH" ;; esac

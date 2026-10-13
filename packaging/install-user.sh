#!/usr/bin/env bash
#
# Install XCOM for the current user, in the locations Debian expects.
#
#   ./packaging/install-user.sh
#
# Everything lands under ~/.local, so no root is needed.  Uninstall with:
#   ./packaging/install-user.sh --uninstall
#
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${HOME}/.local/bin"
app_dir="${HOME}/.local/share/applications"
icon_root="${HOME}/.local/share/icons/hicolor"
desktop_file="${app_dir}/xcom.desktop"

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -f "${bin_dir}/xcom" "${desktop_file}"
    find "${icon_root}" -name 'xcom.png' -delete 2>/dev/null || true
    command -v update-desktop-database >/dev/null && update-desktop-database "${app_dir}" || true
    echo "Removed XCOM from ${HOME}/.local"
    exit 0
fi

binary="${repo}/target/release/xcom"
if [[ ! -x "${binary}" ]]; then
    echo "error: ${binary} not found. Run 'cargo build --release' first." >&2
    exit 1
fi

# ~/.local/bin is added to PATH by Debian's default ~/.profile, so Exec=xcom and
# TryExec=xcom resolve without hard-coding a path in the desktop entry.
install -Dm755 "${binary}" "${bin_dir}/xcom"

for source in "${repo}"/packaging/icons/xcom-*.png; do
    size="$(basename "${source}" .png)"
    size="${size#xcom-}"
    install -Dm644 "${source}" "${icon_root}/${size}x${size}/apps/xcom.png"
done

install -Dm644 "${repo}/packaging/xcom.desktop" "${desktop_file}"

command -v update-desktop-database >/dev/null && update-desktop-database "${app_dir}" || true

# A user-level hicolor tree normally has no index.theme, so there is no cache to
# build; GTK scans the directory instead.  Only refresh the cache when the file
# is actually there, otherwise gtk-update-icon-cache just prints an error.
if [[ -f "${icon_root}/index.theme" ]] && command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -f "${icon_root}" >/dev/null || true
fi

echo "Installed:"
echo "  ${bin_dir}/xcom"
echo "  ${desktop_file}"
echo "  ${icon_root}/<size>x<size>/apps/xcom.png"

case ":${PATH}:" in
    *":${bin_dir}:"*) ;;
    *)
        echo
        echo "Note: ${bin_dir} is not on PATH in this shell."
        echo "      Debian adds it on login; until then run 'xcom' by full path."
        ;;
esac

# TryExec is resolved on PATH.  When it fails, GNOME ignores the whole entry --
# no menu item and no icon -- which is confusing because the icon files are all
# present.  Catch it here rather than leaving the user to guess.
if grep -qx 'TryExec=xcom' "${desktop_file}" && ! command -v xcom >/dev/null; then
    echo
    echo "Warning: the entry sets TryExec=xcom but 'xcom' is not on PATH yet,"
    echo "         so GNOME will ignore it entirely. Log out and back in so that"
    echo "         ${bin_dir} takes effect, then run without 'sudo':"
    echo "             update-desktop-database ${app_dir}"
fi

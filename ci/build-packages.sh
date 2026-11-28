#!/usr/bin/env bash
# Builds the packages under pkgbuilds/. Runs INSIDE a throwaway archlinux
# container as root; used by ci/build-repo.sh (signed release builds) and
# ci/build-pkgs.sh (unsigned test builds) so both build identically.
#
# Usage (in the container): /src/ci/build-packages.sh <out-dir>
# Mounts: /src (repository, read-only), /srcdest (download cache, optional)
# Env:
#   SAMURAI_PKGREL_OVERRIDE  monotonic pkgrel for samurai-tools/samurai-release
#   SAMURAI_PUBLISHED_URL    set: reuse already published files for unchanged versions
#   SAMURAI_ONLY             space-separated package names (default: all)
set -euo pipefail

out="${1:?usage: build-packages.sh <out-dir>}"
id builder >/dev/null 2>&1 || useradd -m builder
mkdir -p /srcdest /build "$out"
chown builder /srcdest /build

selected() {
    [ -z "${SAMURAI_ONLY:-}" ] && return 0
    case " $SAMURAI_ONLY " in *" $1 "*) return 0 ;; esac
    return 1
}

cd /build
for src in /src/pkgbuilds/*/; do
    pkg="$(basename "$src")"
    selected "$pkg" || continue
    cp -a "$src" "$pkg"
    cp -a /src/LICENSE "$pkg/LICENSE"
    if [ "$pkg" = samurai-tools ]; then
        pacman -S --noconfirm --needed rust >/dev/null
        # the Rust CLI replaces the shell dispatcher (updates/samurai stays for the live ISO)
        mkdir -p "$pkg/toolbox"
        tar -C /src/toolbox --exclude=target --exclude=./gui -cf - . | tar -C "$pkg/toolbox" -xf -
        for name in samurai-update samurai-snap samurai-rescue samurai-apps samurai-notifier validate-interventions.py \
                    samurai-notifier.service samurai-notifier.timer \
                    samurai-health.service samurai-health.timer \
                    95-samurai-boot.hook; do
            cp -a "/src/updates/$name" "$pkg/"
        done
        cp -a /src/updates/interventions/0001-update-mechanism-demo.toml "$pkg/"
        cp -a /src/updates/interventions/0001-update-mechanism-demo.sh "$pkg/"
        cp -a /src/catalog/garuda-setup-wizard.toml /src/catalog/samurai.toml "$pkg/"
    fi
    if [ "$pkg" = samurai-chwd ]; then
        pacman -S --noconfirm --needed rust clang git lua pciutils libusb >/dev/null
    fi
    if [ "$pkg" = samurai-toolbox-gui ]; then
        pacman -S --noconfirm --needed cmake ninja qt6-base qt6-declarative >/dev/null
        cp -a /src/toolbox/gui "$pkg/gui"
    fi
    if [ -n "${SAMURAI_PKGREL_OVERRIDE:-}" ] && \
       { [ "$pkg" = samurai-tools ] || [ "$pkg" = samurai-release ]; }; then
        sed -i "s/^pkgrel=.*/pkgrel=$SAMURAI_PKGREL_OVERRIDE/" "$pkg/PKGBUILD"
    fi
    if [ "$pkg" = samurai-miniforge ]; then
        # conda embeds its prefix: build into the real path (see the PKGBUILD)
        install -d -o builder /opt/miniforge
    fi
    chown -R builder "$pkg"
    echo "==> makepkg $pkg"
    # --nodeps: runtime depends on our own packages (samurai-release ->
    # samurai-tools) cannot be installed from a repo this very run creates;
    # build dependencies of the compiled packages are installed above
    su builder -c "cd /build/$pkg && SRCDEST=/srcdest makepkg -df --noconfirm"
    set -- /build/"$pkg"/*.pkg.tar.zst   # .sig files are created by the signing step
    # spec 02 step 4: namcap lint (advisory; its exit code is not a verdict)
    if command -v namcap >/dev/null; then
        su builder -c "cd /build/$pkg && namcap PKGBUILD $* 2>&1 | head -n 60 | sed 's/^/    namcap: /'" || true
    fi
    for built in "$@"; do
        # never change the bytes behind an already published file name (stable channel)
        if [ -n "${SAMURAI_PUBLISHED_URL:-}" ]; then
            /src/ci/reuse-published "$built" "$SAMURAI_PUBLISHED_URL"
        fi
        cp -v "$built" "$out/"
        if [ -f "$built.sig" ]; then cp -v "$built.sig" "$out/"; fi
    done
    # large repackaged trees: free the space before the next package
    rm -rf "/build/$pkg" /opt/miniforge
done

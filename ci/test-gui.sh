#!/usr/bin/env bash
# Build the toolbox GUI in an Arch container, lint the QML, and open every page offscreen against a
# fake CLI (tests/gui/fake-samurai). Any QML warning fails the run; screenshots land in the output
# directory. The real thing is tested in a Plasma VM: tests/vm/test_toolbox_gui.py.
#
# Usage: ci/test-gui.sh [out-dir]   (default: .tmp/gui-test)
# Env: SAMURAI_GUI_IMAGE  container with qt6, kirigami, breeze, cmake, python (default archlinux:base-devel,
#      installing them on each run)
set -euo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$(realpath -m "${1:-$repo/.tmp/gui-test}")"
image="${SAMURAI_GUI_IMAGE:-archlinux:base-devel}"
mkdir -p "$out"

docker run --rm -i -v "$repo":/src:ro -v "$out":/out -e OWNER="$(id -u):$(id -g)" "$image" bash -es <<'EOS'
set -euo pipefail
trap 'chown -R "$OWNER" /out' EXIT
need=(cmake ninja qt6-base qt6-declarative qt6-svg kirigami qqc2-desktop-style breeze breeze-icons
      noto-fonts ttf-hack python)
if ! pacman -Qq "${need[@]}" >/dev/null 2>&1; then
    pacman -Syu --noconfirm --needed "${need[@]}" >/dev/null
fi
cmake -S /src/toolbox/gui -B /tmp/build -G Ninja -DSAMURAI_GUI_DEVTOOLS=ON \
    -DCMAKE_BUILD_TYPE=Release -DCMAKE_CXX_FLAGS="-Wall -Wextra -Werror" >/tmp/cmake.log \
    || { cat /tmp/cmake.log; exit 1; }
cmake --build /tmp/build >/tmp/build.log || { cat /tmp/build.log; exit 1; }
echo "==> built"
# the real helper (real argument checks) in front of the fake CLI
g++ -std=c++20 -O1 -DSAMURAI_CLI='"/src/tests/gui/fake-samurai"' -DSAMURAI_REQUIRE_ROOT=0 \
    /src/toolbox/gui/common/operations.cpp /src/toolbox/gui/helper/main.cpp -o /tmp/helper
printf '#!/bin/sh\n# pkexec --disable-internal-agent /usr/lib/samurai/gui-helper verb args\nshift 2\nexec /tmp/helper "$@"\n' >/tmp/pkexec
chmod +x /tmp/pkexec
useradd -m tester
mkdir -p /tmp/shots && chown tester /tmp/shots
failed=0
for page in overview health apps cleanup kernels updates snapshots diagnostics; do
    rc=0
    su tester -c "cd /tmp && QT_QPA_PLATFORM=offscreen QT_QUICK_BACKEND=software \
        QT_QPA_PLATFORMTHEME=kde XDG_CURRENT_DESKTOP=KDE LANG=C.UTF-8 QT_FORCE_STDERR_LOGGING=1 \
        timeout 60 /tmp/build/samurai-toolbox --page $page --cli /src/tests/gui/fake-samurai \
        --pkexec /tmp/pkexec --grab /tmp/shots" >"/tmp/$page.log" 2>&1 || rc=$?
    cp "/tmp/$page.log" /out/
    # Kirigami's PageRow creates every pushed page under a non-visual parent and warns once
    grep -v 'Created graphical object was not placed in the graphics scene' "/tmp/$page.log" \
        >"/tmp/$page.filtered" || true
    if [ "$rc" != 0 ] || grep -E 'qrc:|\.qml:[0-9]+|TypeError|ReferenceError' "/tmp/$page.filtered"; then
        echo "FAIL $page (exit $rc)"; cat "/tmp/$page.log"; failed=1
    else
        echo "ok   $page"
    fi
done
cp /tmp/shots/*.png /out/ || true
lint=/usr/lib/qt6/bin/qmllint
"$lint" -I /tmp/build /tmp/build/org/samurai/toolbox/*.qml \
    >/out/qmllint.log 2>&1 || true
echo "==> qmllint: $(grep -c '^Warning' /out/qmllint.log || true) warning(s) (advisory, see qmllint.log)"
exit "$failed"
EOS
echo "==> screenshots in $out"

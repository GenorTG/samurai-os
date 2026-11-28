#!/usr/bin/env python3
"""Expected result of `samurai hw detect --json` for every fixture machine (ci/test-hw.sh)."""
import json
import pathlib
import sys

# machine -> (profiles in plan order, refusal expected, packages that must be planned,
#             packages that must not be planned)
EXPECTED = {
    "genorbox1": (["nvidia-dkms-580xx"], False,
                  ["nvidia-580xx-dkms", "linux-cachyos-headers", "linux-zen-headers"],
                  ["nvidia-open-dkms", "nvidia-utils"]),
    "desktop-rtx3090": (["nvidia-open-dkms"], False, ["nvidia-open-dkms", "linux-zen-headers"],
                        ["linux-cachyos-nvidia-open"]),
    "desktop-rtx3090-cachyos": (["nvidia-open-dkms"], False, ["linux-cachyos-nvidia-open"],
                                ["nvidia-open-dkms", "dkms"]),
    "desktop-rtx5080": (["nvidia-open-dkms"], False, ["linux-cachyos-nvidia-open"],
                        ["nvidia-580xx-utils"]),
    "desktop-gtx1070": (["nvidia-dkms-580xx"], False, ["linux-cachyos-nvidia-580xx"],
                        ["nvidia-utils"]),
    "desktop-gtx750": (["nvidia-dkms-580xx"], False, ["nvidia-580xx-dkms", "linux-headers"], []),
    "desktop-gtx680": (["nouveau.kepler"], False, ["vulkan-nouveau", "linux-firmware-nvidia"],
                       ["nvidia-utils", "nvidia-580xx-utils"]),
    "desktop-amd-rx7900": (["amd"], False, ["vulkan-radeon", "opencl-mesa"], ["nvidia-utils"]),
    "laptop-intel-rtx4060": (["nvidia-open-dkms.prime", "intel"], False,
                             ["nvidia-prime", "switcheroo-control", "intel-media-driver"],
                             ["opencl-mesa"]),
    "laptop-intel": (["intel"], False, ["vulkan-intel", "intel-media-driver", "opencl-mesa"],
                     ["nvidia-utils"]),
    "laptop-broadcom": (["intel", "broadcom-wl"], False, ["broadcom-wl-dkms"], []),
    "qemu": (["qemu"], False, ["spice-vdagent", "vulkan-virtio"], ["nvidia-utils"]),
}


def main(out: pathlib.Path) -> int:
    failures = []
    found = sorted(p.name for p in out.iterdir())
    if found != sorted(EXPECTED):
        failures.append(f"machines differ: {found} != {sorted(EXPECTED)}")
    for name, (profiles, refused, wanted, unwanted) in EXPECTED.items():
        path = out / name / "detect.json"
        if not path.is_file():
            failures.append(f"{name}: no detect.json")
            continue
        plan = json.loads(path.read_text(encoding="utf-8"))
        got = [s["profile"] for s in plan["steps"]]
        planned = {p for s in plan["steps"] for p in s["install"]}
        problems = []
        if got != profiles:
            problems.append(f"profiles {got}, expected {profiles}")
        if bool(plan["refusals"]) != refused:
            problems.append(f"refusals {plan['refusals']}")
        problems += [f"missing package {p}" for p in wanted if p not in planned]
        problems += [f"unexpected package {p}" for p in unwanted if p in planned]
        failures += [f"{name}: {p}" for p in problems]
        print(f"{'FAIL' if problems else 'ok  '} {name:<24} {' + '.join(got)}"
              + (f"  [{'; '.join(plan['warnings'])}]" if plan["warnings"] else ""))
    for failure in failures:
        print(f"error: {failure}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main(pathlib.Path(sys.argv[1])))

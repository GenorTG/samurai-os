#!/usr/bin/env python3
"""Turn a tests/hw-fixtures/*.hw file into the trees a container mounts so that the REAL chwd,
lspci and `samurai hw` see that machine:

  <out>/pci/devices/<addr>/{vendor,device,class,config,...}   -> /sys/bus/pci
  <out>/dmi/{chassis_type,product_name,board_name}            -> /sys/devices/virtual/dmi/id
  <out>/{kernels,virt}                                         read by the in-container driver
"""
import pathlib
import sys


def parse(path: pathlib.Path) -> dict:
    machine = {"devices": []}
    for line in path.read_text(encoding="utf-8").splitlines():
        words = line.split()
        if not words or words[0].startswith("#"):
            continue
        if words[0] == "dev":
            addr, cls, vendor, device = words[1:5]
            machine["devices"].append((addr, cls, vendor, device, "boot" in words[5:]))
        else:
            machine[words[0]] = words[1:]
    return machine


def config_space(cls: str, vendor: str, device: str) -> bytes:
    space = bytearray(256)
    space[0:2] = int(vendor, 16).to_bytes(2, "little")
    space[2:4] = int(device, 16).to_bytes(2, "little")
    space[0x0A] = int(cls[2:], 16)
    space[0x0B] = int(cls[:2], 16)
    return bytes(space)


def build(fixture: pathlib.Path, out: pathlib.Path) -> None:
    machine = parse(fixture)
    for addr, cls, vendor, device, boot in machine["devices"]:
        base = out / "pci" / "devices" / addr
        base.mkdir(parents=True)
        values = {
            "vendor": f"0x{vendor}", "device": f"0x{device}", "class": f"0x{cls}00",
            "subsystem_vendor": "0x0000", "subsystem_device": "0x0000", "revision": "0x00",
            "irq": "0", "resource": "\n".join(["0x0 0x0 0x0"] * 7),
            "uevent": f"PCI_SLOT_NAME={addr}",
        }
        if cls.startswith("03"):
            values["boot_vga"] = "1" if boot else "0"
        for name, value in values.items():
            (base / name).write_text(value + "\n", encoding="ascii")
        (base / "config").write_bytes(config_space(cls, vendor, device))
    dmi = out / "dmi"
    dmi.mkdir(parents=True)
    (dmi / "chassis_type").write_text(machine["chassis"][0] + "\n", encoding="ascii")
    (dmi / "product_name").write_text("Fixture\n", encoding="ascii")
    (dmi / "board_name").write_text("Fixture\n", encoding="ascii")
    (out / "kernels").write_text(" ".join(machine["kernels"]) + "\n", encoding="ascii")
    (out / "virt").write_text(machine["virt"][0] + "\n", encoding="ascii")


if __name__ == "__main__":
    build(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]))

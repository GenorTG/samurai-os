"""samurai-settings-security (spec 09): file contents that must not regress."""
from __future__ import annotations

import json
import re
import shutil
import subprocess
import xml.etree.ElementTree as ET

import pytest

from conftest import REPO

PKG = REPO / "pkgbuilds" / "samurai-settings-security"


def sysctls() -> dict[str, str]:
    lines = (PKG / "80-samurai-security.conf").read_text().splitlines()
    pairs = [line.split("=", 1) for line in lines if line.strip() and not line.startswith("#")]
    return {key.strip(): value.strip() for key, value in pairs}


def test_spec_sysctls_are_present():
    conf = sysctls()
    spec = (REPO / "docs" / "specs" / "09-SECURITY.md").read_text()
    block = spec.split("## Kernel and sysctl hardening")[1].split("```")[1]
    for key, value in re.findall(r"([a-z][\w.]+) = (\d)", block):
        assert conf[key] == value, key


def test_redirects_cover_every_interface():
    """conf/all alone is ORed with the interface value: it changes nothing."""
    conf = sysctls()
    for family, name in (("ipv4", "accept_redirects"), ("ipv4", "send_redirects"), ("ipv6", "accept_redirects")):
        for scope in ("all", "default", "*"):
            assert conf[f"net.{family}.conf.{scope}.{name}"] == "0"


def test_user_namespaces_stay_on():
    assert not [key for key in sysctls() if "userns" in key or "user.max_user_namespaces" in key]


def test_sudoers_only_sets_defaults_and_parses(tmp_path):
    lines = [line for line in (PKG / "50-samurai-security.sudoers").read_text().splitlines()
             if line.strip() and not line.startswith("#")]
    assert lines and all(line.startswith("Defaults ") for line in lines)
    assert "NOPASSWD" not in "".join(lines)
    if not shutil.which("visudo"):
        pytest.skip("visudo not available (asserted in the VM test)")
    subprocess.run(["visudo", "-c", "-f", str(PKG / "50-samurai-security.sudoers")], check=True)


def test_firefox_policy():
    policy = json.loads((PKG / "policies.json").read_text())["policies"]
    assert policy["DisableTelemetry"] is True
    ublock = policy["ExtensionSettings"]["uBlock0@raymondhill.net"]
    assert ublock["installation_mode"] == "normal_installed"
    assert ublock["install_url"].startswith("https://addons.mozilla.org/")


def ports(path):
    return {(p.get("port"), p.get("protocol")) for p in ET.parse(path).getroot().iter("port")}


def test_firewall_files():
    assert ("47990", "tcp") not in ports(PKG / "samurai-sunshine.xml"), "the Sunshine admin page stays closed"
    zone = ET.parse(PKG / "samurai-tailnet.xml").getroot()
    assert zone.get("target") is None, "default target refuses inbound"
    assert [i.get("name") for i in zone.iter("interface")] == ["tailscale0"]
    assert not list(zone.iter("service")) and not list(zone.iter("port"))


def test_installed_by_default_and_apparmor_activated():
    planner = (REPO / "installer" / "samurai-profile-plan").read_text()
    assert '"samurai-settings-security"' in planner and '"apparmor.service"' in planner
    postinstall = (REPO / "installer" / "samurai-postinstall").read_text()
    assert "lsm=landlock,lockdown,yama,integrity,apparmor,bpf" in postinstall

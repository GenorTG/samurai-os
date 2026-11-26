"""The wizard's only door to the install engine: the validator, the planner, the app
catalog and the password hash. All fixed argv; secrets travel on stdin only."""
from __future__ import annotations

import json
import os
import pathlib
import subprocess
import tempfile
from dataclasses import dataclass


@dataclass(frozen=True)
class Tools:
    """Where the engine's programs live (the live ISO by default; tests override)."""
    validator: tuple[str, ...] = ("/usr/local/bin/validate-profile.py",)
    planner: tuple[str, ...] = ("/usr/local/bin/samurai-profile-plan",)
    apps: tuple[str, ...] = ("/usr/local/bin/samurai-apps",)
    openssl: tuple[str, ...] = ("openssl",)

    @staticmethod
    def from_checkout(repo: pathlib.Path) -> "Tools":
        py = "python3"
        return Tools(validator=(py, str(repo / "installer" / "validate-profile.py")),
                     planner=(py, str(repo / "installer" / "samurai-profile-plan")),
                     apps=(py, str(repo / "updates" / "samurai-apps")))


class EngineError(RuntimeError):
    pass


def _run(argv: list[str], stdin: str | None = None) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(argv, input=stdin, capture_output=True, text=True, timeout=60,
                              check=False)
    except (OSError, subprocess.SubprocessError) as e:
        raise EngineError(f"cannot run {argv[0]}: {e}") from e


def hash_password(tools: Tools, password: str) -> str:
    done = _run([*tools.openssl, "passwd", "-6", "-stdin"], stdin=password + "\n")
    value = done.stdout.strip()
    if done.returncode != 0 or not value.startswith("$6$"):
        raise EngineError("could not hash the password")
    return value


def _with_profile(text: str, argv: list[str]) -> subprocess.CompletedProcess[str]:
    """Run ``argv <file>`` on a private (0600) temporary copy of the profile."""
    runtime = os.environ.get("XDG_RUNTIME_DIR") or None
    with tempfile.TemporaryDirectory(prefix="samurai-installer-", dir=runtime) as directory:
        path = pathlib.Path(directory) / "install-profile.toml"
        write_private(path, text)
        return _run([*argv, str(path)])


def validate(tools: Tools, text: str) -> str:
    """"" when the validator accepts the profile, else its reason."""
    done = _with_profile(text, list(tools.validator))
    if done.returncode == 0:
        return ""
    reason = done.stderr.strip().splitlines()
    return reason[-1].removeprefix("VALIDATION ERROR: ") if reason else "validation failed"


def plan(tools: Tools, text: str, offline: bool, nvidia: bool, amd: bool) -> dict:
    argv = [*tools.planner, "--json"]
    argv += ["--offline"] if offline else []
    argv += ["--nvidia"] if nvidia else []
    argv += ["--amd"] if amd else []
    done = _with_profile(text, argv)
    try:
        if done.returncode != 0:
            raise ValueError(done.stderr.strip())
        return json.loads(done.stdout)
    except ValueError as e:
        raise EngineError(f"the planner failed: {e}") from e


def catalog(tools: Tools) -> list[dict]:
    """Selectable catalog apps: [{id, name, category, category_name, packages}]."""
    # list --json needs pacman for install states; on the live medium it is there
    done = _run([*tools.apps, "list", "--json"])
    cats = _run([*tools.apps, "categories", "--json"])
    try:
        apps = json.loads(done.stdout)["apps"]
        names = {c["id"]: c["name"] for c in json.loads(cats.stdout)["categories"]}
    except (ValueError, KeyError, TypeError) as e:
        raise EngineError(f"the app catalog is unusable: {done.stderr.strip() or e}") from e
    return [{"id": a["id"], "name": a["name"], "category": a["category"],
             "category_name": names.get(a["category"], a["category"]),
             "packages": ", ".join(p["name"] for p in a.get("packages", []))} for a in apps]


def write_private(path: pathlib.Path, text: str) -> None:
    """Atomically write a 0600 file (profiles may carry a password hash)."""
    fd, tmp = tempfile.mkstemp(prefix=".profile-", dir=path.parent)   # created 0600
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(text)
        os.replace(tmp, path)
    except BaseException:
        pathlib.Path(tmp).unlink(missing_ok=True)
        raise

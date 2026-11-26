"""Install progress from the engine's own log lines.

Each stage is recognised by a message samurai-install or samurai-postinstall prints
(tests/unit/test_gui_progress.py fails when a marker disappears from those scripts)."""
from __future__ import annotations

from dataclasses import dataclass


@dataclass(frozen=True)
class Stage:
    marker: str       # substring of a log line
    label: str
    percent: int      # progress once this stage has started


STAGES: tuple[Stage, ...] = (
    Stage("samurai-install starting", "Checking the profile and the disk", 1),
    Stage("partitioning ", "Partitioning the disk", 3),
    Stage("creating btrfs", "Creating the encrypted file system", 6),
    Stage("installing base system", "Installing the base system", 9),
    Stage("fstab written", "Writing the file system table", 38),
    Stage("running samurai-postinstall", "Configuring the new system", 40),
    Stage("identity: ", "Setting language, time zone and name", 42),
    Stage("repos: enabling [samurai]", "Setting up package repositories", 45),
    Stage("packages: ", "Installing the desktop and your software", 50),
    Stage("desktop: ", "Setting up the desktop", 80),
    Stage("mkinitcpio for ", "Building the boot images", 84),
    Stage("kernel cmdline: ", "Installing the bootloader", 90),
    Stage("snapshots: snapper root config", "Setting up snapshots", 94),
    Stage("samurai-postinstall complete", "Finishing the configuration", 96),
    Stage("snapshots: creating install-complete", "Taking the first snapshot", 97),
    Stage("install complete; unmounting", "Unmounting the new system", 99),
)
FAILURE_MARKERS = ("FATAL: ",)


@dataclass
class Progress:
    percent: int = 0
    label: str = "Starting"
    error: str = ""
    _index: int = -1

    def feed(self, line: str) -> bool:
        """Update from one log line; True when something visible changed."""
        for marker in FAILURE_MARKERS:
            if marker in line and not self.error:
                self.error = line.split(marker, 1)[1].strip()
                return True
        for index in range(self._index + 1, len(STAGES)):
            stage = STAGES[index]
            if stage.marker in line:
                self._index, self.percent, self.label = index, stage.percent, stage.label
                return True
        return False

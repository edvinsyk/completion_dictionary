from __future__ import annotations

import os
import sys
from pathlib import Path


def data_root() -> Path:
    xdg_data_home = os.environ.get("XDG_DATA_HOME")
    if xdg_data_home:
        return Path(xdg_data_home) / "completion-dictionary"
    if sys.platform == "darwin":
        return Path.home() / "Library" / "Application Support" / "completion-dictionary"
    if os.name == "nt":
        local_app_data = os.environ.get("LOCALAPPDATA")
        if local_app_data:
            return Path(local_app_data) / "completion-dictionary"
        return Path.home() / "AppData" / "Local" / "completion-dictionary"
    return Path.home() / ".local" / "share" / "completion-dictionary"

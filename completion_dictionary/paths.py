from __future__ import annotations

import os
import sys
from collections.abc import Iterator
from contextlib import contextmanager
from pathlib import Path
from tempfile import NamedTemporaryFile


@contextmanager
def atomic_output(path: Path) -> Iterator[Path]:
    """Replace a destination only after its temporary sibling is fully written."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with NamedTemporaryFile(
        dir=path.parent, prefix=f".{path.name}.", suffix=".tmp", delete=False
    ) as temporary:
        temporary_path = Path(temporary.name)
    try:
        yield temporary_path
        # Windows chmod can make a failed replacement's temporary file undeletable.
        if os.name != "nt" and path.exists():
            temporary_path.chmod(path.stat().st_mode & 0o777)
        temporary_path.replace(path)
    finally:
        temporary_path.unlink(missing_ok=True)


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

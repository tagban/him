"""What SmarterChild remembers, as small JSON files in its data folder."""

from __future__ import annotations

import json
import os
import threading
from pathlib import Path


class Store:
    """A JSON document on disk, written atomically after each change."""

    def __init__(self, path: Path, default):
        self.path = path
        self.lock = threading.Lock()
        try:
            self.data = json.loads(path.read_text())
        except (OSError, ValueError):
            self.data = default

    def save(self) -> None:
        with self.lock:
            self.path.parent.mkdir(parents=True, exist_ok=True)
            tmp = self.path.with_suffix(".tmp")
            tmp.write_text(json.dumps(self.data, indent=1, sort_keys=True))
            os.replace(tmp, self.path)


class Memory:
    """Per-person facts: {login: {"name", "place", "units", "facts": {...}, "seen"}}."""

    def __init__(self, folder: Path):
        self.store = Store(folder / "memory.json", {})

    def of(self, login: str) -> dict:
        key = login.lower()
        m = self.store.data.setdefault(key, {})
        m.setdefault("facts", {})
        return m

    def set(self, login: str, key: str, value) -> None:
        self.of(login)[key] = value
        self.store.save()

    def forget(self, login: str) -> None:
        self.store.data.pop(login.lower(), None)
        self.store.save()

"""What SmarterChild didn't understand, kept so its answers can grow where people need them.

Two kinds:
- "fallback": nothing matched, so it shrugged ("I'm not sure what you mean...");
- "lookup": only the catch-alls caught it (Wikipedia, the dictionary), which is right for
  "who is Ada Lovelace" but wrong for "what are you wearing".

Kept in data/misses.json by the message's words (lowercased), with how often and when,
never who said it. `smarterchild-misses` lists them, most asked first:

  smarterchild-misses               the top 40 of both kinds
  smarterchild-misses 100 fallback  the top 100 shrugs
  smarterchild-misses --clear       start over (after the answers are written)
"""

from __future__ import annotations

import os
import re
import sys
import time
from pathlib import Path

from .store import Store

KEEP = 5000      # phrases kept; the rarest, oldest go first
LONGEST = 200    # characters kept of a message

# Things that look personal are left out: email addresses, phone numbers, long digit runs.
_PRIVATE = re.compile(r"\S+@\S+\.\w+|\+?\d[\d\s().-]{7,}\d")


class Misses:
    def __init__(self, folder: Path):
        self.store = Store(folder / "misses.json", {})

    def record(self, text: str, kind: str, room: bool = False) -> None:
        t = " ".join(text.split())[:LONGEST]
        if not t or _PRIVATE.search(t):
            return
        key = t.lower()
        now = int(time.time())
        e = self.store.data.get(key)
        if e is None:
            e = self.store.data[key] = {"text": t, "kind": kind, "count": 0, "first": now, "im": 0, "room": 0}
        e["count"] += 1
        e["last"] = now
        e["kind"] = kind
        e["room" if room else "im"] += 1
        if len(self.store.data) > KEEP:
            drop = sorted(self.store.data, key=lambda k: (self.store.data[k]["count"], self.store.data[k]["last"]))
            for k in drop[: len(self.store.data) - KEEP]:
                del self.store.data[k]
        self.store.save()

    def top(self, n: int = 40, kind: str | None = None) -> list[dict]:
        rows = [e for e in self.store.data.values() if not kind or e["kind"] == kind]
        return sorted(rows, key=lambda e: (-e["count"], -e["last"]))[:n]

    def clear(self) -> None:
        self.store.data = {}
        self.store.save()


def main() -> None:
    args = sys.argv[1:]
    data = Path(os.environ.get("SMARTERCHILD_DATA", "data"))
    m = Misses(data)
    if "--clear" in args:
        n = len(m.store.data)
        m.clear()
        print(f"Cleared {n} phrases.")
        return
    n = next((int(a) for a in args if a.isdigit()), 40)
    kind = next((a for a in args if a in ("fallback", "lookup")), None)
    rows = m.top(n, kind)
    if not rows:
        print("Nothing yet.")
        return
    total = sum(e["count"] for e in m.store.data.values())
    print(f"{len(m.store.data)} phrases, {total} messages. Most asked first:\n")
    for e in rows:
        when = time.strftime("%m-%d", time.localtime(e["last"]))
        print(f"{e['count']:5}  {e['kind']:8}  {when}  {e['text']}")


if __name__ == "__main__":
    main()

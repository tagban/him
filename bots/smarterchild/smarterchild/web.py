"""Small HTTP helpers for the skills: free services only, no API keys."""

from __future__ import annotations

import asyncio
import json
import time
import urllib.parse
import urllib.request

UA = "SmarterChild/0.1 (Hotline IM bot; +https://github.com/tagban/him)"
_cache: dict[str, tuple[float, object]] = {}


def _get(url: str, timeout: float) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": "application/json"})
    with urllib.request.urlopen(req, timeout=timeout) as r:
        return r.read(2_000_000)


async def get_json(url: str, params: dict | None = None, *, ttl: float = 0, timeout: float = 8) -> object:
    """GET a JSON document (kept `ttl` seconds when asked). Raises on failure."""
    if params:
        url += "?" + urllib.parse.urlencode(params)
    hit = _cache.get(url)
    if hit and hit[0] > time.time():
        return hit[1]
    data = json.loads(await asyncio.to_thread(_get, url, timeout))
    if ttl:
        _cache[url] = (time.time() + ttl, data)
    return data


def quote(s: str) -> str:
    return urllib.parse.quote(s.replace(" ", "_"), safe="")

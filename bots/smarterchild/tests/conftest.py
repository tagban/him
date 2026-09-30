"""Runs HIM's test server (hotline-im's mock-server) for the integration tests.

It is built from this repo: `cargo build -p hotline-im --features mock-server --bin mock-server`.
Tests that need it are skipped when it isn't there.
"""

import os
import random
import socket
import subprocess
import time
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[3]
MOCK = ROOT / "target" / "debug" / "mock-server"


def _free_pair() -> int:
    """A port whose neighbor below is free too (the test server also opens port - 1)."""
    for _ in range(50):
        p = random.randint(20000, 40000)
        ok = True
        for q in (p, p - 1):
            with socket.socket() as s:
                try:
                    s.bind(("127.0.0.1", q))
                except OSError:
                    ok = False
        if ok:
            return p
    raise RuntimeError("no free ports")


@pytest.fixture
def mock_server():
    if not MOCK.exists():
        pytest.skip("build the test server first: cargo build -p hotline-im --features mock-server --bin mock-server")
    port = _free_pair()
    proc = subprocess.Popen([str(MOCK), str(port)], stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                            env={**os.environ, "MOCK_AGREEMENT": ""})
    deadline = time.time() + 10
    while time.time() < deadline:
        with socket.socket() as s:
            if s.connect_ex(("127.0.0.1", port)) == 0:
                break
        time.sleep(0.05)
    yield port
    proc.terminate()
    proc.wait(timeout=5)

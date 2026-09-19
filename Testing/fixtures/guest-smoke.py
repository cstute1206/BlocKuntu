#!/usr/bin/env python3
"""Start/stop the HTTP and process fixtures twice inside a clean test clone."""
import json
import signal
from pathlib import Path
import subprocess
import sys
import time
import urllib.request


def ready(path):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        try:
            return json.loads(path.read_text())
        except (OSError, ValueError):
            time.sleep(0.1)
    raise RuntimeError(f"No readiness file: {path}")


def identity(pid, comm, executable):
    assert Path(f"/proc/{pid}/comm").read_text().strip() == comm
    assert Path(f"/proc/{pid}/exe").readlink().name == executable


for iteration in range(2):
    directory = Path(f"fixture-run-{iteration}")
    directory.mkdir()
    processes = []
    helper = None
    try:
        with (directory / "site.log").open("w") as log:
            site = subprocess.Popen([sys.executable, "server.py", "--port", "0", "--ready-file", str(directory / "site.json")], stdout=log, stderr=log)
        processes.append(site)
        port = ready(directory / "site.json")["port"]
        with urllib.request.urlopen(f"http://127.0.0.1:{port}/healthz", timeout=5) as response:
            assert response.status == 200
        for executable, comm in [("blockuntu-test-app-a", "bk-test-app-a"), ("blockuntu-test-app-b", "bk-test-app-b"), ("blockuntu-test-block", "bk-test-block")]:
            process = subprocess.Popen([f"./{executable}", "--lifetime", "20", "--ready-file", str(directory / executable)])
            processes.append(process)
            ready(directory / executable)
            identity(process.pid, comm, executable)
        parent = subprocess.Popen(["./blockuntu-test-parent", "--lifetime", "20", "--spawn-helper", "./blockuntu-test-helper", "--ready-file", str(directory / "parent.json")])
        processes.append(parent)
        helper = ready(directory / "parent.json")["child_pid"]
        identity(parent.pid, "bk-test-parent", "blockuntu-test-parent")
        time.sleep(0.2)
        identity(helper, "bk-test-helper", "blockuntu-test-helper")
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
    assert site.returncode in (0, -signal.SIGTERM)
    assert all(process.returncode == 0 for process in processes if process is not site)
    assert helper is not None and not Path(f"/proc/{helper}").exists()
print(json.dumps({"iterations": 2, "http_health": True, "process_identities": True, "helper_reaped": True, "all_stopped": True}))

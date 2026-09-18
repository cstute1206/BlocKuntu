#!/usr/bin/env python3
"""Run one source-validation job; retain all logs and command outcomes on failure."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
RUST = {"core": "focus-core", "daemon": "focusd", "native": "native-host", "tauri": "focus-gui/src-tauri"}
NODE = {"gui": "focus-gui", "firefox": "browser-extension-firefox", "chrome": "browser-extension-chrome"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("component", choices=[*RUST, *NODE])
    parser.add_argument("--install", action="store_true", help="Install locked npm dependencies first")
    args = parser.parse_args()
    report_dir = ROOT / "Testing/results/pr-ci" / args.component
    report_dir.mkdir(parents=True, exist_ok=True)
    env = {**os.environ, "TZ": "Europe/Berlin", "CI": "true", "CARGO_TERM_COLOR": "never"}
    if args.component in RUST:
        cwd = ROOT / RUST[args.component]
        commands = [
            ["cargo", "fmt", "--check"],
            ["cargo", "clippy", "--locked", "--all-targets", "--", "-D", "warnings"],
            ["cargo", "test", "--locked", "--all-targets"],
            ["cargo", "test", "--locked", "--doc"],
        ]
    else:
        cwd = ROOT / NODE[args.component]
        commands = ([["npm", "ci"]] if args.install else [])
        commands += [["npm", "run", "test"], ["npm", "run", "build"], ["npm", "run", "check"]]
    if args.component == "core":
        commands += [["python3", str(ROOT / "Testing/scripts/test_pr_ci.py")], ["python3", str(ROOT / "Testing/scripts/check-pr-coverage.py")]]
    results = []
    for index, command in enumerate(commands):
        print(f"[{args.component}] {' '.join(command)}", flush=True)
        started = time.monotonic()
        log = report_dir / f"{index + 1:02d}-{Path(command[1]).name}.log"
        with log.open("w") as output:
            try:
                status = subprocess.run(command, cwd=cwd, env=env, stdout=output, stderr=subprocess.STDOUT, check=False).returncode
            except OSError as error:
                output.write(str(error))
                status = 127
        print(log.read_text(), end="", flush=True)
        results.append({"command": command, "exit_code": status, "seconds": round(time.monotonic() - started, 3), "log": log.name})
        (report_dir / "result.json").write_text(json.dumps({"layer": "source-validation", "component": args.component, "completed": len(results) == len(commands), "passed": len(results) == len(commands) and all(item["exit_code"] == 0 for item in results), "commands": results}, indent=2) + "\n")
        if command == ["npm", "ci"] and status:
            break  # Never validate stale node_modules after a failed locked install.
    return int(any(item["exit_code"] != 0 for item in results))


if __name__ == "__main__":
    sys.exit(main())

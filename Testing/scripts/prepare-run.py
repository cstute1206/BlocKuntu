#!/usr/bin/env python3
"""Create an isolated BlocKuntu test-run directory under Testing/results."""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path


TEST_HOSTS = (
    "web.blockuntu.test",
    "allowed.blockuntu.test",
    "outside.blockuntu.test",
    "hard.blockuntu.test",
    "scheduled.blockuntu.test",
    "metered.blockuntu.test",
)


def parse_args() -> argparse.Namespace:
    testing_root = Path(__file__).resolve().parent.parent
    timestamp = dt.datetime.now(dt.timezone.utc).strftime("run-%Y%m%dT%H%M%SZ")
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-id", default=timestamp)
    parser.add_argument("--results-root", type=Path, default=testing_root / "results")
    parser.add_argument("--site-port", type=int, default=18080)
    parser.add_argument("--package", type=Path)
    parser.add_argument("--vm-template", choices=("ubuntu", "fedora", "cachyos"))
    parser.add_argument("--browser")
    parser.add_argument("--browser-package")
    return parser.parse_args()


def safe_run_id(value: str) -> str:
    if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9._-]{0,79}", value):
        raise ValueError(
            "run ID must start with a letter or digit and contain at most 80 "
            "letters, digits, dots, underscores, or hyphens"
        )
    return value


def policy_id_prefix(run_id: str) -> str:
    normalized = re.sub(r"[^a-z0-9]+", "-", run_id.lower()).strip("-")
    if not normalized:
        raise ValueError("run ID does not produce a usable policy ID")
    return f"test-{normalized}"


def git_value(repo_root: Path, *args: str) -> str | None:
    result = subprocess.run(
        ["git", *args],
        cwd=repo_root,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return None
    return result.stdout.strip()


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def policy_toml(prefix: str, port: int) -> str:
    web = f"http://web.blockuntu.test:{port}"
    return f'''[strict_mode]
require_firefox_extension = true
require_chrome_extension = true
kill_supported_browser_if_extension_stale = true
block_unsupported_browsers = true
grace_seconds = 30

[[allowances]]
id = "{prefix}-site-block-daily"
name = "Test website blocklist allowance"
daily_minutes = 1

[[allowances]]
id = "{prefix}-app-block-daily"
name = "Test application blocklist allowance"
daily_minutes = 1

[[allowances]]
id = "{prefix}-site-allow-daily"
name = "Test website allowlist allowance"
daily_minutes = 1

[[allowances]]
id = "{prefix}-app-allow-daily"
name = "Test application allowlist allowance"
daily_minutes = 1

[[schedules]]
id = "{prefix}-always"
name = "Nearly always active test schedule"
windows = [{{ weekday = "everyday", start = "00:00", end = "23:59" }}]

[[schedules]]
id = "{prefix}-inactive"
name = "Inactive test schedule"

[[rules]]
id = "{prefix}-hard-domain"
name = "Hard domain fixture"
tier = "hard"
patterns = [
  {{ kind = "domain", value = "hard.blockuntu.test", match_subdomains = true }}
]

[[rules]]
id = "{prefix}-hard-exact"
name = "Hard exact URL fixture"
tier = "hard"
patterns = [
  {{ kind = "exact_url", value = "{web}/exact/blocked" }}
]

[[rules]]
id = "{prefix}-hard-prefix"
name = "Hard URL prefix fixture"
tier = "hard"
patterns = [
  {{ kind = "url_prefix", value = "{web}/prefix/blocked" }}
]

[[rules]]
id = "{prefix}-hard-contains"
name = "Hard URL contains fixture"
tier = "hard"
patterns = [
  {{ kind = "url_contains", value = "blockuntu-marker" }}
]

[[rules]]
id = "{prefix}-hard-path"
name = "Hard path prefix fixture"
tier = "hard"
patterns = [
  {{ kind = "path_prefix", value = "web.blockuntu.test/path/blocked" }}
]

[[rules]]
id = "{prefix}-scheduled-site-block"
name = "Inactive Tier 2 website blocklist fixture"
tier = "scheduled_block"
schedule_ids = ["{prefix}-inactive"]
patterns = [
  {{ kind = "domain", value = "scheduled.blockuntu.test", match_subdomains = true }}
]

[[rules]]
id = "{prefix}-controlled-site-block"
name = "Inactive Tier 3 website blocklist fixture"
tier = "controlled_access"
schedule_ids = ["{prefix}-inactive"]
allowance_id = "{prefix}-site-block-daily"
patterns = [
  {{ kind = "domain", value = "metered.blockuntu.test", match_subdomains = true }}
]

[[rules]]
id = "{prefix}-tier2-site-allow"
name = "Inactive Tier 2 website allowlist fixture"
tier = "scheduled_block"
mode = "allowlist"
schedule_ids = ["{prefix}-inactive"]
patterns = [
  {{ kind = "domain", value = "allowed.blockuntu.test", match_subdomains = true }}
]

[[rules]]
id = "{prefix}-tier3-site-allow"
name = "Inactive Tier 3 website allowlist fixture"
tier = "controlled_access"
mode = "allowlist"
schedule_ids = ["{prefix}-inactive"]
allowance_id = "{prefix}-site-allow-daily"
patterns = [
  {{ kind = "domain", value = "allowed.blockuntu.test", match_subdomains = true }}
]

[[app_rules]]
id = "{prefix}-hard-app-block"
name = "Hard application blocklist fixture"
tier = "hard"
matchers = [
  {{ kind = "command_name", value = "bk-test-block" }}
]

[[app_rules]]
id = "{prefix}-scheduled-app-block"
name = "Inactive Tier 2 application blocklist fixture"
tier = "scheduled_block"
schedule_ids = ["{prefix}-inactive"]
matchers = [
  {{ kind = "command_name", value = "bk-test-app-b" }}
]

[[app_rules]]
id = "{prefix}-controlled-app-block"
name = "Inactive Tier 3 application blocklist fixture"
tier = "controlled_access"
schedule_ids = ["{prefix}-inactive"]
allowance_id = "{prefix}-app-block-daily"
matchers = [
  {{ kind = "command_name", value = "bk-test-app-b" }}
]

[[app_rules]]
id = "{prefix}-tier2-app-allow"
name = "Inactive Tier 2 application allowlist fixture"
tier = "scheduled_block"
mode = "allowlist"
schedule_ids = ["{prefix}-inactive"]
matchers = [
  {{ kind = "command_name", value = "bk-test-app-a" }}
]

[[app_rules]]
id = "{prefix}-tier3-app-allow"
name = "Inactive Tier 3 application allowlist fixture"
tier = "controlled_access"
mode = "allowlist"
schedule_ids = ["{prefix}-inactive"]
allowance_id = "{prefix}-app-allow-daily"
matchers = [
  {{ kind = "command_name", value = "bk-test-app-a" }}
]
'''


def fixture_inventory(port: int) -> dict[str, object]:
    web = f"http://web.blockuntu.test:{port}"
    return {
        "hosts": list(TEST_HOSTS),
        "site": {
            "health": f"{web}/healthz",
            "free": f"{web}/free",
            "exact_positive": f"{web}/exact/blocked",
            "exact_negative": f"{web}/exact/blocked/child",
            "url_prefix_positive": f"{web}/prefix/blocked/child",
            "url_prefix_negative": f"{web}/prefix/free",
            "url_contains_positive": f"{web}/contains?marker=blockuntu-marker",
            "url_contains_negative": f"{web}/contains?marker=safe",
            "path_prefix_positive": f"{web}/path/blocked/child",
            "path_prefix_negative": f"{web}/path/free",
            "spa": f"{web}/spa",
            "long_running": f"{web}/long-running",
            "allowed": f"http://allowed.blockuntu.test:{port}/allowed",
            "outside": f"http://outside.blockuntu.test:{port}/outside",
            "hard_domain": f"http://hard.blockuntu.test:{port}/free",
            "scheduled_domain": f"http://scheduled.blockuntu.test:{port}/free",
            "metered_domain": f"http://metered.blockuntu.test:{port}/free",
        },
        "processes": {
            "app_a": {
                "executable": "blockuntu-test-app-a",
                "command_name": "bk-test-app-a",
            },
            "app_b": {
                "executable": "blockuntu-test-app-b",
                "command_name": "bk-test-app-b",
            },
            "block": {
                "executable": "blockuntu-test-block",
                "command_name": "bk-test-block",
            },
            "parent": {
                "executable": "blockuntu-test-parent",
                "command_name": "bk-test-parent",
            },
            "helper": {
                "executable": "blockuntu-test-helper",
                "command_name": "bk-test-helper",
            },
        },
    }


def main() -> int:
    args = parse_args()
    try:
        run_id = safe_run_id(args.run_id)
        if not 1 <= args.site_port <= 65535:
            raise ValueError("site port must be between 1 and 65535")

        testing_root = Path(__file__).resolve().parent.parent
        repo_root = testing_root.parent
        results_root = args.results_root.resolve()
        try:
            results_root.relative_to(testing_root)
        except ValueError as error:
            raise ValueError(
                f"results root must stay inside {testing_root}: {results_root}"
            ) from error
        run_dir = results_root / run_id
        if run_dir.exists():
            raise ValueError(f"run directory already exists: {run_dir}")

        package_metadata = None
        if args.package is not None:
            package_path = args.package.resolve(strict=True)
            package_metadata = {
                "path": str(package_path),
                "filename": package_path.name,
                "size_bytes": package_path.stat().st_size,
                "sha256": sha256(package_path),
            }

        prefix = policy_id_prefix(run_id)
        policy = policy_toml(prefix, args.site_port)
        tomllib.loads(policy)

        for relative in (
            "policy",
            "fixtures",
            "evidence/journal",
            "evidence/browser",
            "evidence/screenshots",
            "evidence/exports",
            "reports",
        ):
            (run_dir / relative).mkdir(parents=True, exist_ok=False)

        policy_path = run_dir / "policy/blockuntu-policy.toml"
        policy_path.write_text(policy, encoding="utf-8")

        inventory = fixture_inventory(args.site_port)
        (run_dir / "fixtures/inventory.json").write_text(
            json.dumps(inventory, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        hosts_line = "127.0.0.1 " + " ".join(TEST_HOSTS) + "\n"
        (run_dir / "fixtures/hosts.entries").write_text(hosts_line, encoding="utf-8")

        commit = git_value(repo_root, "rev-parse", "HEAD")
        status = git_value(repo_root, "status", "--porcelain")
        created_at = dt.datetime.now(dt.timezone.utc).isoformat()
        metadata = {
            "schema_version": 1,
            "run_id": run_id,
            "created_at": created_at,
            "repository": {
                "root": str(repo_root),
                "commit": commit,
                "dirty": bool(status),
            },
            "package": package_metadata,
            "vm_template": args.vm_template,
            "browser": {
                "name": args.browser,
                "package": args.browser_package,
            }
            if args.browser or args.browser_package
            else None,
            "fixtures": {
                "policy": "policy/blockuntu-policy.toml",
                "inventory": "fixtures/inventory.json",
                "hosts_entries": "fixtures/hosts.entries",
                "site_port": args.site_port,
                "policy_id_prefix": prefix,
            },
        }
        (run_dir / "metadata.json").write_text(
            json.dumps(metadata, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        (run_dir / "results.json").write_text(
            json.dumps(
                {
                    "schema_version": 1,
                    "run_id": run_id,
                    "status": "planned",
                    "cases": [],
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        (run_dir / "README.md").write_text(
            f"""# BlocKuntu test run `{run_id}`

- Created: `{created_at}`
- Commit: `{commit or 'unavailable'}`
- VM template: `{args.vm_template or 'not selected'}`
- Policy: `policy/blockuntu-policy.toml`
- Fixture inventory: `fixtures/inventory.json`
- Case results: `results.json`

Store all generated evidence inside this directory. Do not edit the clean VM
template to preserve a result.
""",
            encoding="utf-8",
        )
    except (OSError, ValueError, tomllib.TOMLDecodeError) as error:
        print(f"prepare-run: {error}", file=sys.stderr)
        return 1

    print(run_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Regression tests for failure propagation and evidence retention in the CI runner."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("pr_ci", Path(__file__).with_name("pr-ci.py"))
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class RunnerTests(unittest.TestCase):
    def run_job(self, component, statuses, install=False):
        with tempfile.TemporaryDirectory() as directory:
            def execute(command, **kwargs):
                kwargs["stdout"].write("test evidence\n")
                return type("Result", (), {"returncode": next(statuses)})()
            with patch.object(runner, "ROOT", Path(directory)), patch.object(runner.subprocess, "run", side_effect=execute) as calls, patch("sys.argv", ["pr-ci.py", component] + (["--install"] if install else [])):
                result = runner.main()
                report_dir = Path(directory) / "Testing/results/pr-ci" / component
                data = json.loads((report_dir / "result.json").read_text())
                self.assertTrue(all((report_dir / item["log"]).read_text() == "test evidence\n" for item in data["commands"]))
                return result, data, calls.call_args_list

    def test_failure_survives_later_success_and_all_rust_checks_run(self):
        code, report, calls = self.run_job("native", iter([1, 0, 0, 0]))
        self.assertEqual(code, 1)
        self.assertFalse(report["passed"])
        self.assertEqual(len(calls), 4)
        for item in report["commands"][1:]:
            self.assertIn("--locked", item["command"])

    def test_failed_install_never_tests_stale_dependencies(self):
        code, report, calls = self.run_job("gui", iter([1]), install=True)
        self.assertEqual(code, 1)
        self.assertEqual(len(calls), 1)
        self.assertEqual(report["commands"][0]["command"], ["npm", "ci"])

    def test_core_audit_commands_with_absolute_paths_have_safe_log_names(self):
        code, report, calls = self.run_job("core", iter([0] * 6))
        self.assertEqual(code, 0)
        self.assertEqual(len(calls), 6)
        self.assertTrue(report["completed"])
        self.assertTrue(all("/" not in item["log"] for item in report["commands"]))

    def test_success_requires_every_command(self):
        code, report, _ = self.run_job("firefox", iter([0, 0, 0]))
        self.assertEqual(code, 0)
        self.assertTrue(report["passed"])


if __name__ == "__main__":
    unittest.main()

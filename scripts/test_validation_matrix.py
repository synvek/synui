"""Integration tests for validation_matrix workload selection and attribution."""

import json
import tempfile
import unittest
from argparse import Namespace
from pathlib import Path
from unittest.mock import patch

from scripts import validation_matrix as matrix


class ValidationMatrixTest(unittest.TestCase):
    def test_workloads_and_selection_are_explicit_and_ordered(self):
        self.assertEqual(
            [workload.id for workload in matrix.WORKLOADS],
            [
                "counter",
                "facade-compilation",
                "component-interaction",
                "virtual-table",
                "virtual-data",
                "rich-text",
                "rich-text-style",
                "platform",
                "headless-render",
                "native-abi",
                "python-sdk",
                "a11y",
            ],
        )
        selected = matrix.select_workloads("a11y,counter,python-sdk")
        self.assertEqual([workload.id for workload in selected], ["a11y", "counter", "python-sdk"])
        with self.assertRaisesRegex(ValueError, "unknown workload"):
            matrix.select_workloads("missing")

    def test_failed_workload_is_independently_attributed(self):
        workload = matrix.Workload(
            "fake",
            "fake workload",
            "test boundary",
            lambda _: [["first"], ["second"]],
        )
        completed = []

        def run(command, **kwargs):
            completed.append((command, kwargs))
            return Namespace(returncode=1 if len(completed) == 1 else 0)

        with patch.object(matrix.subprocess, "run", side_effect=run):
            result = matrix.run_workload(
                workload,
                matrix.Context("python", Path("target/debug/libsynui_abi.dylib")),
            )
        self.assertEqual(result["status"], "failed")
        self.assertEqual(len(result["commands"]), 2)
        self.assertEqual(result["commands"][0]["exit_code"], 1)
        self.assertEqual(result["commands"][1]["exit_code"], 0)
        self.assertEqual(completed[0][1]["cwd"], matrix.ROOT)
        self.assertIn("SYNUI_ABI_LIBRARY", completed[0][1]["env"])

    def test_report_generation_and_platform_ci_wiring(self):
        with tempfile.TemporaryDirectory() as directory:
            report = Path(directory) / "report.json"
            args = Namespace(
                list=False,
                only="counter,a11y",
                python="python",
                abi_library=Path("abi.dylib"),
                report=report,
            )
            results = {
                "counter": {"id": "counter", "status": "passed"},
                "a11y": {"id": "a11y", "status": "passed"},
            }
            with patch.object(matrix, "parse_args", return_value=args), patch.object(
                matrix,
                "run_workload",
                side_effect=lambda workload, context: results[workload.id],
            ):
                self.assertEqual(matrix.main(), 0)
            document = json.loads(report.read_text(encoding="utf-8"))
            self.assertEqual(document["selected"], ["counter", "a11y"])
            self.assertEqual([item["status"] for item in document["results"]], ["passed", "passed"])

        ci = (matrix.ROOT / ".github/workflows/ci.yml").read_text(encoding="utf-8")
        for platform in ("ubuntu", "windows", "macos"):
            self.assertIn(platform, ci)
        self.assertIn("validation-matrix", ci)
        self.assertIn("a11y", ci)

    def test_a11y_metadata_and_bridge_are_tri_state(self):
        no_metadata = matrix.WorkItem("no-semantics", (), metadata_present=False)
        self.assertEqual(matrix._a11y_status(no_metadata)[0], "not_applicable")
        unavailable_bridge = matrix.WorkItem("bridge-missing", (), metadata_present=True, bridge_available=False)
        status, activation, checks = matrix._a11y_status(unavailable_bridge)
        self.assertEqual((status, activation), ("unavailable", "unavailable"))
        self.assertTrue(any(check["name"] == "a11y_bridge" for check in checks))

    def test_headless_checks_require_no_system_window_and_all_assertions(self):
        item = matrix.WorkItem("fixture", ())
        result = [{"status": "passed", "details": "system_window_created=false; headless fixture completed"}]
        status, activation, checks = matrix._headless_checks(item, result)
        self.assertEqual((status, activation), ("passed", "passed"))
        self.assertEqual({check["name"] for check in checks}, {"system_window_created", "layout_paint", "mounted_virtualized_bounds", "example_assertion"})
        self.assertFalse(next(check for check in checks if check["name"] == "system_window_created")["value"])

        created_window = [{"status": "passed", "details": "system_window_created=true; host window created"}]
        status, _, checks = matrix._headless_checks(item, created_window)
        self.assertEqual(status, "failed")
        self.assertEqual(next(check for check in checks if check["name"] == "system_window_created")["status"], "failed")

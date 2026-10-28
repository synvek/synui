"""Focused rule tests for migration waves and conservative gate history."""
import tempfile
import unittest
from pathlib import Path

from scripts import migration_gate as gate


class MigrationGateTest(unittest.TestCase):
    def test_wave_order_and_contract_prerequisites_are_blocking(self):
        findings = gate.check_wave_prerequisites(
            "data/virtualized",
            contract_changed=True,
            fixture_results={"table_fixture": "failed"},
            prior_examples={"todo_list": "passed"},
            completed_waves={"inventory/export", "component exposure", "minimal fixtures", "low-risk"},
        )
        self.assertEqual([item.code for item in findings], ["WaveFixturePrerequisite"])
        order = gate.check_wave_prerequisites("low-risk", completed_waves={"inventory/export"})
        self.assertEqual(order[0].code, "WaveOrderViolation")

    def test_history_does_not_clear_without_approved_reset(self):
        finding = gate._finding("ForbiddenLegacyEntry", "old View", package="example")
        state, history, _ = gate.evaluate_gate([finding])
        self.assertEqual(state, "blocked")
        state, history, _ = gate.evaluate_gate([], history)
        self.assertEqual(state, "blocked")
        state, history, errors = gate.evaluate_gate([], history, reset={
            "approver": "maintainer", "reason": "classified retained boundary",
            "classification": "retained_engine", "impact_boundary": "example only", "reset_id": "reset-1",
        })
        self.assertEqual((state, errors), ("passing", []))
        self.assertEqual(history["resets"][-1]["reset_id"], "reset-1")

    def test_incomplete_reset_is_rejected(self):
        history, errors = gate.apply_manual_reset({}, approver="maintainer", reason=None, classification="deferred", impact_boundary="example", reset_id="r1")
        self.assertEqual(history, {})
        self.assertEqual(errors[0].code, "ManualResetIncomplete")

    def test_report_contains_explicit_empty_categories_and_commands(self):
        report = gate.build_report({}, "passing", [], None, {"state": "passing"})
        self.assertEqual(report["deferred"], [])
        self.assertEqual(report["retained_engine"], [])
        self.assertEqual(report["non_production"], [])
        self.assertTrue(any("validation_matrix.py" in command for command in report["exact_commands"]))

    def test_report_contains_explicit_empty_categories_and_commands(self):
        report = gate.build_report({}, "passing", [], None, {"state": "passing"})
        self.assertEqual(report["deferred"], [])
        self.assertEqual(report["retained_engine"], [])
        self.assertEqual(report["non_production"], [])
        self.assertTrue(any("validation_matrix.py" in command for command in report["exact_commands"]))

    def test_final_report_schema_keeps_each_validation_boundary_independent(self):
        matrix = {"validation_results": [
            {"workload": "facade-compilation", "example_or_fixture": "counter", "boundary": "facade-only",
             "status": "passed", "activation_status": "passed", "checks": [], "commands": [],
             "platform": "desktop", "details": "facade"},
            {"workload": "headless-render", "example_or_fixture": "counter", "boundary": "headless",
             "status": "passed", "activation_status": "passed", "checks": [], "commands": [],
             "platform": "desktop", "details": "headless"},
            {"workload": "a11y", "example_or_fixture": "counter", "boundary": "a11y",
             "status": "unavailable", "activation_status": "passed", "checks": [], "commands": [],
             "platform": "desktop", "details": "bridge unavailable"},
            {"workload": "native-abi", "example_or_fixture": "native_abi", "boundary": "Production ABI",
             "status": "passed", "activation_status": "passed", "checks": [], "commands": [],
             "platform": "darwin", "details": "ABI"},
            {"workload": "python-sdk", "example_or_fixture": "python-sdk", "boundary": "Python SDK / Production ABI",
             "status": "passed", "activation_status": "passed", "checks": [], "commands": [],
             "platform": "darwin", "details": "Python"},
        ]}
        report = gate.build_report({}, "blocked", [], matrix, {"state": "blocked"}, commit="test-commit")
        self.assertEqual(gate.validate_report(report), [])
        self.assertEqual(report["commit"], "test-commit")
        self.assertEqual(len(report["validation_results"]), 5)
        self.assertEqual({item["workload"] for item in report["validation_results"]},
                         {"facade-compilation", "headless-render", "a11y", "native-abi", "python-sdk"})
        self.assertEqual(report["gate_status"], "blocked")
        self.assertEqual(report["documentation_index"][:3], [
            "docs/api-reference-v0.2.md",
            "docs/migration-v0.2.md",
            "docs/component-api-example-migration.toml",
        ])

    def test_report_document_index_records_canonical_paths_and_boundary_links(self):
        manifest = {
            "documentation": [],
            "components": [{"id": "button", "canonical_path": "synui::widgets::Button", "stability": "stable", "type_name": "Button", "prelude": True}],
            "examples": [],
        }
        report = gate.build_report(manifest, "passing", [], None, {"state": "passing"})
        self.assertEqual(gate.validate_report(report, manifest), [])
        self.assertEqual(report["stable_components"], ["synui::widgets::Button"])
        self.assertEqual(report["stable_component_records"][0]["module_path"], "synui::widgets")

    def test_contract_change_requires_complete_fixture_and_prior_example_evidence(self):
        findings = gate.check_wave_prerequisites(
            "rich-text/style",
            contract_changed=True,
            completed_waves={"inventory/export", "component exposure", "minimal fixtures", "low-risk", "data/virtualized"},
            required_fixtures={"external_component_fixture", "block_doc"},
            fixture_results={"external_component_fixture": "passed"},
            required_examples={"counter", "todo_list"},
            prior_examples={"counter": "passed", "todo_list": "unavailable"},
        )
        self.assertEqual(
            [item.code for item in findings],
            ["WaveFixturePrerequisite", "PriorExamplePrerequisite"],
        )
        self.assertIn("block_doc", findings[0].evidence)
        self.assertIn("todo_list", findings[1].evidence)
        self.assertEqual(findings[0].boundary, "wave prerequisite")

    def test_abi_and_python_results_cannot_satisfy_migration_prerequisites(self):
        findings = gate.check_wave_prerequisites(
            "low-risk",
            contract_changed=True,
            completed_waves={"inventory/export", "component exposure", "minimal fixtures"},
            fixture_results={"native-abi": "passed"},
            prior_examples={"python-sdk": {"status": "passed", "workload": "python-sdk"}},
        )
        self.assertEqual(
            [item.code for item in findings],
            ["WaveEvidenceBoundaryViolation", "WaveFixturePrerequisite", "WaveEvidenceBoundaryViolation", "PriorExamplePrerequisite"],
        )
        self.assertTrue(all("ABI/Python" in item.evidence or "missing or not passed" in item.evidence for item in findings))

    def test_wave_plan_keeps_canonical_order_and_attaches_block_reason(self):
        finding = gate.check_wave_prerequisites("data/virtualized", completed_waves={"inventory/export"})[0]
        plan = gate.wave_plan(findings=[finding])
        self.assertEqual([wave["id"] for wave in plan], list(gate.WAVE_ORDER))
        blocked = plan[gate.WAVE_ORDER.index("data/virtualized")]
        self.assertEqual(blocked["status"], "blocked")
        self.assertEqual(blocked["findings"][0]["code"], "WaveOrderViolation")


if __name__ == "__main__":
    unittest.main()

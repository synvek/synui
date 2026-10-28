#!/usr/bin/env python3
"""Conservative migration wave planner, gate, history, and final report.

This checker is intentionally static. It reads Cargo metadata, the Migration
Manifest, source/dependency evidence, the read-only ABI reference path, and the
Validation Matrix; it never changes ABI, SDK, or Engine implementation files.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable

ROOT = Path(__file__).resolve().parents[1]
MANIFEST_PATH = ROOT / "docs/component-api-example-migration.toml"
MATRIX_PATH = ROOT / "target/validation-matrix/component-api-example-migration.json"
REPORT_PATH = ROOT / "target/migration-gate/report.json"
HISTORY_PATH = ROOT / "target/migration-gate/finding-history.json"
MIGRATION_WORKLOADS = (
    "facade-compilation", "component-interaction", "virtual-data",
    "rich-text-style", "headless-render", "a11y",
)
WAVE_ORDER = (
    "inventory/export",
    "component exposure",
    "minimal fixtures",
    "low-risk",
    "data/virtualized",
    "rich-text/style",
    "platform",
    "gallery/final",
)
WAVE_ALIASES = {
    "inventory": "inventory/export", "export": "inventory/export", "component": "component exposure",
    "fixtures": "minimal fixtures", "fixture": "minimal fixtures", "data": "data/virtualized",
    "virtualized": "data/virtualized", "rich-text": "rich-text/style", "style": "rich-text/style",
    "gallery": "gallery/final", "final": "gallery/final",
}
FORBIDDEN_FEATURES = {"legacy", "plugins"}
FORBIDDEN_IMPORTS = (
    "ViewEngine", "WidgetStore", "Signal", "Binding", "key_scope", "section_fill", "style_css",
    "parse_view_json", "view_from_json", "numeric action", "numeric key",
)
ENGINE_DEPENDENCIES = {"synui-render", "synui-winit", "synui-widgets", "synui-platform", "synui-core", "winit"}
ABI_REFERENCE = "crates/synui-abi/abi/synui_v1.reference.toml"

REPORT_SCHEMA_VERSION = 1
REPORT_CLASSIFICATIONS = ("migrated", "deferred", "retained_engine", "non_production")
INDEPENDENT_VALIDATION_PATHS = (
    ROOT / "target/validation-matrix/python-sdk.json",
    ROOT / "target/validation-matrix/text-abi-final.json",
)
REPORT_REQUIRED_FIELDS = {
    "schema_version", "commit", "waves", "examples_by_classification", "migrated",
    "deferred", "retained_engine", "non_production", "stable_components",
    "unresolved_risks", "gate_status", "validation_results", "exact_commands",
    "documentation_index", "documentation_index_entries",
}


def now() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


@dataclass(frozen=True)
class Finding:
    code: str
    package: str | None
    path: str | None
    boundary: str
    evidence: str
    severity: str = "blocking"
    remediation: str = ""
    first_seen: str | None = None
    manual_reset_required: bool = True

    def key(self) -> str:
        raw = "|".join(str(value) for value in (self.code, self.package, self.path, self.boundary, self.evidence))
        return hashlib.sha256(raw.encode("utf-8")).hexdigest()[:20]

    def json(self) -> dict[str, Any]:
        value = asdict(self)
        value["finding_id"] = self.key()
        return value


def _finding(code: str, evidence: str, *, package: str | None = None, path: str | None = None,
             boundary: str = "migration", remediation: str = "", severity: str = "blocking") -> Finding:
    return Finding(code, package, path, boundary, evidence, severity, remediation, now())


def load_manifest() -> dict[str, Any]:
    try:
        from migration_manifest import load_toml  # type: ignore
        manifest, errors = load_toml(MANIFEST_PATH)
        if errors or manifest is None:
            return {}
        return manifest
    except (ImportError, OSError, ValueError):
        return {}


def cargo_metadata() -> dict[str, Any] | None:
    command = ["cargo", "metadata", "--format-version", "1", "--no-deps", "--manifest-path", str(ROOT / "Cargo.toml")]
    try:
        result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
    except OSError:
        return None
    if result.returncode:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def canonical_wave(value: str) -> str:
    normalized = str(value).strip().lower()
    return WAVE_ALIASES.get(normalized, normalized)


def _wave_finding(item: Finding) -> bool:
    """Whether a finding is a wave planning/prerequisite result."""
    return item.boundary == "wave prerequisite" or item.code in {
        "UnknownMigrationWave", "UnknownCompletedWave", "WaveOrderViolation",
        "WaveFixturePrerequisite", "PriorExamplePrerequisite", "WaveEvidenceBoundaryViolation",
    }


def wave_plan(*, completed_waves: Iterable[str] = (), current_wave: str | None = None,
              findings: Iterable[Finding] = ()) -> list[dict[str, Any]]:
    """Return the immutable canonical plan with auditable status per wave.

    ``completed_waves`` is intentionally explicit: a successful ABI or Python
    workload never marks a migration wave complete.  A current wave is marked
    ``ready`` only when no finding is attached to it; callers may use the
    prerequisite checker below to produce those findings.
    """
    completed = {canonical_wave(item) for item in completed_waves}
    current = canonical_wave(current_wave) if current_wave is not None else None
    wave_findings = list(findings)
    plan: list[dict[str, Any]] = []
    for index, wave in enumerate(WAVE_ORDER):
        attached = [item.json() for item in wave_findings if item.path == wave]
        if attached:
            status = "blocked"
        elif wave in completed:
            status = "completed"
        elif current == wave:
            status = "ready"
        else:
            status = "planned"
        plan.append({
            "order": index,
            "id": wave,
            "prerequisites": list(WAVE_ORDER[:index]),
            "status": status,
            "findings": attached,
        })
    return plan


def _result_status(value: Any) -> str:
    if isinstance(value, dict):
        return str(value.get("status", "")).strip().lower()
    return str(value).strip().lower()


def _result_workload(value: Any) -> str | None:
    if not isinstance(value, dict) or value.get("workload") is None:
        return None
    return str(value["workload"]).strip().lower()


def _status_passed(value: Any) -> bool:
    """Only an explicitly passed migration result is eligible evidence."""
    workload = _result_workload(value)
    return _result_status(value) == "passed" and (workload is None or workload in MIGRATION_WORKLOADS)


def _boundary_violation(name: str, value: Any) -> bool:
    """Reject native ABI/Python results as fixture/example migration evidence."""
    workload = _result_workload(value)
    lowered = str(name).strip().lower()
    return workload in {"native-abi", "python-sdk"} or lowered in {"native-abi", "python-sdk"}


def _prerequisite_names(results: dict[str, Any] | None, required: Iterable[str] | None) -> tuple[list[str], list[str], list[str]]:
    supplied = results or {}
    required_names = {str(name) for name in (required or ())}
    missing = sorted(required_names - set(supplied))
    not_passed = sorted(name for name, result in supplied.items() if not _status_passed(result))
    boundary = sorted(name for name, result in supplied.items() if _boundary_violation(name, result))
    return missing, not_passed, boundary


def check_wave_prerequisites(current_wave: str, *, contract_changed: bool = False,
                              fixture_results: dict[str, Any] | None = None,
                              prior_examples: dict[str, Any] | None = None,
                              completed_waves: Iterable[str] = (),
                              required_fixtures: Iterable[str] | None = None,
                              required_examples: Iterable[str] | None = None) -> list[Finding]:
    """Return blocking findings if a wave is out of order or lacks migration evidence.

    When a stable component contract changes, both the changed component fixture
    set and every already-migrated example are mandatory evidence.  The sets are
    passed explicitly so an ABI/Python result cannot silently stand in for either
    migration boundary.
    """
    wave = canonical_wave(current_wave)
    if wave not in WAVE_ORDER:
        return [_finding(
            "UnknownMigrationWave", f"Unknown migration wave {current_wave!r}.",
            path=wave, boundary="wave prerequisite",
            remediation=f"Use one of: {', '.join(WAVE_ORDER)}.",
        )]
    index = WAVE_ORDER.index(wave)
    completed_values = [canonical_wave(item) for item in completed_waves]
    completed = set(completed_values)
    findings: list[Finding] = []
    unknown_completed = sorted(set(completed_values) - set(WAVE_ORDER))
    if unknown_completed:
        findings.append(_finding(
            "UnknownCompletedWave", f"Unknown completed wave evidence: {', '.join(unknown_completed)}.",
            path=wave, boundary="wave prerequisite",
            remediation="Record only canonical migration wave ids.",
        ))
    missing = [required for required in WAVE_ORDER[:index] if required not in completed]
    if missing:
        findings.append(_finding(
            "WaveOrderViolation", f"Wave {wave} cannot start before: {', '.join(missing)}.",
            path=wave, boundary="wave prerequisite",
            remediation="Complete waves in canonical inventory/export through gallery/final order.",
        ))
    if not contract_changed:
        return findings

    fixture_missing, failed_fixtures, invalid_fixture_evidence = _prerequisite_names(fixture_results, required_fixtures)
    if fixture_results is None and not fixture_missing:
        fixture_missing = ["changed component fixture results"]
    if invalid_fixture_evidence:
        findings.append(_finding(
            "WaveEvidenceBoundaryViolation",
            "ABI/Python workload results cannot satisfy changed component fixtures: "
            + ", ".join(invalid_fixture_evidence) + ".",
            path=wave, boundary="wave prerequisite",
            remediation="Run the corresponding facade migration fixture; do not substitute native ABI or Python SDK results.",
        ))
    fixture_blockers = sorted(set(fixture_missing) | set(failed_fixtures) | set(invalid_fixture_evidence))
    if fixture_blockers:
        findings.append(_finding(
            "WaveFixturePrerequisite",
            "Changed component fixtures are not all passed (missing or not passed): "
            + ", ".join(fixture_blockers) + ".",
            path=wave, boundary="wave prerequisite",
            remediation="Run and pass every changed component fixture before the next wave.",
        ))

    example_missing, failed_examples, invalid_example_evidence = _prerequisite_names(prior_examples, required_examples)
    if prior_examples is None and not example_missing:
        example_missing = ["prior migrated example results"]
    if invalid_example_evidence:
        findings.append(_finding(
            "WaveEvidenceBoundaryViolation",
            "ABI/Python workload results cannot satisfy migrated example prerequisites: "
            + ", ".join(invalid_example_evidence) + ".",
            path=wave, boundary="wave prerequisite",
            remediation="Run every migrated example's migration workloads; do not substitute native ABI or Python SDK results.",
        ))
    example_blockers = sorted(set(example_missing) | set(failed_examples) | set(invalid_example_evidence))
    if example_blockers:
        findings.append(_finding(
            "PriorExamplePrerequisite",
            "Previously migrated examples are not all passed (missing or not passed): "
            + ", ".join(example_blockers) + ".",
            path=wave, boundary="wave prerequisite",
            remediation="Run and pass all prior migrated examples before advancing the wave.",
        ))
    return findings


def _git_changed(path: str) -> bool:
    try:
        result = subprocess.run(["git", "diff", "--name-only", "HEAD", "--", path], cwd=ROOT, text=True, capture_output=True, check=False)
        return bool(result.stdout.strip())
    except OSError:
        return False


def _record_source(record: dict[str, Any]) -> str:
    return "\n".join(str(path) for path in record.get("entry_paths", []))


def manifest_findings(manifest: dict[str, Any], metadata: dict[str, Any] | None) -> list[Finding]:
    if not manifest or metadata is None:
        return []
    try:
        from migration_manifest import validate_manifest  # type: ignore
        errors = validate_manifest(manifest, metadata, ROOT)
    except (ImportError, AttributeError, TypeError):
        return [_finding("ManifestValidatorUnavailable", "The manifest validator could not be loaded.", path=str(MANIFEST_PATH), remediation="Run scripts/migration_manifest.py --check with the same workspace.")]
    return [_finding(
        error.code, error.message, package=error.package, path=error.path,
        boundary="migration manifest", remediation=error.remediation,
    ) for error in errors]


def scan_manifest_and_sources(manifest: dict[str, Any], metadata: dict[str, Any] | None) -> list[Finding]:
    findings: list[Finding] = []
    if metadata is None:
        findings.append(_finding("CargoMetadataUnavailable", "Cargo metadata could not be read.", path="Cargo.toml", remediation="Provide a valid Cargo workspace before running the migration gate."))
    examples = manifest.get("examples", []) if manifest else []
    for record in examples:
        package = str(record.get("package", ""))
        classification = str(record.get("classification", ""))
        path = str(record.get("manifest_path", ""))
        if classification in {"retained_engine", "non_production", "deferred"}:
            docs = [ROOT / str(value) for value in record.get("documentation", [])]
            readme = next((doc for doc in docs if doc.name.lower() == "readme.md"), None)
            if readme is None or not readme.is_file():
                findings.append(_finding("BoundaryDocumentationMissing", f"{classification} package has no README boundary evidence.", package=package, path=path, boundary=classification, remediation="Document retained/deferred/non-production dependencies and replacement/deferral links in the README."))
        if classification != "migrated":
            continue
        features = set(str(value) for value in record.get("features", [])) | {value.split(":", 1)[1] for value in record.get("dependency_features", []) if ":" in value}
        for feature in sorted(FORBIDDEN_FEATURES & features):
            findings.append(_finding("ForbiddenFeature", f"Migrated example enables forbidden feature {feature!r}.", package=package, path=path, boundary="migrated facade", remediation="Remove the feature or classify the package as an explicit retained/non-production/deferred boundary."))
        dependencies = set(str(value) for value in record.get("dependencies", []))
        for dependency in sorted(ENGINE_DEPENDENCIES & dependencies):
            # Explicit host support and BlockDoc data support are allowed only when
            # the manifest and README declare that non-ordinary boundary.
            allowed_data_support = dependency == "synui-core" and "rich-text" in set(record.get("components", [])) and "data" in str(record.get("replacement", "")).lower()
            allowed_host_support = dependency in {"synui-winit", "synui-platform", "winit"} and bool(record.get("host_boundary"))
            if not allowed_data_support and not allowed_host_support:
                findings.append(_finding("DirectEngineDependency", f"Migrated example declares direct Engine dependency {dependency!r}.", package=package, path=path, boundary="migrated facade", remediation="Use synui facade or classify the package's host/Engine integration explicitly."))
        for entry in record.get("entry_paths", []):
            source_path = ROOT / str(entry)
            if not source_path.is_file():
                findings.append(_finding("MigratedSourceMissing", f"Migrated source entry is missing: {entry}.", package=package, path=str(entry), remediation="Restore the declared entry or update the manifest."))
                continue
            source = source_path.read_text(encoding="utf-8", errors="replace")
            for forbidden in FORBIDDEN_IMPORTS:
                if forbidden in source:
                    findings.append(_finding("ForbiddenLegacyEntry", f"Migrated source contains forbidden legacy/Engine entry {forbidden!r}.", package=package, path=str(entry), boundary="migrated facade", remediation="Replace the old entry with the documented facade or record an approved non-ordinary boundary."))
    if _git_changed(ABI_REFERENCE):
        findings.append(_finding("ProductionAbiChanged", "The versioned Production ABI reference has changed in this checkout.", path=ABI_REFERENCE, boundary="native ABI", remediation="Restore the ABI reference; example migration must not alter the published subset."))
    return findings


def matrix_findings(matrix: dict[str, Any] | None, manifest: dict[str, Any]) -> tuple[list[Finding], list[dict[str, Any]]]:
    if not matrix:
        return [_finding("MatrixMissingEvidence", f"Validation Matrix report is missing: {MATRIX_PATH}.", path=str(MATRIX_PATH), remediation="Run the migration Matrix and preserve its independent result artifact.")], []
    selected = set(str(value) for value in matrix.get("selected", []))
    missing_workloads = sorted(set(MIGRATION_WORKLOADS) - selected)
    findings: list[Finding] = []
    if missing_workloads:
        findings.append(_finding("MigrationWorkloadMissing", f"Matrix does not contain all six migration workloads: {', '.join(missing_workloads)}.", path=str(MATRIX_PATH), boundary="migration matrix", remediation="Run facade-compilation, component-interaction, virtual-data, rich-text-style, headless-render, and a11y together."))
    validation = list(matrix.get("validation_results", []))
    for result in validation:
        status = str(result.get("status", ""))
        if status == "failed":
            findings.append(_finding("ValidationFailure", f"{result.get('workload')} failed for {result.get('example_or_fixture')}.", package=str(result.get("example_or_fixture")), path=str(MATRIX_PATH), boundary=str(result.get("boundary", "migration")), remediation="Fix the originating workload; unrelated workloads remain independently attributed."))
        elif status == "unavailable":
            # Unavailable platform/bridge evidence is retained as an unresolved risk,
            # not silently changed to passed and not attributed to another boundary.
            findings.append(_finding("ValidationUnavailable", f"{result.get('workload')} is unavailable for {result.get('example_or_fixture')}.", package=str(result.get("example_or_fixture")), path=str(MATRIX_PATH), boundary=str(result.get("boundary", "migration")), severity="risk", remediation="Provide the declared target/bridge or retain the explicit unavailable classification."))
        if result.get("workload") == "headless-render":
            checks = {str(check.get("name")): check for check in result.get("checks", []) if isinstance(check, dict)}
            if checks.get("system_window_created", {}).get("value") is not False:
                findings.append(_finding("HeadlessWindowCreated", f"Headless workload created a system window for {result.get('example_or_fixture')}.", package=str(result.get("example_or_fixture")), path=str(MATRIX_PATH), boundary="headless", remediation="Use the no-window HeadlessRenderer path and assert system_window_created=false."))
        if result.get("workload") == "a11y" and result.get("status") == "passed" and any(check.get("status") == "unavailable" for check in result.get("checks", []) if isinstance(check, dict)):
            findings.append(_finding("A11yUnavailableCountedAsPassed", "A11y bridge unavailable result was counted as passed.", package=str(result.get("example_or_fixture")), path=str(MATRIX_PATH), boundary="a11y", remediation="Use unavailable when metadata exists but the bridge cannot run."))
    return findings, validation


def load_history(path: Path = HISTORY_PATH) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
        return value if isinstance(value, dict) else {}
    except (FileNotFoundError, json.JSONDecodeError):
        return {"state": "clean", "findings": {}, "transitions": [], "resets": []}


def apply_manual_reset(history: dict[str, Any], *, approver: str | None, reason: str | None,
                       classification: str | None, impact_boundary: str | None, reset_id: str | None) -> tuple[dict[str, Any], list[Finding]]:
    missing = [name for name, value in (("approver", approver), ("reason", reason), ("classification", classification), ("impact_boundary", impact_boundary), ("reset_id", reset_id)) if not value]
    if missing:
        return history, [_finding("ManualResetIncomplete", f"Manual reset is missing: {', '.join(missing)}.", boundary="gate history", remediation="Provide approver, reason, classification, impact boundary, and reset id.")]
    reset = {"approver": approver, "reason": reason, "classification": classification, "impact_boundary": impact_boundary, "reset_id": reset_id, "at": now()}
    history.setdefault("resets", []).append(reset)
    history["state"] = "clean"
    return history, []


def evaluate_gate(findings: list[Finding], history: dict[str, Any] | None = None,
                  reset: dict[str, str] | None = None) -> tuple[str, dict[str, Any], list[Finding]]:
    history = dict(history or {"state": "clean", "findings": {}, "transitions": [], "resets": []})
    history.setdefault("findings", {})
    history.setdefault("transitions", [])
    history.setdefault("resets", [])
    reset_findings: list[Finding] = []
    if reset is not None:
        history, reset_findings = apply_manual_reset(history, **reset)
    current = {item.key(): item.json() for item in findings}
    history["findings"].update(current)
    prior_state = str(history.get("state", "clean"))
    has_blocking = any(item.severity == "blocking" for item in findings)
    if has_blocking:
        state = "blocked"
    elif prior_state == "blocked":
        # Removing a finding never clears history. Only apply_manual_reset above
        # changes this state to clean before re-evaluation.
        state = "blocked"
    else:
        state = "passing"
    history["state"] = state
    history["transitions"].append({"from": prior_state, "to": state, "at": now(), "active_finding_ids": sorted(current)})
    return state, history, reset_findings


def _read_matrix(path: Path) -> dict[str, Any] | None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
        return value if isinstance(value, dict) else None
    except (FileNotFoundError, json.JSONDecodeError):
        return None


def _read_validation_artifact(path: Path) -> dict[str, Any] | None:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (FileNotFoundError, json.JSONDecodeError):
        return None
    return value if isinstance(value, dict) else None


def _artifact_validation_results() -> list[dict[str, Any]]:
    """Load independent ABI/Python evidence without merging its boundary.

    Migration Matrix artifacts remain the source for migration workloads.  The
    supplemental artifacts are deliberately normalized only when they contain
    no per-item ``validation_results`` (the native ABI report currently uses a
    workload-level result), and are never used by ``matrix_findings``.
    """
    results: list[dict[str, Any]] = []
    seen_workloads: set[str] = set()
    for path in INDEPENDENT_VALIDATION_PATHS:
        artifact = _read_validation_artifact(path)
        if not artifact:
            continue
        for result in artifact.get("validation_results", []):
            if not isinstance(result, dict):
                continue
            workload = str(result.get("workload", ""))
            if workload in {"native-abi", "python-sdk"} and workload not in seen_workloads:
                results.append(result)
                seen_workloads.add(workload)
        for result in artifact.get("results", []):
            if not isinstance(result, dict):
                continue
            workload = str(result.get("id", ""))
            if workload not in {"native-abi", "python-sdk"} or workload in seen_workloads:
                continue
            commands = result.get("commands", [])
            results.append({
                "workload": workload,
                "example_or_fixture": workload,
                "boundary": str(result.get("boundary", result.get("category", "independent"))),
                "platform": str(artifact.get("platform", "unknown")),
                "status": str(result.get("status", "unavailable")),
                "activation_status": str(result.get("status", "unavailable")),
                "checks": [],
                "commands": commands if isinstance(commands, list) else [],
                "details": str(result.get("description", "")),
            })
            seen_workloads.add(workload)
    return results


def _documentation_paths(manifest: dict[str, Any]) -> list[str]:
    paths: list[str] = []
    declared = manifest.get("documentation")
    if isinstance(declared, list):
        values = declared
    else:
        values = ["docs/api-reference-v0.2.md", "docs/migration-v0.2.md"]
    for value in values:
        if isinstance(value, str) and value not in paths:
            paths.append(value)
    manifest_path = "docs/component-api-example-migration.toml"
    if manifest_path not in paths:
        paths.append(manifest_path)
    for record in manifest.get("examples", []):
        for value in record.get("documentation", []):
            if isinstance(value, str) and value not in paths:
                paths.append(value)
    return paths


def _documentation_index(manifest: dict[str, Any]) -> tuple[list[str], list[dict[str, Any]]]:
    """Return the shared document list plus auditable canonical/boundary details."""
    components = [record for record in manifest.get("components", []) if isinstance(record, dict)]
    canonical_paths = [str(record.get("canonical_path")) for record in components if record.get("canonical_path")]
    paths = _documentation_paths(manifest)
    entries: list[dict[str, Any]] = []
    for relative in paths:
        path = ROOT / relative
        text = path.read_text(encoding="utf-8", errors="replace") if path.is_file() else ""
        boundary_lines = [
            line.strip() for line in text.splitlines()
            if re.search(r"non[- ]ordinary|retained Engine|non-production|experimental|advanced|deferred|replacement|deferral", line, re.IGNORECASE)
        ]
        links = re.findall(r"\[[^\]]+\]\([^\)]+\)", text)
        entries.append({
            "path": relative,
            "exists": path.is_file(),
            "canonical_paths": [canonical for canonical in canonical_paths if canonical in text],
            "non_ordinary_references": boundary_lines,
            "replacement_or_deferral_links": links,
        })
    return paths, entries


def validate_report(report: dict[str, Any], manifest: dict[str, Any] | None = None) -> list[str]:
    """Validate the stable, machine-readable shape of a final MigrationReport."""
    errors = sorted(REPORT_REQUIRED_FIELDS - set(report))
    if report.get("schema_version") != REPORT_SCHEMA_VERSION:
        errors.append("schema_version")
    if report.get("gate_status") not in {"passing", "blocked"}:
        errors.append("gate_status")
    for name in REPORT_CLASSIFICATIONS:
        if not isinstance(report.get(name), list):
            errors.append(name)
    if not isinstance(report.get("waves"), list) or [item.get("id") for item in report.get("waves", []) if isinstance(item, dict)] != list(WAVE_ORDER):
        errors.append("waves")
    if not isinstance(report.get("validation_results"), list):
        errors.append("validation_results")
    else:
        for index, result in enumerate(report["validation_results"]):
            if not isinstance(result, dict) or any(key not in result for key in ("workload", "example_or_fixture", "boundary", "status", "activation_status", "checks", "commands", "platform", "details")):
                errors.append(f"validation_results[{index}]")
    if not isinstance(report.get("exact_commands"), list) or not all(isinstance(command, str) and command.strip() for command in report.get("exact_commands", [])):
        errors.append("exact_commands")
    if manifest is not None:
        expected = {record.get("canonical_path") for record in manifest.get("components", []) if isinstance(record, dict) and record.get("stability") == "stable"}
        if not expected.issubset(set(report.get("stable_components", []))):
            errors.append("stable_components")
    return sorted(set(errors))


def build_report(manifest: dict[str, Any], state: str, findings: list[Finding], matrix: dict[str, Any] | None,
                 history: dict[str, Any], *, commit: str = "unknown",
                 completed_waves: Iterable[str] = (), current_wave: str | None = None) -> dict[str, Any]:
    examples = list(manifest.get("examples", []))
    categories = {name: [record.get("package") for record in examples if record.get("classification") == name] for name in REPORT_CLASSIFICATIONS}
    stable_records = [record for record in manifest.get("components", []) if record.get("stability") == "stable"]
    stable = [record.get("canonical_path") for record in stable_records]
    stable_details = [{
        "id": record.get("id"),
        "canonical_path": record.get("canonical_path"),
        "module_path": "::".join(str(record.get("canonical_path")).split("::")[:2]),
        "type_name": record.get("type_name"),
        "prelude": record.get("prelude"),
        "stability": record.get("stability"),
    } for record in stable_records]
    risks = [
        {"package": record.get("package"), "risk": risk}
        for record in examples for risk in record.get("risks", [])
    ]
    risks.extend({"code": item.code, "risk": item.evidence, "severity": item.severity} for item in findings if item.severity != "blocking")
    validation = list((matrix or {}).get("validation_results", []))
    commands = [
        "python3 scripts/migration_manifest.py --check",
        "python3 scripts/migration_gate.py --check",
        "python3 scripts/test_migration_gate.py",
        "python3 scripts/test_validation_matrix.py",
        "python3 scripts/validation_matrix.py --only facade-compilation,component-interaction,virtual-data,rich-text-style,headless-render,a11y --report target/validation-matrix/component-api-example-migration.json",
        "cargo test -p synui-abi --test native_abi -- --nocapture",
        "python3 scripts/validation_matrix.py --only python-sdk",
        "git diff --check",
    ]
    for result in validation:
        for command in result.get("commands", []):
            argv = command.get("argv") if isinstance(command, dict) else None
            if isinstance(argv, list):
                exact = " ".join(str(value) for value in argv)
                if exact and exact not in commands:
                    commands.append(exact)
    waves = wave_plan(completed_waves=completed_waves, current_wave=current_wave, findings=findings)
    historical_findings = list((history.get("findings") or {}).values())
    blocked_findings = historical_findings if state == "blocked" else []
    documentation_paths, documentation_entries = _documentation_index(manifest)
    report = {
        "schema_version": REPORT_SCHEMA_VERSION,
        "commit": commit,
        "generated_at": now(),
        "waves": waves,
        "examples_by_classification": categories,
        "migrated": categories["migrated"],
        "deferred": categories["deferred"],
        "retained_engine": categories["retained_engine"],
        "non_production": categories["non_production"],
        "stable_components": stable,
        "stable_component_records": stable_details,
        "unresolved_risks": risks,
        "gate_status": state,
        "findings": [item.json() for item in findings],
        "blocked_findings": blocked_findings,
        "finding_history": history,
        "validation_results": validation,
        "exact_commands": commands,
        "documentation_index": documentation_paths,
        "documentation_index_entries": documentation_entries,
    }
    return report


def _git_commit() -> str:
    try:
        result = subprocess.run(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True, capture_output=True, check=False)
        return result.stdout.strip() if result.returncode == 0 else "unknown"
    except OSError:
        return "unknown"


def run(*, matrix_path: Path = MATRIX_PATH, report_path: Path = REPORT_PATH, history_path: Path = HISTORY_PATH,
        reset: dict[str, str] | None = None) -> tuple[int, dict[str, Any]]:
    manifest = load_manifest()
    metadata = cargo_metadata()
    findings = manifest_findings(manifest, metadata)
    findings.extend(scan_manifest_and_sources(manifest, metadata))
    matrix = _read_matrix(matrix_path)
    matrix_errors, _ = matrix_findings(matrix, manifest)
    findings.extend(matrix_errors)
    report_matrix = dict(matrix or {})
    report_matrix["validation_results"] = list(report_matrix.get("validation_results", [])) + _artifact_validation_results()
    history = load_history(history_path)
    state, history, reset_errors = evaluate_gate(findings, history, reset=reset)
    findings.extend(reset_errors)
    if reset_errors:
        state = "blocked"
        history["state"] = state
    report = build_report(manifest, state, findings, report_matrix, history, commit=_git_commit())
    report_path.parent.mkdir(parents=True, exist_ok=True)
    history_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    history_path.write_text(json.dumps(history, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return (1 if state == "blocked" else 0), report


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Check migration waves, boundaries, Matrix evidence, gate history, and final report.")
    parser.add_argument("--check", action="store_true", help="run the conservative migration gate (default)")
    parser.add_argument(
        "--legacy-free-check",
        action="store_true",
        help="Phase 49 Wave H3: ordinary facade packages must not require synui/legacy; cargo check them without legacy",
    )
    parser.add_argument(
        "--facade-regression",
        action="store_true",
        help="Phase 50 Wave I0: must-green H0/session UI regression cargo test filters",
    )
    parser.add_argument("--report", type=Path, default=REPORT_PATH)
    parser.add_argument("--history", type=Path, default=HISTORY_PATH)
    parser.add_argument("--matrix", type=Path, default=MATRIX_PATH)
    parser.add_argument("--reset-id")
    parser.add_argument("--approver")
    parser.add_argument("--reason")
    parser.add_argument("--classification")
    parser.add_argument("--impact-boundary")
    return parser.parse_args(argv)


# Packages allowed to enable `synui/legacy`. Empty after Phase 51 Wave J feature removal.
LEGACY_OPT_IN_PACKAGES = frozenset()

# Ordinary facade demos + core crates that must build without `legacy`.
LEGACY_FREE_CHECK_PACKAGES = (
    "synui",
    "synui-core",
    "synui-widgets",
    "synui-layout",
    "synui-render",
    "synui-winit",
    "counter",
    "todo_list",
    "list_demo",
    "tree_list_demo",
    "table_demo",
    "data_browser_demo",
    "navigation_demo",
    "style_demo",
    "style_sticky_demo",
    "rich_edit_demo",
    "rich_blocks_demo",
    "show_image",
    "multi_window_demo",
    "widget_gallery_facade",
)


def _package_enables_synui_legacy(cargo_toml: Path) -> bool:
    text = cargo_toml.read_text(encoding="utf-8", errors="replace")
    # Match `synui = { ..., features = [..., "legacy", ...] }` in package manifests.
    return bool(re.search(r'synui\s*=\s*\{[^}]*features\s*=\s*\[[^\]]*["\']legacy["\']', text, re.S))


def run_legacy_free_check(*, run_cargo: bool = True) -> tuple[int, dict[str, Any]]:
    """Static + optional cargo check: facade path does not require `feature = \"legacy\"`."""
    findings: list[dict[str, Any]] = []
    synui_cargo = (ROOT / "crates/synui/Cargo.toml").read_text(encoding="utf-8", errors="replace")
    if re.search(r'(?m)^\s*legacy\s*=', synui_cargo):
        findings.append({
            "code": "SynUiLegacyFeatureStillPresent",
            "package": "synui",
            "path": "crates/synui/Cargo.toml",
            "evidence": "synui still declares a legacy feature after the v0.3 cut",
            "remediation": "Remove the legacy feature from crates/synui/Cargo.toml (Phase 51 Wave J).",
        })
    examples_root = ROOT / "examples"
    for cargo_toml in sorted(examples_root.glob("*/Cargo.toml")):
        package = cargo_toml.parent.name
        if package in LEGACY_OPT_IN_PACKAGES:
            continue
        if _package_enables_synui_legacy(cargo_toml):
            findings.append({
                "code": "OrdinaryExampleEnablesLegacy",
                "package": package,
                "path": str(cargo_toml.relative_to(ROOT)),
                "evidence": "ordinary facade example enables synui/legacy",
                "remediation": "Remove legacy from features; the synui legacy feature was deleted in Phase 51.",
            })

    cargo: dict[str, Any] | None = None
    if run_cargo and not findings:
        packages = list(LEGACY_FREE_CHECK_PACKAGES)
        command = ["cargo", "check"]
        for name in packages:
            command.extend(["-p", name])
        # Default features on synui are empty (no legacy). Explicitly avoid --all-features.
        try:
            result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
        except OSError as exc:
            findings.append({
                "code": "LegacyFreeCargoUnavailable",
                "package": None,
                "path": "Cargo.toml",
                "evidence": str(exc),
                "remediation": "Install a Rust toolchain that can run cargo check.",
            })
            result = None
        if result is not None:
            cargo = {
                "command": " ".join(command),
                "returncode": result.returncode,
            }
            if result.returncode != 0:
                findings.append({
                    "code": "LegacyFreeCargoCheckFailed",
                    "package": None,
                    "path": "Cargo.toml",
                    "evidence": (result.stderr or result.stdout or "")[-4000:],
                    "remediation": "Fix compile errors in facade packages without enabling synui/legacy.",
                })

    ok = not findings
    report = {
        "ok": ok,
        "gate": "legacy-free-check",
        "legacy_opt_in": sorted(LEGACY_OPT_IN_PACKAGES),
        "checked_packages": list(LEGACY_FREE_CHECK_PACKAGES),
        "cargo": cargo,
        "findings": findings,
    }
    out = ROOT / "target/migration-gate/legacy-free-check.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return (0 if ok else 1), report


# Phase 50 Wave I0: session UI regressions (gap §8 + slider grab).
FACADE_REGRESSION_TESTS = (
    ("synui-widgets", "open_drawer_overlay"),
    ("synui", "navigation_drawer_overlay"),
    ("synui-widgets", "grid_three_equal_columns"),
    ("synui-widgets", "single_click_places_caret"),
    ("synui-widgets", "double_click_word_selection"),
    ("synui-render", "ascii_caret_x"),
    ("synui", "label_text_change_marks_layout"),
    ("synui", "facade_label_length_change_marks_layout"),
    ("synui", "facade_label_same_length_text_stays_paint"),
    ("synui-widgets", "slider_rejects_flex_shrink"),
    # Phase 50 Wave I smoke (NumberInput grab, Label ellipsis, DataTable merge spans).
    ("synui", "facade_number_input_drag_grabs_pointer"),
    ("synui", "facade_label_truncates_to_paint_width"),
    ("synui", "facade_data_table_applies_merge_spans"),
    ("synui-render", "fit_text_with_ellipsis"),
    # Phase 58 Wave Q: geometry vs paint text cache.
    ("synui-render", "measure_does_not_populate_paint_cache"),
    ("synui-render", "dark_on_surface_paint_uses_light_brush"),
)


def run_facade_regression() -> tuple[int, dict[str, Any]]:
    """Run must-green facade interaction regressions; fail the gate on any non-zero cargo test."""
    findings: list[dict[str, Any]] = []
    results: list[dict[str, Any]] = []
    for package, filter_name in FACADE_REGRESSION_TESTS:
        command = ["cargo", "test", "-p", package, "--lib", filter_name, "--", "--nocapture"]
        try:
            result = subprocess.run(command, cwd=ROOT, text=True, capture_output=True, check=False)
        except OSError as exc:
            findings.append({
                "code": "FacadeRegressionCargoUnavailable",
                "package": package,
                "path": filter_name,
                "evidence": str(exc),
                "remediation": "Install a Rust toolchain that can run cargo test.",
            })
            break
        entry = {
            "package": package,
            "filter": filter_name,
            "command": " ".join(command),
            "returncode": result.returncode,
        }
        results.append(entry)
        if result.returncode != 0:
            findings.append({
                "code": "FacadeRegressionFailed",
                "package": package,
                "path": filter_name,
                "evidence": (result.stderr or result.stdout or "")[-4000:],
                "remediation": "Fix the failing Wave H0/I regression before merging.",
            })

    ok = not findings
    report = {
        "ok": ok,
        "gate": "facade-regression",
        "tests": [{"package": p, "filter": f} for p, f in FACADE_REGRESSION_TESTS],
        "results": results,
        "findings": findings,
    }
    out = ROOT / "target/migration-gate/facade-regression.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    return (0 if ok else 1), report


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.legacy_free_check:
        code, report = run_legacy_free_check(run_cargo=True)
        print(json.dumps({
            "ok": code == 0,
            "gate": "legacy-free-check",
            "findings": len(report["findings"]),
            "report": "target/migration-gate/legacy-free-check.json",
        }, indent=2))
        return code
    if args.facade_regression:
        code, report = run_facade_regression()
        print(json.dumps({
            "ok": code == 0,
            "gate": "facade-regression",
            "findings": len(report["findings"]),
            "report": "target/migration-gate/facade-regression.json",
        }, indent=2))
        return code
    reset_values = [args.approver, args.reason, args.classification, args.impact_boundary, args.reset_id]
    reset = None
    if any(value is not None for value in reset_values):
        reset = {"approver": args.approver, "reason": args.reason, "classification": args.classification, "impact_boundary": args.impact_boundary, "reset_id": args.reset_id}
    code, report = run(matrix_path=args.matrix, report_path=args.report, history_path=args.history, reset=reset)
    print(json.dumps({"ok": code == 0, "gate_status": report["gate_status"], "findings": len(report["findings"]), "report": str(args.report)}, indent=2))
    return code


if __name__ == "__main__":
    raise SystemExit(main())

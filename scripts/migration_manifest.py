#!/usr/bin/env python3
"""Static coverage and evidence checker for the component migration manifest.

This command intentionally performs no build or runtime workload. It only reads
TOML/Markdown and asks Cargo for workspace metadata with ``--no-deps`` so that
example package coverage is compared with the actual workspace declarations.
"""
from __future__ import annotations

import argparse
import ast
import json
import subprocess
import sys
from dataclasses import dataclass, asdict
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.9/3.10 fallback keeps the checker dependency-free.
    tomllib = None  # type: ignore[assignment]

SCHEMA_VERSION = 1
CLASSIFICATIONS = {"migrated", "needs_migration", "retained_engine", "non_production", "deferred"}
PLATFORMS = {"desktop", "web", "android", "ios"}
WORKLOADS = {
    "facade-compilation",
    "component-interaction",
    "virtual-data",
    "rich-text-style",
    "headless-render",
    "a11y",
    "platform",
    "native-abi",
    "python-sdk",
    "plugin-experimental",
}
COMPONENT_STABILITY = {"stable", "external_element", "advanced_integration", "deferred"}
REQUIRES_EVIDENCE = {"retained_engine", "non_production", "deferred"}
DENYLIST_SYMBOLS = {
    "WidgetStore",
    "ViewEngine",
    "Engine layout/render types",
    "numeric action identifiers",
    "global key/memo/section scheduling",
    "plugin registry/loading symbols",
    "JSON View creation symbols",
}


@dataclass(frozen=True)
class Finding:
    code: str
    package: str | None
    path: str | None
    message: str
    remediation: str

    def json(self) -> dict[str, Any]:
        return asdict(self)


def finding(code: str, message: str, remediation: str, package: str | None = None, path: str | None = None) -> Finding:
    return Finding(code, package, path, message, remediation)


def repo_root_from(script: Path) -> Path:
    return script.resolve().parents[1]


def _strip_toml_comment(line: str) -> str:
    quoted = False
    escaped = False
    for index, char in enumerate(line):
        if char == '"' and not escaped:
            quoted = not quoted
        if char == "#" and not quoted:
            return line[:index].rstrip()
        escaped = char == "\\" and not escaped
        if char != "\\":
            escaped = False
    return line.rstrip()


def _fallback_toml_load(text: str) -> dict[str, Any]:
    """Parse the deliberately simple manifest subset on Python 3.9.

    The manifest uses scalar values, one-line arrays, and array-of-table
    records. Keeping this fallback local avoids adding a runtime dependency to
    a repository validation script.
    """
    root: dict[str, Any] = {}
    current: dict[str, Any] = root
    for raw in text.splitlines():
        line = _strip_toml_comment(raw).strip()
        if not line:
            continue
        if line.startswith("[[") and line.endswith("]]" ):
            key = line[2:-2].strip()
            record: dict[str, Any] = {}
            root.setdefault(key, []).append(record)
            current = record
            continue
        if line.startswith("[") and line.endswith("]"):
            key = line[1:-1].strip()
            current = root.setdefault(key, {})
            continue
        if "=" not in line:
            raise ValueError(f"expected key/value line: {raw}")
        key, raw_value = (part.strip() for part in line.split("=", 1))
        if raw_value == "true":
            value: Any = True
        elif raw_value == "false":
            value = False
        elif raw_value.startswith('"'):
            value = ast.literal_eval(raw_value)
        elif raw_value.startswith("["):
            value = ast.literal_eval(raw_value)
        else:
            try:
                value = int(raw_value)
            except ValueError:
                value = raw_value
        current[key] = value
    return root


def load_toml(path: Path) -> tuple[dict[str, Any] | None, list[Finding]]:
    try:
        text = path.read_text(encoding="utf-8")
    except FileNotFoundError:
        return None, [finding("ManifestMissing", f"Manifest does not exist: {path}", "Create docs/component-api-example-migration.toml.", path=str(path))]
    try:
        if tomllib is not None:
            value = tomllib.loads(text)
        else:
            value = _fallback_toml_load(text)
    except (ValueError, SyntaxError) as exc:
        return None, [finding("ManifestTomlInvalid", str(exc), "Fix TOML syntax before running the checker.", path=str(path))]
    if not isinstance(value, dict):
        return None, [finding("ManifestRootInvalid", "Manifest root must be a TOML table.", "Use a table root with [[components]] and [[examples]].", path=str(path))]
    return value, []


def cargo_metadata(root: Path) -> tuple[dict[str, Any] | None, list[Finding]]:
    # Metadata is a read-only workspace query. Never add a build, run, test, or
    # target command here: the manifest check must remain safe in headless CI.
    command = ["cargo", "metadata", "--format-version", "1", "--no-deps", "--manifest-path", str(root / "Cargo.toml")]
    try:
        result = subprocess.run(command, cwd=root, text=True, capture_output=True, check=False)
    except OSError as exc:
        return None, [finding("CargoMetadataUnavailable", str(exc), "Install Cargo or provide a valid workspace before checking coverage.", path="Cargo.toml")]
    if result.returncode != 0:
        detail = (result.stderr or result.stdout).strip().splitlines()[-1:]
        return None, [finding("CargoMetadataFailed", " ".join(detail) or "cargo metadata failed.", "Fix workspace metadata; this checker does not run builds.", path="Cargo.toml")]
    try:
        return json.loads(result.stdout), []
    except json.JSONDecodeError as exc:
        return None, [finding("CargoMetadataInvalid", str(exc), "Run cargo metadata manually and repair its JSON output.", path="Cargo.toml")]


def string_list(record: dict[str, Any], key: str, *, package: str, path: str, errors: list[Finding] | None, required: bool = True) -> list[str]:
    value = record.get(key)
    if not isinstance(value, list) or any(not isinstance(item, str) or not item.strip() for item in value):
        if errors is not None:
            errors.append(finding("ManifestFieldType", f"{key} must be a non-empty string array." if required else f"{key} must be a string array.", f"Set {key} to an explicit TOML array of strings.", package, path))
        return []
    if required and not value and errors is not None:
        errors.append(finding("ManifestMissingField", f"{key} must not be empty.", f"Add at least one {key} entry.", package, path))
    return value


def required_string(record: dict[str, Any], key: str, *, package: str, path: str, errors: list[Finding]) -> str:
    value = record.get(key)
    if not isinstance(value, str) or not value.strip():
        errors.append(finding("ManifestMissingField", f"{key} must be a non-empty string.", f"Add {key} to the record.", package, path))
        return ""
    return value


def workspace_examples(metadata: dict[str, Any], root: Path) -> dict[str, dict[str, Any]]:
    packages: dict[str, dict[str, Any]] = {}
    for package in metadata.get("packages", []):
        manifest = Path(package.get("manifest_path", ""))
        try:
            relative = manifest.resolve().relative_to(root.resolve()).as_posix()
        except ValueError:
            continue
        if relative.startswith("examples/") and relative.endswith("/Cargo.toml"):
            packages[package["name"]] = package | {"manifest_relative": relative}
    return packages


def declared_dependencies(package: dict[str, Any]) -> set[str]:
    names: set[str] = set()
    for dependency in package.get("dependencies", []):
        if not isinstance(dependency, dict):
            continue
        names.add(str(dependency.get("rename") or dependency.get("name") or ""))
    return {name for name in names if name}


def declared_features(package: dict[str, Any]) -> set[str]:
    features = package.get("features", {})
    return set(features) if isinstance(features, dict) else set()


def declared_dependency_features(package: dict[str, Any]) -> set[str]:
    values: set[str] = set()
    for dependency in package.get("dependencies", []):
        if not isinstance(dependency, dict):
            continue
        name = str(dependency.get("rename") or dependency.get("name") or "")
        for feature in dependency.get("features", []) or []:
            values.add(f"{name}:{feature}")
    return values


def validate_components(manifest: dict[str, Any], root: Path, errors: list[Finding]) -> dict[str, dict[str, Any]]:
    records = manifest.get("components")
    if not isinstance(records, list):
        errors.append(finding("ManifestComponentsMissing", "components must be an array of ComponentRecord tables.", "Add [[components]] records for every allowlist item.", path="docs/component-api-example-migration.toml"))
        return {}
    by_id: dict[str, dict[str, Any]] = {}
    stable_paths: dict[str, str] = {}
    for index, record in enumerate(records):
        path = f"docs/component-api-example-migration.toml:components[{index}]"
        if not isinstance(record, dict):
            errors.append(finding("ComponentRecordInvalid", "ComponentRecord must be a table.", "Use a [[components]] table.", path=path))
            continue
        component_id = record.get("id")
        if not isinstance(component_id, str) or not component_id.strip():
            errors.append(finding("ComponentIdMissing", "ComponentRecord id is missing.", "Give each component a unique canonical id.", path=path))
            continue
        if component_id in by_id:
            errors.append(finding("DuplicateComponent", f"Component {component_id!r} is registered more than once.", "Keep exactly one ComponentRecord per allowlist id.", path=path))
            continue
        by_id[component_id] = record
        for key in ("canonical_path", "type_name", "ownership", "thread_rule", "lifecycle", "error_contract", "identity_policy", "event_model", "a11y_metadata", "replacement", "boundary_note"):
            required_string(record, key, package=component_id, path=path, errors=errors)
        source_paths = string_list(record, "source_paths", package=component_id, path=path, errors=errors)
        for source in source_paths:
            if not (root / source).is_file():
                errors.append(finding("ComponentSourceMissing", f"Component source path does not exist: {source}", "Point source_paths at a delivered facade/compatibility implementation.", component_id, source))
        stability = record.get("stability")
        if stability not in COMPONENT_STABILITY:
            errors.append(finding("InvalidComponentStability", f"Unknown component stability {stability!r}.", "Use stable, external_element, advanced_integration, or deferred.", component_id, path))
        prelude = record.get("prelude")
        if not isinstance(prelude, bool):
            errors.append(finding("ComponentPreludeType", "prelude must be boolean.", "Set prelude = true only for documented stable application types.", component_id, path))
        elif prelude and stability != "stable":
            errors.append(finding("UnstableComponentExposure", "Only stable components may enter Prelude.", "Set prelude = false and document the external/advanced/deferred boundary.", component_id, path))
        if stability != "stable" and not record.get("boundary_note"):
            errors.append(finding("MissingBoundaryEvidence", "Non-stable component lacks a boundary note.", "Record why the internal implementation is not a Prelude contract.", component_id, path))
        fixture_commands = string_list(record, "fixture_commands", package=component_id, path=path, errors=errors)
        workloads = string_list(record, "validation_workloads", package=component_id, path=path, errors=errors)
        for workload in workloads:
            if workload not in WORKLOADS:
                errors.append(finding("UnknownWorkload", f"Unknown component validation workload {workload!r}.", "Use a workload from the manifest allowed_workloads registry.", component_id, path))
        for command in fixture_commands:
            if not command.strip():
                errors.append(finding("EmptyCommand", "Fixture command cannot be empty.", "Provide an exact, reviewable validation command.", component_id, path))
        canonical = record.get("canonical_path")
        if stability == "stable" and isinstance(canonical, str):
            if canonical in stable_paths:
                errors.append(finding("CanonicalPathConflict", f"Stable canonical path duplicates {stable_paths[canonical]}.", "Give each stable component one unique canonical path/type.", component_id, path))
            stable_paths[canonical] = component_id
    return by_id


def validate_examples(manifest: dict[str, Any], metadata: dict[str, Any], root: Path, components: dict[str, dict[str, Any]], errors: list[Finding]) -> None:
    records = manifest.get("examples")
    if not isinstance(records, list):
        errors.append(finding("ManifestExamplesMissing", "examples must be an array of ExampleRecord tables.", "Add one [[examples]] record per examples/**/Cargo.toml package.", path="docs/component-api-example-migration.toml"))
        return
    workspace = workspace_examples(metadata, root)
    seen: dict[str, str] = {}
    for index, record in enumerate(records):
        path = f"docs/component-api-example-migration.toml:examples[{index}]"
        if not isinstance(record, dict):
            errors.append(finding("ExampleRecordInvalid", "ExampleRecord must be a table.", "Use an [[examples]] table.", path=path))
            continue
        package = record.get("package")
        if not isinstance(package, str) or not package.strip():
            errors.append(finding("ExamplePackageMissing", "ExampleRecord package is missing.", "Use the Cargo package name, including synui_plugin_sample for plugin_sample.", path=path))
            continue
        if package in seen:
            errors.append(finding("DuplicateExample", f"Package {package!r} is registered more than once.", "Keep exactly one ExampleRecord per workspace example package.", package, path))
        seen[package] = path
        package_info = workspace.get(package)
        manifest_path = required_string(record, "manifest_path", package=package, path=path, errors=errors)
        if package_info is None:
            errors.append(finding("UnknownExamplePackage", f"{package!r} is not an examples workspace package.", "Use the exact package name returned by cargo metadata.", package, manifest_path or path))
        elif manifest_path != package_info["manifest_relative"]:
            errors.append(finding("ExampleManifestPathMismatch", f"Manifest path {manifest_path!r} does not match Cargo metadata {package_info['manifest_relative']!r}.", "Copy the package manifest path from cargo metadata.", package, manifest_path or path))
        elif not (root / manifest_path).is_file():
            errors.append(finding("ExampleManifestMissing", f"Example manifest does not exist: {manifest_path}", "Point manifest_path at an existing examples/**/Cargo.toml.", package, manifest_path))
        entries = string_list(record, "entry_paths", package=package, path=path, errors=errors)
        for entry in entries:
            if not (root / entry).is_file():
                errors.append(finding("ExampleEntryMissing", f"Entry path does not exist: {entry}", "Record every actual src/main.rs, src/lib.rs, or declared target entry.", package, entry))
        dependencies = string_list(record, "dependencies", package=package, path=path, errors=None)
        features = string_list(record, "features", package=package, path=path, errors=None)
        dependency_features = string_list(record, "dependency_features", package=package, path=path, errors=None)
        if package_info is not None:
            declared = declared_dependencies(package_info)
            if set(dependencies) != declared:
                errors.append(finding("DependencyCoverageMismatch", f"Manifest dependencies {sorted(dependencies)!r} differ from Cargo declarations {sorted(declared)!r}.", "Mirror every direct dependency name exactly; do not omit or invent dependencies.", package, manifest_path))
            cargo_features = declared_features(package_info)
            if set(features) != cargo_features:
                errors.append(finding("FeatureCoverageMismatch", f"Manifest features {sorted(features)!r} differ from Cargo declarations {sorted(cargo_features)!r}.", "Mirror every package feature key exactly, including an explicit default feature when Cargo declares one.", package, manifest_path))
            cargo_dependency_features = declared_dependency_features(package_info)
            if set(dependency_features) != cargo_dependency_features:
                errors.append(finding("DependencyFeatureCoverageMismatch", f"Manifest dependency_features {sorted(dependency_features)!r} differ from Cargo declarations {sorted(cargo_dependency_features)!r}.", "Record every dependency feature as dependency:feature.", package, manifest_path))
        legacy_entries = string_list(record, "legacy_entries", package=package, path=path, errors=None)
        referenced = string_list(record, "components", package=package, path=path, errors=None)
        for component in referenced:
            if component not in components:
                errors.append(finding("UnknownComponent", f"Example references unknown component {component!r}.", "Reference an id from the Component Allowlist or add an evidence-backed ComponentRecord.", package, path))
        platforms = string_list(record, "platforms", package=package, path=path, errors=errors)
        for platform in platforms:
            if platform not in PLATFORMS:
                errors.append(finding("InvalidPlatform", f"Unknown platform {platform!r}.", "Use desktop, web, android, or ios and record only declared targets.", package, path))
        classification = record.get("classification")
        if classification not in CLASSIFICATIONS:
            errors.append(finding("InvalidClassification", f"Unknown classification {classification!r}.", "Use migrated, needs_migration, retained_engine, non_production, or deferred.", package, path))
        replacement = required_string(record, "replacement", package=package, path=path, errors=errors)
        non_equivalent = required_string(record, "non_equivalent", package=package, path=path, errors=errors)
        required_string(record, "behavior_baseline", package=package, path=path, errors=errors)
        risks = string_list(record, "risks", package=package, path=path, errors=errors)
        workloads = string_list(record, "validation_workloads", package=package, path=path, errors=errors)
        commands = string_list(record, "commands", package=package, path=path, errors=errors)
        documentation = string_list(record, "documentation", package=package, path=path, errors=errors)
        for workload in workloads:
            if workload not in WORKLOADS:
                errors.append(finding("UnknownWorkload", f"Unknown example validation workload {workload!r}.", "Use a workload from the manifest allowed_workloads registry.", package, path))
        for command in commands:
            if not command.strip():
                errors.append(finding("EmptyCommand", "Example command cannot be empty.", "Provide an exact static validation command; the checker will not execute it.", package, path))
        if classification in REQUIRES_EVIDENCE or any("RemovedWithSteps" in item for item in legacy_entries):
            if not replacement.strip() or not non_equivalent.strip() or not risks or not workloads:
                errors.append(finding("IncompleteRemovalEvidence", f"{classification} or RemovedWithSteps record lacks replacement/non-equivalent/risk/validation evidence.", "Fill replacement, non_equivalent, risks, and validation_workloads before retaining or deferring the example.", package, path))
        if classification == "migrated" and ("legacy" in features or "plugins" in features or any(item in {"synui:legacy", "synui:plugins"} for item in dependency_features)):
            errors.append(finding("MigratedLegacyFeature", "Migrated example enables legacy/plugins.", "Remove the feature or classify the package as an explicit non-production/retained boundary.", package, manifest_path))
        for doc in documentation:
            if not (root / doc).is_file():
                errors.append(finding("DocumentationMissing", f"Documentation path does not exist: {doc}", "Add the referenced API/migration documentation before passing the manifest check.", package, doc))
    expected = set(workspace)
    actual = set(seen)
    for package in sorted(expected - actual):
        errors.append(finding("ManifestMissingExample", f"Workspace example package {package!r} has no ExampleRecord.", "Add exactly one record for its examples/**/Cargo.toml package.", package, workspace[package]["manifest_relative"]))
    for package in sorted(actual - expected):
        errors.append(finding("ManifestUnexpectedExample", f"ExampleRecord {package!r} is not present in Cargo metadata.", "Remove the stale record or add the package to the workspace explicitly.", package, seen[package]))


def validate_document_index(manifest: dict[str, Any], root: Path, components: dict[str, dict[str, Any]], errors: list[Finding]) -> None:
    docs = manifest.get("documentation")
    if not isinstance(docs, list) or any(not isinstance(doc, str) or not doc.strip() for doc in docs):
        errors.append(finding("DocumentationIndexMissing", "documentation must list the shared API and migration documents.", "Add docs/api-reference-v0.2.md and docs/migration-v0.2.md.", path="docs/component-api-example-migration.toml"))
        return
    for doc in docs:
        if not (root / doc).is_file():
            errors.append(finding("DocumentationMissing", f"Indexed documentation does not exist: {doc}", "Create the indexed document or remove the stale index entry.", path=doc))
    api_path = root / "docs/api-reference-v0.2.md"
    migration_path = root / "docs/migration-v0.2.md"
    if not api_path.is_file():
        return
    api = api_path.read_text(encoding="utf-8")
    migration = migration_path.read_text(encoding="utf-8") if migration_path.is_file() else ""
    for component_id, record in components.items():
        canonical = record.get("canonical_path", "")
        if canonical and canonical not in api:
            errors.append(finding("DocumentationCanonicalMissing", f"Canonical path {canonical!r} for component {component_id!r} is absent from API Reference.", "Add the exact canonical path to docs/api-reference-v0.2.md.", component_id, "docs/api-reference-v0.2.md"))
    if "component-api-example-migration.toml" not in migration or "api-reference-v0.2.md" not in migration:
        errors.append(finding("DocumentationIndexLinkMissing", "Migration Guide does not link the manifest and API Reference.", "Add links to both shared canonical indexes.", path="docs/migration-v0.2.md"))


def validate_manifest(manifest: dict[str, Any], metadata: dict[str, Any], root: Path) -> list[Finding]:
    errors: list[Finding] = []
    if manifest.get("schema_version") != SCHEMA_VERSION:
        errors.append(finding("ManifestSchemaVersion", f"Expected schema_version = {SCHEMA_VERSION}, got {manifest.get('schema_version')!r}.", "Update the manifest schema version or checker together.", path="docs/component-api-example-migration.toml"))
    for key, expected in (("allowed_classifications", CLASSIFICATIONS), ("allowed_platforms", PLATFORMS), ("allowed_workloads", WORKLOADS)):
        values = manifest.get(key)
        if not isinstance(values, list) or set(values) != expected:
            errors.append(finding("ManifestRegistryMismatch", f"{key} does not match the checker registry.", f"Declare exactly the supported {key} values.", path="docs/component-api-example-migration.toml"))
    denylist = manifest.get("denylist_symbols")
    if not isinstance(denylist, list) or set(denylist) != DENYLIST_SYMBOLS:
        errors.append(finding("DenylistRegistryMismatch", "denylist_symbols does not cover the required facade boundary symbols.", "List WidgetStore, ViewEngine, Engine layout/render, numeric action, global scheduling, plugin, and JSON View symbols exactly.", path="docs/component-api-example-migration.toml"))
    components = validate_components(manifest, root, errors)
    validate_examples(manifest, metadata, root, components, errors)
    validate_document_index(manifest, root, components, errors)
    return errors


def run(manifest_path: Path, root: Path) -> dict[str, Any]:
    manifest, manifest_errors = load_toml(manifest_path)
    if manifest is None:
        return {"schema_version": SCHEMA_VERSION, "ok": False, "errors": [error.json() for error in manifest_errors], "checked": str(manifest_path)}
    metadata, metadata_errors = cargo_metadata(root)
    errors = list(manifest_errors) + list(metadata_errors)
    if metadata is not None:
        errors.extend(validate_manifest(manifest, metadata, root))
    return {
        "schema_version": SCHEMA_VERSION,
        "ok": not errors,
        "errors": [error.json() for error in errors],
        "checked": str(manifest_path),
        "workspace_metadata": "cargo metadata --format-version 1 --no-deps",
        "runtime_workloads_executed": [],
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Check migration manifest coverage and documentation indexes without running workloads.")
    parser.add_argument("--check", action="store_true", help="validate the manifest; this is the default")
    parser.add_argument("--json", action="store_true", help="emit machine-readable JSON (always emitted for stable automation)")
    parser.add_argument("--manifest", default="docs/component-api-example-migration.toml", help="manifest path relative to workspace root")
    parser.add_argument("--workspace-root", default=None, help="workspace root; defaults to the repository containing this script")
    args = parser.parse_args(argv)
    root = Path(args.workspace_root).resolve() if args.workspace_root else repo_root_from(Path(__file__))
    manifest_path = Path(args.manifest)
    if not manifest_path.is_absolute():
        manifest_path = root / manifest_path
    result = run(manifest_path, root)
    print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
    return 0 if result["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())

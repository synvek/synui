#!/usr/bin/env python3
"""Selectable, independently attributed SynUI v0.2 validation workloads.

The matrix is deliberately a runner, not an implementation of ABI, Python SDK,
or Engine behavior.  It records the boundary and evidence owner for each
command so a failed example cannot make an unrelated workload fail.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import shlex
from dataclasses import dataclass
from pathlib import Path
import subprocess
import sys
import time
from typing import Callable, Iterable

ROOT = Path(__file__).resolve().parents[1]
MATRIX_SCHEMA_VERSION = 2
MIGRATION_WORKLOADS = (
    "facade-compilation", "component-interaction", "virtual-data",
    "rich-text-style", "headless-render", "a11y",
)
BOUNDARY_CATEGORIES = (
    "facade-only", "Engine-bound", "platform", "headless", "native ABI", "Python SDK",
)
BOUNDARY_HELP = (
    "facade-only: stable facade fixtures and migrated examples\n"
    "Engine-bound: retained Engine/data/bridge regression evidence\n"
    "platform: desktop/web/Android/iOS host and target checks\n"
    "headless: no system window; layout/paint and mounted/virtualized bounds\n"
    "native ABI: Production ABI C11/C++17 smoke/export checks\n"
    "Python SDK: ABI-only Python fixture checks"
)


@dataclass(frozen=True)
class Context:
    python: str
    abi_library: Path


@dataclass(frozen=True)
class WorkItem:
    """One independently attributable example or fixture invocation."""
    example_or_fixture: str
    commands: tuple[tuple[str, ...], ...]
    platform: str = "desktop"
    boundary: str | None = None
    metadata_present: bool | None = None
    bridge_available: bool | None = None


@dataclass(frozen=True)
class Workload:
    id: str
    description: str
    boundary: str
    commands: Callable[[Context], list[list[str]]]
    category: str = "other"
    items: Callable[[Context], list[WorkItem]] | None = None


def cargo(*args: str) -> list[str]:
    return ["cargo", *args]


def python_sdk(context: Context) -> list[list[str]]:
    return [[context.python, "sdk/python/fixtures/counter.py"],
            [context.python, "sdk/python/fixtures/virtual_data_table.py"]]


def facade_fixture_tests() -> list[list[str]]:
    return [
        cargo("test", "-p", "synui", "--test", "counter_prelude_fixture", "--", "--nocapture"),
        cargo("test", "-p", "synui", "--test", "external_component_fixture", "--", "--nocapture"),
    ]


def platform_checks() -> list[list[str]]:
    return [
        cargo("check", "-p", "mobile_hello"),
        cargo("check", "-p", "web_hello"),
        cargo("check", "-p", "mobile_hello", "--target", "aarch64-linux-android", "--features", "mobile"),
        cargo("check", "-p", "web_hello", "--target", "wasm32-unknown-unknown", "--features", "web"),
    ]


def _manifest_records() -> tuple[list[dict], list[dict]]:
    """Read the migration manifest without running any workload."""


def _manifest_records() -> tuple[list[dict], list[dict]]:
    """Read the migration manifest without running any workload."""
    try:
        from migration_manifest import load_toml  # type: ignore
        manifest, errors = load_toml(ROOT / "docs/component-api-example-migration.toml")
    except (ImportError, OSError, ValueError):
        return [], []
    if errors or not manifest:
        return [], []
    return list(manifest.get("examples", [])), list(manifest.get("fixtures", []))


def _command_for(record: dict, workload: str) -> tuple[tuple[str, ...], ...]:
    package = str(record.get("package", ""))
    commands = [str(command) for command in record.get("commands", [])]
    selected: list[str] = []
    for command in commands:
        if workload in {"facade-compilation", "component-interaction", "headless-render", "a11y"}:
            if f"cargo check -p {package}" in command and "--target" not in command:
                selected.append(command)
        elif workload == "virtual-data" and ("model::tests" in command or "external_component_fixture" in command):
            selected.append(command)
        elif workload == "rich-text-style" and any(token in command for token in ("block_doc", "style::tests", "scroll_tests", package)):
            selected.append(command)
    if not selected:
        selected = [f"cargo check -p {package}"] if package else []
    return tuple(tuple(shlex.split(command)) for command in selected)


def _a11y_bridge_commands() -> tuple[tuple[str, ...], ...]:
    """Use the existing semantic and virtual-grid tests as bridge evidence."""
    return (
        tuple(cargo("test", "-p", "synui-a11y", "semantic_props_map_state_actions_and_virtual_grid_metadata", "--", "--exact", "--nocapture")),
        tuple(cargo("test", "-p", "synui-testing", "--test", "virt_a11y_tests", "--", "--nocapture")),
    )


def _headless_commands() -> tuple[tuple[str, ...], ...]:
    """Use existing no-window and mounted-bounds fixtures; do not create a host."""
    return (
        tuple(cargo("test", "-p", "synui-testing", "--test", "integration", "headless_scene_builds", "--", "--exact", "--nocapture")),
        tuple(cargo("test", "-p", "synui-testing", "--test", "virt_a11y_tests", "--", "--nocapture")),
    )


MIGRATION_FIXTURE_REGISTRY = (
    {
        "id": "counter_prelude_fixture",
        "workloads": ("facade-compilation", "component-interaction"),
        "commands": (tuple(facade_fixture_tests()[0]),),
        "platform": "desktop",
        "boundary": "facade-only / Curated Rust Facade",
        "metadata_present": True,
    },
    {
        "id": "external_component_fixture",
        "workloads": ("facade-compilation", "component-interaction", "virtual-data"),
        "commands": (tuple(facade_fixture_tests()[1]),),
        "platform": "desktop",
        "boundary": "facade-only / external Element",
        "metadata_present": True,
    },
    {
        "id": "virtual_data_model_fixture",
        "workloads": ("virtual-data",),
        "commands": (tuple(cargo("test", "-p", "synui", "model::tests::hundred_thousand_rows_request_only_viewport_and_prefetch", "--", "--exact", "--nocapture")),),
        "platform": "desktop",
        "boundary": "facade-only / App-owned Model",
        "metadata_present": True,
    },
    {
        "id": "rich_text_style_regressions",
        "workloads": ("rich-text-style",),
        "commands": (
            tuple(cargo("test", "-p", "synui-core", "--lib", "block_doc")),
            tuple(cargo("test", "-p", "synui", "--lib", "style::tests")),
            tuple(cargo("test", "-p", "synui-testing", "--test", "scroll_tests")),
        ),
        "platform": "desktop",
        "boundary": "Engine-bound data/style regression owners",
        "metadata_present": None,
    },
    {
        "id": "headless_scene_fixture",
        "workloads": ("headless-render",),
        "commands": _headless_commands(),
        "platform": "desktop",
        "boundary": "headless / retained Engine test harness",
        "metadata_present": None,
    },
    {
        "id": "a11y_bridge_fixture",
        "workloads": ("a11y",),
        "commands": _a11y_bridge_commands(),
        "platform": "desktop",
        "boundary": "Engine-bound / a11y bridge",
        "metadata_present": True,
    },
)


def _commands_for_item(record: dict, workload: str) -> tuple[tuple[str, ...], ...]:
    commands = _command_for(record, workload)
    if workload == "headless-render":
        return commands + _headless_commands()
    if workload == "a11y":
        return commands + _a11y_bridge_commands()
    return commands


def migration_items(workload: str, _context: Context) -> list[WorkItem]:
    examples, fixtures = _manifest_records()
    items: list[WorkItem] = []
    for fixture in MIGRATION_FIXTURE_REGISTRY:
        if workload in fixture["workloads"]:
            items.append(WorkItem(
                fixture["id"], fixture["commands"],
                platform=fixture["platform"],
                boundary=fixture["boundary"],
                metadata_present=fixture["metadata_present"],
            ))
    for fixture in fixtures:
        if workload not in fixture.get("workloads", []):
            continue
        commands = tuple(shlex.split(str(command)) for command in fixture.get("commands", []))
        if workload == "headless-render":
            commands = _headless_commands()
        elif workload == "a11y" and fixture.get("components"):
            commands += _a11y_bridge_commands()
        items.append(WorkItem(
            str(fixture.get("id", fixture.get("path", "fixture"))), commands,
            boundary=str(fixture.get("boundary", "Curated Rust Facade")),
            metadata_present=bool(fixture.get("components")) if workload == "a11y" else None,
        ))
    for record in examples:
        if str(record.get("classification", "")) != "migrated":
            continue
        if workload not in record.get("validation_workloads", []):
            continue
        boundary = "Curated Rust Facade"
        metadata = bool(record.get("components")) if workload == "a11y" else None
        platforms = [str(platform) for platform in record.get("platforms", []) if str(platform)]
        platform = ",".join(platforms) if platforms else "desktop"
        items.append(WorkItem(
            str(record.get("package", "example")), _commands_for_item(record, workload),
            platform=platform,
            boundary=boundary, metadata_present=metadata,
        ))
    return items


def _migration_workload(workload_id: str, description: str, boundary: str, category: str) -> Workload:
    return Workload(workload_id, description, boundary,
                    lambda _: [], category=category,
                    items=lambda context: migration_items(workload_id, context))


WORKLOADS = (
    Workload("counter", "facade-only desktop Counter compile and curated-prelude fixture", "facade-only / Curated Rust Facade", lambda _: [cargo("check", "-p", "counter"), *facade_fixture_tests()], category="facade-only"),
    _migration_workload("facade-compilation", "facade-only compilation for stable and external Elements", "facade-only / Curated Rust Facade", "facade-only"),
    _migration_workload("component-interaction", "App state, typed input, BusinessKey, model, style, and interaction fixtures", "facade-only / Curated Rust Facade", "facade-only"),
    Workload("virtual-table", "bounded 100,000-row VirtualDataTable workload", "facade-only / Curated Rust Facade", lambda _: [cargo("test", "-p", "synui", "model::tests::hundred_thousand_rows_request_only_viewport_and_prefetch", "--", "--exact", "--nocapture")], category="facade-only"),
    _migration_workload("virtual-data", "bounded virtual data, selection, edit, sort, filter, and grid bounds", "facade-only plus explicit data adapter boundary", "facade-only"),
    Workload("rich-text", "retained BlockDoc Markdown roundtrip workload", "Engine-bound data support", lambda _: [cargo("test", "-p", "synui-core", "block_doc::tests::markdown_roundtrip_subset", "--", "--exact", "--nocapture")], category="Engine-bound"),
    _migration_workload("rich-text-style", "rich-text data, style diagnostics, and external scroll regression", "facade-only plus explicit external/retained regression owners", "Engine-bound"),
    Workload("platform", "declared desktop, web, Android, and iOS target checks", "platform/host boundary (not Prelude)", lambda _: platform_checks(), category="platform"),
    _migration_workload("headless-render", "no-window layout/paint, mounted bounds, and example assertions", "headless / retained Engine test harness", "headless"),
    Workload("native-abi", "C11/C++17 Production ABI smoke and export gate", "native ABI / Production ABI", lambda _: [cargo("test", "-p", "synui-abi", "--test", "native_abi", "--", "--nocapture")], category="native ABI"),
    Workload("python-sdk", "ABI-only Python Counter and 100,000-row table fixtures", "Python SDK / Production ABI", python_sdk, category="Python SDK"),
    _migration_workload("a11y", "role/name/value/state/action and virtual-grid accessibility semantics", "a11y / facade-to-bridge boundary", "Engine-bound"),
)


def default_abi_library() -> Path:
    if sys.platform == "win32":
        name = "synui_abi.dll"
    elif sys.platform == "darwin":
        name = "libsynui_abi.dylib"
    else:
        name = "libsynui_abi.so"
    return ROOT / "target" / "debug" / name


def parse_args() -> argparse.Namespace:
    categories = ", ".join(BOUNDARY_CATEGORIES)
    parser = argparse.ArgumentParser(
        description=__doc__,
        epilog="Boundary categories: " + "; ".join(
            f"{category}" for category in BOUNDARY_CATEGORIES
        ) + ". "
        + f"Migration workload IDs: {', '.join(MIGRATION_WORKLOADS)}.",
    )
    parser.add_argument("--only", help="comma-separated workload IDs; default is all")
    parser.add_argument("--list", action="store_true", help="list workload IDs, category, and boundary, then exit")
    parser.add_argument("--python", default=sys.executable, help="Python used for SDK fixtures")
    parser.add_argument("--abi-library", type=Path, default=default_abi_library())
    parser.add_argument("--report", type=Path, default=ROOT / "target/validation-matrix/component-api-example-migration.json")
    parser.add_argument("--boundary-help", action="store_true", help=f"show boundary categories: {categories}")
    return parser.parse_args()


def select_workloads(only: str | None) -> list[Workload]:
    by_id = {workload.id: workload for workload in WORKLOADS}
    if not only:
        return list(WORKLOADS)
    requested = [item.strip() for item in only.split(",") if item.strip()]
    unknown = sorted(set(requested) - set(by_id))
    if unknown:
        raise ValueError(f"unknown workload IDs: {', '.join(unknown)}")
    return [by_id[item] for item in requested]


UNAVAILABLE_MARKERS = (
    "target may not be installed", "can't find crate for `core`", "can't find crate for `std`", "linker `",
    "android ndk", "android-activity", "either \"game-activity\" or \"native-activity\" must be enabled",
    "android_sdk_root", "android_home", "could not find `clipboard`", "xcrun: error", "xcode-select",
    "a valid ios target", "wasm-bindgen command not found", "no such file or directory: 'cargo-apk'",
)

def is_unavailable_platform_output(output: str) -> bool:
    lowered = output.lower()
    return any(marker in lowered for marker in UNAVAILABLE_MARKERS)


def _run_command(command: Iterable[str], context: Context, workload_id: str) -> dict[str, object]:
    argv = list(command)
    environment = os.environ.copy()
    environment["SYNUI_ABI_LIBRARY"] = str(context.abi_library.resolve())
    try:
        completed = subprocess.run(argv, cwd=ROOT, env=environment, check=False, text=True, capture_output=True)
    except OSError as error:
        details = f"Unable to execute command: {error}"
        print(details, file=sys.stderr, flush=True)
        return {"argv": argv, "exit_code": None, "status": "failed", "details": details}
    output = "\n".join(part for part in (getattr(completed, "stdout", ""), getattr(completed, "stderr", "")) if part)
    if output:
        print(output, end="" if output.endswith("\n") else "\n", flush=True)
    status = "passed" if completed.returncode == 0 else "failed"
    if completed.returncode != 0 and (workload_id == "platform" or workload_id == "a11y") and is_unavailable_platform_output(output):
        status = "unavailable"
    return {"argv": argv, "exit_code": completed.returncode, "status": status, "details": output[-4000:] if output else ""}


def _a11y_status(item: WorkItem) -> tuple[str, str, list[dict[str, object]]]:
    # Unknown metadata is deliberately conservative: only explicit metadata can
    # activate bridge verification, so missing data can never become passed.
    if item.metadata_present is not True:
        return "not_applicable", "not_applicable", [{"name": "semantic_metadata", "status": "not_applicable", "details": "No role/name/value/state/action or virtual-grid metadata is declared."}]
    bridge = item.bridge_available
    if bridge is None:
        bridge = os.environ.get("SYNUI_A11Y_BRIDGE", "available").lower() not in {"0", "false", "unavailable", "missing"}
    if not bridge:
        return "unavailable", "unavailable", [{"name": "a11y_bridge", "status": "unavailable", "details": "Semantic metadata exists but the AccessKit/Engine bridge is unavailable."}]
    return "passed", "passed", [{"name": "semantic_metadata", "status": "passed", "details": "Declared semantics are checked by the selected bridge workload."}, {"name": "a11y_bridge", "status": "passed", "details": "Accessibility bridge activated."}]


def _headless_system_window_created(output: str) -> bool:
    normalized = output.lower().replace("-", "_")
    explicit = re.search(r"system[ _]window[ _]created\s*[:=]\s*(true|false)", normalized)
    if explicit:
        return explicit.group(1) == "true"
    # A fixture that mentions creation without the required assertion is also
    # evidence of a host window or an incomplete headless contract.
    return any(token in normalized for token in ("created native window", "system window created"))


def _headless_checks(item: WorkItem, command_results: list[dict[str, object]]) -> tuple[str, str, list[dict[str, object]]]:
    output = " ".join(str(result.get("details", "")) for result in command_results)
    created = _headless_system_window_created(output)
    command_ok = all(result["status"] == "passed" for result in command_results) if command_results else False
    checks = [
        {"name": "system_window_created", "status": "failed" if created else "passed", "value": created, "details": "Headless path asserts system_window_created=false and must not create a system window."},
        {"name": "layout_paint", "status": "passed" if command_ok else "failed", "details": "Layout and paint completion is owned by the headless fixture."},
        {"name": "mounted_virtualized_bounds", "status": "passed" if command_ok else "failed", "details": "Mounted/virtualized bounds are checked by the selected fixture."},
        {"name": "example_assertion", "status": "passed" if command_ok else "failed", "details": f"Example/fixture assertion for {item.example_or_fixture}."},
    ]
    status = "failed" if created or not command_ok else "passed"
    # A command that ran but reported a failed assertion still activated the
    # headless path; activation is intentionally independent from verification.
    return status, "passed" if command_results else "failed", checks


def _items_for(workload: Workload, context: Context) -> list[WorkItem]:
    if workload.items is not None:
        return workload.items(context)
    return [WorkItem(workload.id, tuple(tuple(command) for command in workload.commands(context)), boundary=workload.boundary)]


def run_workload(workload: Workload, context: Context) -> dict[str, object]:
    started = time.monotonic()
    item_results: list[dict[str, object]] = []
    print(f"\n=== [{workload.id}] {workload.description} [{workload.category}] ===", flush=True)
    for item in _items_for(workload, context):
        command_results: list[dict[str, object]] = []
        if workload.id != "a11y" or item.metadata_present is True:
            for command in item.commands:
                print("+", " ".join(command), flush=True)
                command_results.append(_run_command(command, context, workload.id))
        command_statuses = [str(result["status"]) for result in command_results]
        if workload.id == "a11y":
            status, activation, checks = _a11y_status(item)
            if status == "passed" and "failed" in command_statuses:
                status = "failed"
            elif status == "passed" and "unavailable" in command_statuses:
                status = "unavailable"
        elif workload.id == "headless-render":
            status, activation, checks = _headless_checks(item, command_results)
        else:
            status = "failed" if "failed" in command_statuses else ("unavailable" if "unavailable" in command_statuses else "passed")
            activation = "passed" if command_results else "not_applicable"
            checks = [{"name": "commands", "status": status, "details": "All commands for this item were evaluated independently."}]
        command_details = " ".join(str(result.get("details", "")) for result in command_results)
        check_details = " ".join(str(check.get("details", "")) for check in checks)
        item_results.append({
            "workload": workload.id,
            "example_or_fixture": item.example_or_fixture,
            "boundary": item.boundary or workload.boundary,
            "platform": item.platform,
            "status": status,
            "activation_status": activation,
            "checks": checks,
            "commands": command_results,
            "details": (command_details or check_details)[-4000:],
        })
    statuses = [str(item["status"]) for item in item_results]
    if not statuses:
        status = "not_applicable"
    else:
        status = "failed" if "failed" in statuses else ("unavailable" if "unavailable" in statuses else ("not_applicable" if all(value == "not_applicable" for value in statuses) else "passed"))
    duration = time.monotonic() - started
    print(f"--- [{workload.id}] {status} ({duration:.2f}s)", flush=True)
    return {
        "id": workload.id, "description": workload.description, "category": workload.category,
        "boundary": workload.boundary, "status": status, "duration_seconds": round(duration, 3),
        "commands": [command for item in item_results for command in item["commands"]],
        "validation_results": item_results,
    }


def main() -> int:
    args = parse_args()
    if getattr(args, "boundary_help", False):
        print(BOUNDARY_HELP)
        return 0
    if args.list:
        for workload in WORKLOADS:
            print(f"{workload.id:22} {workload.category:14} {workload.boundary:48} {workload.description}")
        return 0
    try:
        selected = select_workloads(args.only)
    except ValueError as error:
        print(error, file=sys.stderr)
        return 2
    context = Context(args.python, args.abi_library)
    results = [run_workload(workload, context) for workload in selected]
    validation_results = [item for result in results for item in result.get("validation_results", [])]
    report = {
        "schema_version": MATRIX_SCHEMA_VERSION,
        "platform": sys.platform,
        "selected": [workload.id for workload in selected],
        "boundaries": {workload.id: {"category": workload.category, "boundary": workload.boundary} for workload in selected},
        "results": results,
        "validation_results": validation_results,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
    print(f"\nValidation Matrix report: {args.report}")
    for result in results:
        print(f"{result['id']:22} {result['status']}")
    return 1 if any(result["status"] == "failed" for result in results) else 0


if __name__ == "__main__":
    raise SystemExit(main())

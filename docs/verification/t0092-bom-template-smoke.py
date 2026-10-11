"""Actual production CLI and two native HTTP agents; synthetic sourcing only."""
import argparse
import csv
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser()
for key in ["binary", "previous-binary", "project", "directory", "repo"]:
    parser.add_argument("--" + key, type=Path, required=True)
parser.add_argument("--endpoint", required=True)
args = parser.parse_args()
root = args.directory.resolve()
root.mkdir(parents=True, exist_ok=False)
env = os.environ.copy()
for key in ["DISPLAY", "WAYLAND_DISPLAY"]:
    env.pop(key, None)
report = {"passed": False, "checks": [], "endpoint": args.endpoint,
          "binary_sha256": hashlib.sha256(args.binary.read_bytes()).hexdigest(),
          "previous_binary_sha256": hashlib.sha256(args.previous_binary.read_bytes()).hexdigest(),
          "fixture_sha256": hashlib.sha256(args.project.read_bytes()).hexdigest(), "outputs": {}}

def check(name, value):
    assert value, name
    report["checks"].append(name)

def cli(parameters, binary=None):
    return subprocess.run([str((binary or args.binary).resolve()), *map(str, parameters)],
                          env=env, capture_output=True, text=True, timeout=60)

def rows(path):
    with path.open(encoding="utf-8", newline="") as file:
        return list(csv.reader(file))

def files(path):
    return {str(file.relative_to(path)): file.read_bytes() for file in path.rglob("*") if file.is_file()}

spec = importlib.util.spec_from_file_location("smoke", args.repo / "scripts/mcp-smoke.py")
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
a = module.McpClient(args.endpoint, "template-reader")
b = module.McpClient(args.endpoint, "template-peer")
try:
    source = args.project.resolve()
    source_bytes = source.read_bytes()
    state = a.call("get_session_state", {})
    design = state["project"]
    status = a.call("get_document_status", {})
    report["initial_cursor"] = state["cursor"]
    report["active_native_path"] = status["path"]
    tools = {tool["name"]: tool for tool in a.rpc("tools/list")["tools"]}
    check("two initialized agent sessions are distinct", a.session != b.session)
    check("shared discovery includes both typed BOM template kinds",
          {"bom", "placement", "sourcing_bom", "catalog_bom", "assembly_bom"}
          == set(tools["export_assembly_csv"]["inputSchema"]["properties"]["kind"]["enum"]))
    check("both agents observe identical complete session", b.call("get_session_state", {}) == state)
    for kind in ["catalog_bom", "assembly_bom"]:
        output = root / (kind + ".csv")
        result = cli(["--export-csv", kind, "--open", source, "--output", output])
        check(kind + " actual production no-display CLI succeeds", result.returncode == 0)
        http = root / ("http-" + kind + ".csv")
        result = a.call("export_assembly_csv", {"kind": kind, "path": str(http), "expected_cursor": state["cursor"]})
        check(kind + " actual HTTP bytes equal CLI and count populated subset",
              http.read_bytes() == output.read_bytes() and result["component_count"] == 1
              and result["exported_cursor"] == state["cursor"])
        parsed = rows(output)
        check(kind + " independent CSV parser sees one included row", len(parsed) == 2)
        if kind == "catalog_bom":
            check("catalog exact header bytes and explicit synthetic identifier",
                  parsed[0] == ["Comment", "Designator", "Footprint", "JLCPCB Part #（optional）"]
                  and parsed[1] == ['10k, "μΩ"', "U_TOP", "0603", "C0000000000000000"])
        else:
            check("assembly exact headers Unicode quoting and empty uninferred cells",
                  parsed[0] == ["Item #", "*Designator", "*Qty", "Manufacturer", "*Mfg Part #",
                                "Description / Value", "*Package/Footprint ", "Type", "Your Instructions / Notes"]
                  and parsed[1] == ["1", "U_TOP", "1", 'User μ, "Example"', 'USER-MPN, "μ"', '10k, "μΩ"', "0603", "", ""])
        before = output.read_bytes()
        b.call("export_assembly_csv", {"kind": kind, "path": str(output)}, expect_error=True)
        check(kind + " NEW collision preserves existing bytes", output.read_bytes() == before)
        output.write_bytes(b"explicit replacement only")
        b.call("export_assembly_csv", {"kind": kind, "path": str(output), "overwrite": True})
        check(kind + " explicit overwrite publishes exact complete bytes", output.read_bytes() == before)
        a.call("export_assembly_csv", {"kind": kind, "path": str(source), "overwrite": True}, expect_error=True)
        result = cli(["--export-csv", kind, "--open", source, "--output", source, "--overwrite"])
        check(kind + " CLI and HTTP protect active native bytes", result.returncode != 0 and source.read_bytes() == source_bytes)
        report["outputs"][kind] = hashlib.sha256(before).hexdigest()
    check("all accepted template exports preserve complete shared session/status",
          a.call("get_session_state", {}) == state and b.call("get_document_status", {}) == status)
    for kind in ["bom", "placement", "sourcing_bom"]:
        current, previous = root / (kind + ".csv"), root / ("prior-" + kind + ".csv")
        first = cli(["--export-csv", kind, "--open", source, "--output", current])
        second = cli(["--export-csv", kind, "--open", source, "--output", previous], args.previous_binary)
        check(kind + " previous verified format bytes unchanged", first.returncode == second.returncode == 0 and current.read_bytes() == previous.read_bytes())
    current, previous = root / "fabrication", root / "prior-fabrication"
    first = cli(["--export-gerber", "--open", source, "--output", current])
    second = cli(["--export-gerber", "--open", source, "--output", previous], args.previous_binary)
    check("seven prior fabrication files retain exact bytes", first.returncode == second.returncode == 0 and len(files(current)) == 7 and files(current) == files(previous))
    job = {"format": "viamind-output-job", "version": 1, "name": "Template inventory parity", "profile": "generic_outer_2layer"}
    job_path = root / "job.json"
    a.call("save_output_job", {"path": str(job_path), "job": job})
    for kind in ["catalog_bom", "assembly_bom"]:
        invalid_job = dict(job, profile=kind)
        a.call("save_output_job", {"path": str(root / (kind + ".job.json")), "job": invalid_job}, expect_error=True)
    current, previous = root / "release", root / "prior-release"
    first = cli(["--run-output-job", job_path, "--open", source, "--output", current])
    second = cli(["--run-output-job", job_path, "--open", source, "--output", previous], args.previous_binary)
    cf, pf = files(current), files(previous)
    check("ten existing output-job/config files unchanged and new profiles rejected",
          first.returncode == second.returncode == 0 and len(cf) == 11
          and {k: v for k, v in cf.items() if k != "release-manifest.json"}
          == {k: v for k, v in pf.items() if k != "release-manifest.json"}
          and not any((root / (kind + ".job.json")).exists() for kind in ["catalog_bom", "assembly_bom"]))
    included = next(part for part in design["board"]["parts"].values() if part["assembly"]["populate"])
    bad_path = root / "rejected.csv"
    for field, value in [("supplier", "Other supplier"), ("supplier_sku", "c123"), ("mpn", None)]:
        metadata = dict(included["assembly"])
        if value is None:
            metadata.pop(field)
        else:
            metadata[field] = value
        before = a.call("get_session_state", {})
        b.call("set_part_assembly", {"id": included["id"], "assembly": metadata, "expected": included, "expected_project": design["id"]})
        edited = a.call("get_session_state", {})
        a.call("export_assembly_csv", {"kind": "catalog_bom", "path": str(bad_path), "expected_cursor": before["cursor"]}, expect_error=True)
        kind = "assembly_bom" if field == "mpn" else "catalog_bom"
        a.call("export_assembly_csv", {"kind": kind, "path": str(bad_path)}, expect_error=True)
        check(field + " explicit invalid/absent sourcing and stale cursor fail atomically",
              a.call("get_session_state", {}) == edited and not bad_path.exists())
        b.call("undo", {})
        check(field + " undo restores exact project and clean native status",
              a.call("get_design", {}) == design and a.call("get_document_status", {}) == status)
    check("original native file bytes never changed", source.read_bytes() == source_bytes)
    check("no failed output or leftover temporary publication files",
          not bad_path.exists() and all(not path.name.startswith(".tmp") for path in root.iterdir()))
    report["final_cursor"] = a.call("get_session_state", {})["cursor"]
    report["passed"] = True
finally:
    a.close()
    b.close()
    (root / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))

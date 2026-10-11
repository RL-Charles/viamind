"""Actual production CLI/two HTTP agents, checked release gate and old-byte parity."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess

p = argparse.ArgumentParser()
for key in ["binary", "previous-binary", "project", "directory", "repo"]:
    p.add_argument("--" + key, type=Path, required=True)
p.add_argument("--endpoint", required=True)
a = p.parse_args()
root = a.directory.resolve()
root.mkdir(parents=True, exist_ok=False)
env = os.environ.copy()
for key in ["DISPLAY", "WAYLAND_DISPLAY"]:
    env.pop(key, None)
report = {"passed": False, "checks": [], "endpoint": a.endpoint,
          "binary_sha256": hashlib.sha256(a.binary.read_bytes()).hexdigest(),
          "previous_binary_sha256": hashlib.sha256(a.previous_binary.read_bytes()).hexdigest(),
          "fixture_sha256": hashlib.sha256(a.project.read_bytes()).hexdigest()}

def check(name, result):
    assert result, name
    report["checks"].append(name)

def cli(job, path, binary=None):
    return subprocess.run([str((binary or a.binary).resolve()), "--run-output-job", str(job),
                           "--open", str(a.project.resolve()), "--output", str(path)],
                          env=env, capture_output=True, text=True, timeout=60)

def files(path):
    return {str(p.relative_to(path)): p.read_bytes() for p in path.rglob("*") if p.is_file()}

spec = importlib.util.spec_from_file_location("smoke", a.repo / "scripts/mcp-smoke.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
reader = m.McpClient(a.endpoint, "checked-job-reader")
peer = m.McpClient(a.endpoint, "checked-job-peer")
try:
    before = reader.call("get_session_state", {})
    status = reader.call("get_document_status", {})
    active = Path(status["path"])
    native_bytes = active.read_bytes()
    check("two initialized clients observe identical complete session",
          reader.session != peer.session and peer.call("get_session_state", {}) == before)
    report["initial_cursor"] = before["cursor"]
    tools = {tool["name"]: tool for tool in reader.rpc("tools/list")["tools"]}
    check("shared save schema exposes both profiles and strict rules",
          tools["save_output_job"]["inputSchema"]["properties"]["job"]["properties"]["profile"]["enum"]
          == ["generic_outer_2layer", "checked_outer_2layer"])
    checks = {key: 0.0 for key in ["copper_clearance_mm", "edge_clearance_mm", "minimum_via_drill_mm",
                                 "minimum_annular_width_mm", "hole_clearance_mm", "hole_to_copper_clearance_mm"]}
    checks["max_violations"] = 1000
    job = {"format": "viamind-output-job", "version": 1, "name": "Synthetic checked proof μ",
           "profile": "checked_outer_2layer", "mask_expansion_mm": 0.0, "checks": checks}
    settings = root / "checked.json"
    reader.call("save_output_job", {"path": str(settings), "job": job})
    check("explicit numeric checked settings roundtrip", reader.call("load_output_job", {"path": str(settings)})["job"] == job)
    output = root / "cli-checked"
    result = cli(settings, output)
    check("production no-display checked CLI succeeds", result.returncode == 0)
    expected = files(output)
    check("checked CLI publishes twelve fixed files including validation",
          len(expected) == 12 and "validation/drc.json" in expected)
    validation = json.loads(expected["validation/drc.json"])
    manifest = json.loads(expected["release-manifest.json"])
    check("validation and release share project identity and complete source cursor",
          validation["exported_cursor"] == manifest["exported_cursor"] and validation["project_id"] == manifest["project_id"])
    check("scoped validation states exact rules and honest remaining exclusions",
          validation["requested_rules"] == checks and validation["report"]["violations"] == []
          and validation["report"]["coverage"]["checks_not_performed"]
          and not validation["report"]["truncated"] and validation["report"]["incomplete_reason"] is None)
    http = root / "http-checked"
    value = reader.call("run_output_job", {"job_path": str(settings), "path": str(http), "expected_cursor": before["cursor"]})
    actual = files(http)
    check("production HTTP count and complete cursor identify same native snapshot",
          value["file_count"] == 12 and value["exported_cursor"] == before["cursor"])
    check("all ten fixed format/config bytes equal CLI", {k: v for k, v in actual.items() if k not in ["validation/drc.json", "release-manifest.json"]}
          == {k: v for k, v in expected.items() if k not in ["validation/drc.json", "release-manifest.json"]})
    for name in ["validation/drc.json", "release-manifest.json"]:
        native = json.loads(actual[name])
        independent_cli = json.loads(expected[name])
        independent_cli["exported_cursor"] = before["cursor"]
        check(name + " parity after independent CLI cursor normalization", native == independent_cli)
    direct = reader.call("run_drc", checks)
    check("validation uses actual shared production DRC report", direct["source_cursor"] == before["cursor"]
          and direct["report"] == json.loads(actual["validation/drc.json"])["report"])
    second = root / "peer-checked"
    peer.call("run_output_job", {"job_path": str(settings), "path": str(second), "expected_cursor": before["cursor"]})
    check("two native agents produce all twelve identical bytes", files(second) == actual)
    native_manifest = json.loads(actual["release-manifest.json"])
    check("release inventory byte lengths include complete validation", len(native_manifest["files"]) == 11
          and all(len(actual[k]) == length for k, length in native_manifest["files"].items()))
    reader.call("run_output_job", {"job_path": str(settings), "path": str(http)}, expect_error=True)
    check("NEW collision preserves every completed release byte", files(http) == actual)
    refused = root / "refused"
    reader.call("run_output_job", {"job_path": str(settings), "path": str(refused), "report": {"pass": True}}, expect_error=True)
    check("external authorization report rejected without output", not refused.exists())
    strict = dict(job, checks=dict(checks, edge_clearance_mm=100.0))
    strict_path = root / "strict.json"
    reader.call("save_output_job", {"path": str(strict_path), "job": strict})
    error = reader.call("run_output_job", {"job_path": str(strict_path), "path": str(refused)}, expect_error=True)
    result = cli(strict_path, refused)
    check("actual DRC violations refuse HTTP and CLI before publication",
          "violations" in error and result.returncode != 0 and not refused.exists())
    limited_path = root / "limited.json"
    reader.call("save_output_job", {"path": str(limited_path), "job": dict(strict, checks=dict(strict["checks"], max_violations=1))})
    error = peer.call("run_output_job", {"job_path": str(limited_path), "path": str(refused)}, expect_error=True)
    check("actual truncated DRC refuses release", "incomplete" in error and not refused.exists())
    for label, invalid in [("missing-checks", {k: v for k, v in job.items() if k != "checks"}),
                           ("unexpected-check", dict(job, checks=dict(checks, pass_flag=True))),
                           ("generic-checks", dict(job, profile="generic_outer_2layer"))]:
        invalid_path = root / (label + ".json")
        reader.call("save_output_job", {"path": str(invalid_path), "job": invalid}, expect_error=True)
        check(label + " rejected without file", not invalid_path.exists())
    generic = {k: v for k, v in job.items() if k != "checks"}
    generic["profile"] = "generic_outer_2layer"
    generic_path = root / "generic.json"
    reader.call("save_output_job", {"path": str(generic_path), "job": generic})
    current, previous = root / "current-generic", root / "previous-generic"
    one, two = cli(generic_path, current), cli(generic_path, previous, a.previous_binary)
    cf, pf = files(current), files(previous)
    check("existing generic job ten format/config files retain prior verified bytes", one.returncode == two.returncode == 0
          and len(cf) == len(pf) == 11 and {k: v for k, v in cf.items() if k != "release-manifest.json"}
          == {k: v for k, v in pf.items() if k != "release-manifest.json"})
    check("generic canonical settings omit checks", "checks" not in json.loads(generic_path.read_bytes()))
    peer.call("run_output_job", {"job_path": str(settings), "path": str(active)}, expect_error=True)
    check("active native file unchanged", active.read_bytes() == native_bytes)
    check("accepted exports and refusals preserve entire native session and status",
          reader.call("get_session_state", {}) == before and reader.call("get_document_status", {}) == status)
    peer.call("add_via", {"x_mm": 1, "y_mm": 1, "diameter_mm": 0.6, "drill_mm": 0.3})
    error = reader.call("run_output_job", {"job_path": str(settings), "path": str(refused), "expected_cursor": before["cursor"]}, expect_error=True)
    check("two-client full cursor rejects changed snapshot before output", "document changed" in error and not refused.exists())
    peer.call("undo", {})
    final = reader.call("get_session_state", {})
    check("undo restores exact project and clean native status without pretending unchanged cursor",
          final["project"] == before["project"] and final["cursor"]["sequence"] == before["cursor"]["sequence"] + 2
          and reader.call("get_document_status", {}) == status and active.read_bytes() == native_bytes)
    check("ordinary failures leave no hidden staging directories", not any(path.name.startswith((".viamind-output-job-", ".viamind-job-")) for path in root.iterdir()))
    report["final_cursor"] = final["cursor"]
    report["output_sha256"] = {name: hashlib.sha256(data).hexdigest() for name, data in actual.items()}
    report["passed"] = True
finally:
    reader.close()
    peer.close()
    (root / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))

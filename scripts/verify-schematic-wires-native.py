"""Exercise an owned production headless process; writes only new temporary files."""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument("--binary", required=True)
parser.add_argument("--fixture", default=str(Path(__file__).resolve().parent.parent / "fixtures/schematic/regulator.viamind.json"))
parser.add_argument("--report", required=True)
args = parser.parse_args()
binary = Path(args.binary).resolve()
env = os.environ.copy()
env.pop("DISPLAY", None)
env.pop("WAYLAND_DISPLAY", None)
process = subprocess.Popen([str(binary), "--headless", "--open", str(Path(args.fixture).resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
sequence = 0
checks = []
report = {"passed": False, "checks": checks, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "transport": "production headless stdio"}


def rpc(method, params=None, notification=False):
    global sequence
    sequence += 1
    message = {"jsonrpc": "2.0", "method": method, "params": params or {}}
    if not notification:
        message["id"] = sequence
    process.stdin.write(json.dumps(message) + "\n")
    process.stdin.flush()
    if notification:
        return None
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        assert select.select([process.stdout], [], [], max(0, deadline - time.monotonic()))[0], method
        response = json.loads(process.stdout.readline())
        if response.get("id") == sequence:
            assert "error" not in response, response
            return response["result"]
    raise AssertionError(method)


def call(name, arguments=None, error=False):
    result = rpc("tools/call", {"name": name, "arguments": arguments or {}})
    assert bool(result.get("isError")) == error, result
    return result if error else result.get("structuredContent") or json.loads(result["content"][0]["text"])


def check(name, condition):
    assert condition, name
    checks.append(name)


try:
    rpc("initialize", {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "native-schematic-wires", "version": "1"}})
    rpc("notifications/initialized", notification=True)
    tools = {tool["name"] for tool in rpc("tools/list")["tools"]}
    check("wire command and bounded snapshot discovery", {"draw_schematic_wire_polyline", "snapshot_schematic_wire_sheet"} <= tools)
    project = call("get_design")
    sheet_id = call("edit_schematic_sheet", {"expected_project": project["id"], "sheet_id": None, "expected": None, "name": "Native wire proof", "edits": []})["created"][0]

    def command(vertices):
        result = call("snapshot_schematic_wire_sheet", {"sheet_id": sheet_id})
        result["vertices"] = [{"x": x, "y": y} for x, y in vertices]
        return result

    before = call("get_session_state")
    snapshot = call("snapshot_schematic_wire_sheet", {"sheet_id": sheet_id})
    check("snapshot is repeated deterministic and preserves full session", snapshot == call("snapshot_schematic_wire_sheet", {"sheet_id": sheet_id}) and before == call("get_session_state") and snapshot["source_cursor"] == before["cursor"])
    call("draw_schematic_wire_polyline", command([(1, 3), (7, 11)]), error=True)
    check("noncardinal path rejects without mutation", before == call("get_session_state"))
    stale = command([(1, 3), (7, 3), (7, 11)])
    call("add_part", {"designator": "R999", "value": "concurrent", "footprint": "x", "x_mm": 1, "y_mm": 1})
    before = call("get_session_state")
    call("draw_schematic_wire_polyline", stale, error=True)
    check("stale cursor preserves entire concurrent session", before == call("get_session_state"))
    call("draw_schematic_wire_polyline", command([(1, 3), (7, 3), (7, 11)]))
    after = call("get_session_state")
    normalized = copy.deepcopy(after["project"])
    normalized["schematic"]["sheets"][sheet_id] = before["project"]["schematic"]["sheets"][sheet_id]
    wires = after["project"]["schematic"]["sheets"][sheet_id]["content"]["wires"]
    check("two exact offgrid segments create one event and preserve unrelated project", len(wires) == 2 and after["cursor"]["sequence"] == before["cursor"]["sequence"] + 1 and normalized == before["project"] and any(w["start"] == {"x": 1, "y": 3} and w["end"] == {"x": 7, "y": 3} for w in wires.values()))
    call("undo")
    check("one undo removes complete path", call("get_design") == before["project"])
    call("redo")
    check("redo restores exact nested wire identities", call("get_design") == after["project"])

    def sheet_nets():
        return [net for net in call("query_schematic_connectivity")["nets"] if any(m["sheet_id"] == sheet_id for m in net["members"])]

    call("draw_schematic_wire_polyline", command([(4, -4), (4, 10)]))
    check("ordinary interior crossing is disconnected", len(sheet_nets()) == 2)
    call("draw_schematic_wire_polyline", command([(6, -4), (6, 3)]))
    check("T endpoint joins without implicit junction object", len(sheet_nets()) == 2 and len(call("get_design")["schematic"]["sheets"][sheet_id]["content"]["junctions"]) == 0)
    old = command([(20, 20), (30, 20)])
    directory = Path(tempfile.mkdtemp(prefix="viamind-t0074-native-"))
    path = directory / "wires.viamind.json"
    current = call("get_design")
    call("save_project", {"path": str(path)})
    check("native file retains exact wires", json.loads(path.read_text())["project"] == current)
    call("open_project", {"path": str(path), "expected_cursor": call("get_session_state")["cursor"]})
    reopened = call("get_session_state")
    check("same UUID reopen preserves exact project and sheet", reopened["project"]["id"] == old["expected_project"] and reopened["project"]["schematic"]["sheets"][sheet_id] == old["expected"])
    call("draw_schematic_wire_polyline", old, error=True)
    check("old stream alone rejects unchanged after reopen", call("get_session_state") == reopened)
    report["saved_path"] = str(path)
    process.stdin.close()
    check("clean production exit", process.wait(timeout=15) == 0)
    report["passed"] = True
finally:
    if process.poll() is None:
        process.terminate()
        process.wait(timeout=5)
    Path(args.report).write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))

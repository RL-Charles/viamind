"""Exercise an owned production headless process; writes only new temporary files."""
import sys
sys.dont_write_bytecode = True
import importlib.util
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
parser.add_argument("--fixture", default=str(Path(__file__).resolve().parent.parent / "fixtures/schematic/linked-regulator.viamind.json"))
parser.add_argument("--report", required=True)
parser.add_argument("--url", help="Owned actual desktop production MCP endpoint; opens fixture with current lifecycle guard")
args = parser.parse_args()
binary = Path(args.binary).resolve()
env = os.environ.copy()
env.pop("DISPLAY", None)
env.pop("WAYLAND_DISPLAY", None)
process = None
clients = []
if not args.url:
    process = subprocess.Popen([str(binary), "--headless", "--open", str(Path(args.fixture).resolve())], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
sequence = 0
checks = []
report = {"passed": False, "checks": checks, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "transport": args.url or "production headless stdio"}


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


def call(name, arguments=None, error=False, peer=False):
    result = rpc("tools/call", {"name": name, "arguments": arguments or {}})
    assert bool(result.get("isError")) == error, result
    return result if error else result.get("structuredContent") or json.loads(result["content"][0]["text"])


if args.url:
    spec = importlib.util.spec_from_file_location("mcp_smoke", Path(__file__).resolve().parent / "mcp-smoke.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    clients = [module.McpClient(args.url, "native-pin-pad-a"), module.McpClient(args.url, "native-pin-pad-b")]
    def rpc(method, params=None, notification=False):
        assert not notification
        return clients[0].rpc(method, params or {})
    def call(name, arguments=None, error=False, peer=False):
        return clients[int(peer)].call(name, arguments or {}, expect_error=error)
    call("open_project", {"path":str(Path(args.fixture).resolve()), "expected_cursor":call("get_session_state")["cursor"]})


def check(name, condition):
    assert condition, name
    checks.append(name)


try:
    if process:
        rpc("initialize", {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "native-pin-pad-probe", "version": "1"}})
        rpc("notifications/initialized", notification=True)
    tools = {tool["name"] for tool in rpc("tools/list")["tools"]}
    check("bounded pin pad query discovered", "query_linked_pin_pads" in tools)
    project = call("get_design")
    parts = sorted(project["board"]["parts"].values(), key=lambda p: p["id"])
    before = call("get_session_state")
    reports = [call("query_linked_pin_pads", {"component_id": p["id"]}) for p in parts]
    check("native regulator has seven exact physical terminal pairs", sum(r["total"] for r in reports) == 7 and all(r["coverage"] == "linked" and all(t["navigable"] for t in r["terminals"]) for r in reports))
    check("part and symbol queries produce identical paired identities", all(call("query_linked_pin_pads", {"component_id":p["schematic_symbol"]}) == r for p,r in zip(parts,reports)))
    check("repeat query deterministic and full session preserved", all(call("query_linked_pin_pads", {"component_id":p["id"]},peer=bool(args.url)) == r for p,r in zip(parts,reports)) and call("get_session_state") == before)
    part=parts[0]
    query={"component_id":part["id"]}
    page=call("query_linked_pin_pads",dict(query,limit=1))
    tail=call("query_linked_pin_pads",dict(query,offset=1))
    check("stable paged terminal order",page["terminals"]+tail["terminals"] == reports[0]["terminals"] and page["next_offset"] == 1)
    call("query_linked_pin_pads",dict(query,limit=0),error=True)
    call("query_linked_pin_pads",dict(query,unexpected=True),error=True)
    check("invalid query rejects without mutation",call("get_session_state")==before)
    call("update_part",{"id":part["id"],"designator":"RENAMED99","value":"peer"},peer=bool(args.url))
    renamed=call("query_linked_pin_pads",query)
    check("property rename keeps physical identities with fresh cursor",renamed["terminals"]==reports[0]["terminals"] and renamed["source_cursor"]!=reports[0]["source_cursor"])
    call("update_part",{"id":part["id"],"rotation_deg":45})
    unsupported=call("query_linked_pin_pads",query)
    check("arbitrary angle excluded without rounded integer centers",all(not t["navigable"] and t["board_position"] is None and t["unsupported_reasons"] for t in unsupported["terminals"]))
    call("undo")
    part=call("get_design")["board"]["parts"][part["id"]]
    pads=[{"id":p["id"],"number":p["number"],"x_mm":p["position"]["x"]/1e6,"y_mm":p["position"]["y"]/1e6,"width_mm":p["width"]/1e6,"height_mm":p["height"]/1e6,"net":p.get("net")} for p in part["pads"]]
    pads[0]["number"]="UNMATCHED"
    call("set_part_pads",{"id":part["id"],"expected":part,"pads":pads})
    mismatch=call("query_linked_pin_pads",query)
    check("mismatched terminal sets exclude all partial navigation",mismatch["coverage"]=="terminal_mismatch" and all(not t["navigable"] for t in mismatch["terminals"]))
    call("undo")
    part=call("get_design")["board"]["parts"][part["id"]]
    call("link_schematic_symbol",{"expected_project":project["id"],"expected_part":part,"target":{"kind":"unlink"}})
    unlinked=call("query_linked_pin_pads",query)
    check("explicit unlink removes all counterpart navigation",unlinked["coverage"]=="unlinked" and all(not t["navigable"] for t in unlinked["terminals"]))
    call("undo")
    current=call("query_linked_pin_pads",query)
    directory=Path(tempfile.mkdtemp(prefix="viamind-t0079-native-"))
    path=directory/"paired.viamind.json"
    call("save_project",{"path":str(path)})
    call("open_project",{"path":str(path),"expected_cursor":call("get_session_state")["cursor"]})
    reopened=call("query_linked_pin_pads",query)
    check("same UUID native reopen retains pair IDs and changes event stream",reopened["terminals"]==current["terminals"] and reopened["project_id"]==current["project_id"] and reopened["source_cursor"]["stream_id"]!=current["source_cursor"]["stream_id"])
    before=call("get_session_state")
    call("query_linked_pin_pads",query)
    check("reopened read preserves clean entire session",call("get_session_state")==before and not call("get_document_status")["dirty"])
    report["saved_path"]=str(path)
    if process:
        process.stdin.close()
        check("clean production exit", process.wait(timeout=15) == 0)
    report["passed"] = True
finally:
    if process and process.poll() is None:
        process.terminate()
        process.wait(timeout=5)
    for client in clients:
        client.close()
    Path(args.report).write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps(report, indent=2))

"""Verify an owned production read-only desktop and fresh stdio observer.

The running desktop must open the supplied scratch native document. This probe
never invokes a human edit; local human input has separate native evidence.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import sys

p = argparse.ArgumentParser(description=__doc__)
p.add_argument("--binary", type=Path, required=True)
p.add_argument("--project", type=Path, required=True)
p.add_argument("--repo", type=Path, required=True)
p.add_argument("--endpoint", required=True)
p.add_argument("--output", type=Path, required=True)
a = p.parse_args()
a.output.mkdir(parents=True, exist_ok=False)
sys.path.insert(0, str(a.repo / "scripts"))
from mcp_reference import StdioClient, digest  # noqa: E402

spec = importlib.util.spec_from_file_location("smoke", a.repo / "scripts/mcp-smoke.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class HttpObserver(m.McpClient):
    def rpc(self, method, params=None, notification=False):
        value = super().rpc(method, params, notification)
        if method == "initialize":
            self.initialization = value
        return value


report = {"format": "viamind-read-only-production-proof", "version": 1,
          "passed": False, "checks": [], "endpoint": a.endpoint,
          "binary_sha256": digest(a.binary), "native_sha256": digest(a.project),
          "scope": "Owned production desktop/two independent HTTP agents and fresh stdio; no OS input, human-input or frame-cadence claim."}
native = a.project.read_bytes()


def check(name, condition):
    assert condition, name
    report["checks"].append(name)


def definitions(client):
    result = client.rpc("tools/list", {})
    assert "nextCursor" not in result, "unexpected production catalog pagination"
    return {t["name"]: t for t in result["tools"]}


def verify(client, label, live):
    tools = definitions(client)
    initial = client.call("get_session_state", {})
    status = client.call("get_document_status", {})
    hidden = sorted(live.keys() - tools.keys())
    check(label + " hides every unaudited writer", len(hidden) > 20 and
          {"add_part", "undo", "redo", "save_project", "configure_auto_drc", "set_selection",
           "save_output_job", "save_symbol_template", "open_project"} <= set(hidden))
    refused = a.output / (label + "-forbidden")
    for name in hidden:
        try:
            client.rpc("tools/call", {"name": name, "arguments": {
                "path": str(refused), "readOnlyHint": True, "mode": "live",
                "clientInfo": {"name": "human-admin"}}})
        except (ValueError, RuntimeError) as error:
            # Unknown tools produce invalid-params at the protocol boundary.
            assert "MCP protocol error during tools/call" in str(error) or "-32602" in str(error), str(error)
        else:
            raise AssertionError("hidden writer unexpectedly dispatched: " + name)
    check(label + " all hidden direct calls reject forged identity/hints", True)
    check(label + " all hidden calls preserve complete session/native/output",
          client.call("get_session_state", {}) == initial and client.call("get_document_status", {}) == status
          and a.project.read_bytes() == native and not refused.exists())
    sheets = initial["project"]["schematic"]["sheets"]
    sheet = next(iter(sheets))
    views = ["render_board_view", "render_board_png", "render_schematic_view", "render_schematic_png"]
    for name in views:
        check(label + " " + name + " discovery removes file output",
              "path" not in tools[name]["inputSchema"]["properties"] and "file output" in tools[name]["description"])
        for value in [str(refused), None, False, {"nested": str(refused)}]:
            response = client.rpc("tools/call", {"name": name, "arguments": {"path": value, "sheet": sheet}})
            assert response.get("isError") is True, (name, response)
        args = {"expected_cursor": initial["cursor"], "options": {"width_px": 128, "height_px": 96}}
        if name.startswith("render_schematic"):
            args["sheet"] = sheet
        response = client.rpc("tools/call", {"name": name, "arguments": args})
        check(label + " " + name + " refuses supplied path and renders in memory",
              response.get("isError") is False and not refused.exists()
              and (not name.endswith("png") or any(c["type"] == "image" for c in response["content"])))
    drc = client.call("run_drc", {})
    check(label + " production DRC remains admitted", drc["source_cursor"] == initial["cursor"])
    if "query_named_net_rules" in live:
        policy = client.call("query_named_net_rules", {})
        check(label + " named-net observation admitted and replacement hidden",
              "query_named_net_rules" in tools and "set_named_net_rules" in hidden
              and policy["source_cursor"] == initial["cursor"]
              and policy["document_net_rules"] == initial["project"]["board"].get("net_rules", {}))
    client.call("query_parts", {"limit": 1})
    client.call("get_auto_drc_status", {})
    client.call("watch_events", {"after": initial["cursor"], "timeout_ms": 1})
    check(label + " observations preserve complete session/native/status",
          client.call("get_session_state", {}) == initial and client.call("get_document_status", {}) == status
          and a.project.read_bytes() == native)
    report[label] = {"admitted_tools": sorted(tools), "hidden_tools_refused": hidden,
                     "source_cursor": initial["cursor"]}
    return initial, tools


with StdioClient([str(a.binary.resolve()), "--headless"], a.output, timeout=90) as live:
    live.initialize()
    baseline = definitions(live)
    live.finish()
clients = []
try:
    for label in ["http-human-admin", "http-observer-two"]:
        client = HttpObserver(a.endpoint, label)
        clients.append(client)
        check(label + " initialization discloses mode", "read only" in client.initialization["instructions"])
        verify(client, label, baseline)
    check("two independent HTTP sessions share exact complete state",
          clients[0].session != clients[1].session
          and clients[0].call("get_session_state", {}) == clients[1].call("get_session_state", {}))
    with StdioClient([str(a.binary.resolve()), "--headless", "--agent-read-only", "--open", str(a.project.resolve())],
                     a.output, timeout=90) as client:
        info = client.rpc("initialize", {"protocolVersion": "2025-03-26", "capabilities": {},
                        "clientInfo": {"name": "human-admin mode=live", "version": "1"}})
        client.rpc("notifications/initialized", notification=True)
        check("stdio initialization discloses mode", "read only" in info["instructions"])
        _, tools = verify(client, "stdio", baseline)
        check("stdio and desktop admit identical tools/schemas", tools == definitions(clients[0]))
        client.finish()
    check("executable and protected native identity preserved",
          digest(a.binary) == report["binary_sha256"] and digest(a.project) == report["native_sha256"])
    report["passed"] = True
finally:
    for client in clients:
        client.close()
    with (a.output / "verification.json").open("x") as file:
        json.dump(report, file, indent=2)
        file.write("\n")
print("Passed", len(report["checks"]), "actual production read-only checks")

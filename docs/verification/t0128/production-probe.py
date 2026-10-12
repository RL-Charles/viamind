#!/usr/bin/env python3
"""Certified curved DXF outline workflow against an actual Viamind binary.

Modes: owned headless stdio process (default), restricted headless stdio
(--restricted, launched with --agent-read-only), or two initialized HTTP clients
against an already launched owned GUI endpoint (--endpoint).
"""
import argparse
import hashlib
import json
import shutil
import sys
import time
import importlib.util
from pathlib import Path

sys.dont_write_bytecode = True
parser = argparse.ArgumentParser(description=__doc__)
for field in ("binary", "repo", "directory"):
    parser.add_argument("--" + field, type=Path, required=True)
parser.add_argument("--endpoint")
parser.add_argument("--restricted", action="store_true")
parser.add_argument("--source", required=True)
args = parser.parse_args()
repo, binary, out = args.repo.resolve(), args.binary.resolve(), args.directory.resolve()
out.mkdir()
sys.path.insert(0, str(repo / "scripts"))
from mcp_reference import StdioClient  # noqa: E402

spec = importlib.util.spec_from_file_location("smoke", repo / "scripts/mcp-smoke.py")
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)

FIXTURE = repo / "crates/doc/tests/fixtures/outline-rounded.dxf"
STRAIGHT = repo / "crates/doc/tests/fixtures/outline-mm.dxf"
report = {
    "passed": False, "source": args.source, "binary": str(binary),
    "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    "endpoint": args.endpoint, "restricted": args.restricted, "checks": [], "latency_ms": {},
}
clients, headless = [], None


def check(label, condition=True):
    if not condition:
        raise AssertionError(label)
    report["checks"].append(label)


try:
    if args.endpoint:
        clients = [smoke.McpClient(args.endpoint, "T128 creator"), smoke.McpClient(args.endpoint, "T128 peer")]
    else:
        command = [str(binary), "--headless"] + (["--agent-read-only"] if args.restricted else [])
        headless = StdioClient(command, out, timeout=120)
        headless.initialize()
        clients = [headless]

    def call(name, arguments=None, peer=False, error=False):
        return clients[min(int(peer), len(clients) - 1)].call(name, arguments or {}, expect_error=error)

    def state(peer=False):
        return call("get_session_state", peer=peer)

    def status():
        return call("get_document_status")

    def reject(name, arguments, peer=False):
        before, st = state(), status()
        result = call(name, arguments, peer=peer, error=True)
        assert state() == before and status() == st, name
        return result

    def timed(label, name, arguments, peer=False):
        start = time.perf_counter()
        result = call(name, arguments, peer=peer)
        report["latency_ms"][label] = round((time.perf_counter() - start) * 1000, 3)
        return result

    def discover(client):
        tools, cursor = [], None
        for _ in range(64):
            page = client.rpc("tools/list", {} if cursor is None else {"cursor": cursor})
            tools.extend(page["tools"])
            cursor = page.get("nextCursor")
            if cursor is None:
                return {t["name"]: t for t in tools}
        raise AssertionError("catalog pagination bound")

    tools = discover(clients[0])
    if args.restricted:
        check("restricted catalog keeps preview and hides both outline writers",
              "preview_board_outline_dxf" in tools and not {"apply_board_outline_dxf", "import_board_outline_dxf"} & tools.keys())
    else:
        check("actual catalog advertises preview/apply plus unchanged straight-only import",
              {"preview_board_outline_dxf", "apply_board_outline_dxf", "import_board_outline_dxf"} <= tools.keys())
    check("preview schema requires explicit tolerance_nm", "tolerance_nm" in tools["preview_board_outline_dxf"]["inputSchema"]["required"])
    source = out / "rounded.dxf"
    shutil.copyfile(FIXTURE, source)
    by_path = {"path": str(source), "layer": "Edge", "tolerance_nm": 10000}
    by_text = {"text": FIXTURE.read_text(), "layer": "Edge", "tolerance_nm": 10000}

    if args.restricted:
        before, st = state(), status()
        preview = call("preview_board_outline_dxf", by_path)
        check("restricted mode keeps preview as an observation", preview["changes_outline"] and state() == before and status() == st)
        request = dict(by_path, expected_source_sha256=preview["source_sha256"], expected_cursor=preview["expected_cursor"], expected=preview["expected"])
        try:
            headless.call("apply_board_outline_dxf", request)
            denied = False
        except Exception as error:  # MCP protocol denial, not a tool-level result
            denied = "MCP protocol error during tools/call" in str(error)
        check("restricted mode refuses hidden Apply at the protocol before any work", denied)
        check("restricted refusal preserves full session and document status", state() == before and status() == st)
        headless.finish()
        headless = None
        report["passed"] = True
        raise SystemExit(0)

    if args.endpoint:
        check("two initialized HTTP clients discover identical tools", set(discover(clients[1])) == set(tools))
        check("owned fixture identity verified before HTTP mutation", state()["project"]["name"] == "T128 curved DXF owned inspection")
    else:
        call("new_project", {"name": "T128 curved DXF headless"})
    before, st = state(), status()
    path_preview = timed("preview_path", "preview_board_outline_dxf", by_path)
    text_preview = timed("preview_text", "preview_board_outline_dxf", by_text, peer=True)
    check("text and path previews bind identical bytes, polygon and report",
          path_preview["source_sha256"] == text_preview["source_sha256"] == hashlib.sha256(FIXTURE.read_bytes()).hexdigest()
          and path_preview["outline"] == text_preview["outline"] and path_preview["report"] == text_preview["report"])
    rep = path_preview["report"]
    check("report certifies every chord within the explicit tolerance", 0 < rep["achieved_bound_nm"] <= 10000
          and all(r["bound_nm"] <= 10000 for r in rep["rows"]) and rep["curved_segments"] == 7 and not rep["straight_only"])
    check("report discloses ignored TEXT, polygon-only native result and Y convention",
          rep["ignored_by_type"] == {"TEXT": 1} and any("polygon" in n for n in rep["notices"]) and any("negated" in n for n in rep["notices"]))
    check("rounded board has two cutouts and exact odd-Nm circle start",
          len(path_preview["outline"]["cutouts"]) == 2
          and {"x": 14100000, "y": -30000001} in path_preview["outline"]["cutouts"][0])
    check("previews preserve full session and document status", state() == before and status() == st)
    straight = call("preview_board_outline_dxf", {"text": STRAIGHT.read_text(), "tolerance_nm": 10})
    check("straight-only source through the new entry keeps legacy coordinates and zero sagitta",
          straight["report"]["straight_only"] and straight["report"]["achieved_bound_nm"] == 0
          and straight["outline"]["outer"][2] == {"x": 40000000, "y": -30000000})
    for label, bad in [
        ("tolerance below 10 Nm", dict(by_path, tolerance_nm=9)),
        ("both sources", dict(by_path, text="0\nEOF\n")),
        ("missing tolerance", {"path": str(source)}),
        ("bulge above one", {"text": FIXTURE.read_text().replace("42\n0.4142135623730950", "42\n1.5", 1), "layer": "Edge", "tolerance_nm": 10000}),
        ("unknown field", dict(by_path, weld=True)),
        ("directory instead of regular file", dict(by_path, path=str(out))),
    ]:
        reject("preview_board_outline_dxf", bad)
        check(label + " refuses without mutation")

    def apply(preview, base):
        return dict(base, expected_source_sha256=preview["source_sha256"], expected_cursor=preview["expected_cursor"], expected=preview["expected"])

    call("add_part", {"designator": "T128P", "value": "peer edit", "footprint": "X", "x_mm": 30, "y_mm": -20})
    reject("apply_board_outline_dxf", apply(path_preview, by_path), peer=True)
    check("unrelated edit after preview invalidates Apply by full cursor")
    fresh = call("preview_board_outline_dxf", by_path)
    source.write_bytes(FIXTURE.read_bytes().replace(b"40\n1.6", b"40\n1.5"))
    reject("apply_board_outline_dxf", apply(fresh, by_path))
    check("source rewrite after preview refuses by digest and reparse")
    shutil.copyfile(FIXTURE, source)
    guarded = state()
    applied = timed("apply_path", "apply_board_outline_dxf", apply(fresh, by_path))
    after = state()
    check("guarded Apply commits exactly one BoardOutline delta and one event",
          applied["changed"] and len(applied["result"]["deltas"]) == 1 and "board_outline" in json.dumps(applied["result"]["deltas"][0])
          and after["cursor"]["sequence"] == guarded["cursor"]["sequence"] + 1)
    check("applied outline equals preview polygon and other objects are preserved",
          after["project"]["board"]["outline"] == fresh["outline"] and after["project"]["board"]["parts"] == guarded["project"]["board"]["parts"])
    check("source file bytes are never written", source.read_bytes() == FIXTURE.read_bytes())
    if args.endpoint:
        check("peer reads the exact applied outline", call("get_design", peer=True)["board"]["outline"] == fresh["outline"])
        reject("apply_board_outline_dxf", apply(fresh, by_path), peer=True)
        check("peer cannot reuse the consumed preview")
        call("undo", peer=True, error=True)
        check("peer undo cannot reach creator history", state() == after)
    again = call("preview_board_outline_dxf", by_text)
    noop = call("apply_board_outline_dxf", apply(again, by_text))
    check("identical geometry from a fresh preview is a guarded no-op", not again["changes_outline"] and not noop["changed"] and state() == after)
    native = out / "curved.viamind.json"
    call("save_project", {"path": str(native)})
    saved = json.loads(native.read_bytes())
    check("native save stores the polygon outline in the existing schema", saved["project"]["board"]["outline"] == fresh["outline"])
    call("undo")
    check("creator undo restores the previous outline exactly", state()["project"]["board"].get("outline") == guarded["project"]["board"].get("outline"))
    call("redo")
    check("redo restores the applied polygon", state()["project"]["board"]["outline"] == fresh["outline"])
    drc = call("run_drc", {})
    check("existing DRC consumes the converted polygon", "report" in drc)
    report["final_cursor"] = state()["cursor"]
    report["passed"] = True
finally:
    if headless is not None:
        try:
            headless.finish()
        except Exception as error:  # retained in the receipt
            report["finish_error"] = str(error)
    for client in clients:
        if hasattr(client, "close"):
            client.close()
    (out / "production.json").write_text(json.dumps(report, indent=1, sort_keys=True) + "\n")
    print(json.dumps({"passed": report["passed"], "checks": len(report["checks"]), "receipt": str(out / "production.json")}))

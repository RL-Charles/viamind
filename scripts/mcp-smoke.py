#!/usr/bin/env python3
"""Exercise a running Viamind desktop as two independent MCP agent sessions."""
import argparse
import json
import sys
import time
import urllib.error
import urllib.request
import uuid
from pathlib import Path


class McpClient:
    def __init__(self, url, name):
        self.url = url
        self.session = None
        self.sequence = 0
        info = self.rpc("initialize", {
            "protocolVersion": "2025-03-26", "capabilities": {},
            "clientInfo": {"name": name, "version": "1.0"},
        })
        self.protocol = info["protocolVersion"]
        self.rpc("notifications/initialized", notification=True)

    def rpc(self, method, params=None, notification=False):
        self.sequence += 1
        payload = {"jsonrpc": "2.0", "method": method}
        if not notification:
            payload["id"] = self.sequence
        if params is not None:
            payload["params"] = params
        headers = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}
        if self.session:
            headers["Mcp-Session-Id"] = self.session
        if hasattr(self, "protocol"):
            headers["MCP-Protocol-Version"] = self.protocol
        request = urllib.request.Request(self.url, data=json.dumps(payload).encode(), headers=headers)
        with urllib.request.urlopen(request, timeout=15) as response:
            self.session = response.headers.get("Mcp-Session-Id", self.session)
            if notification:
                return None
            if response.headers.get_content_type() == "text/event-stream":
                # Read until this request's response, rather than waiting for a persistent SSE
                # stream to close. Ignore heartbeat/comment frames and unrelated notifications.
                data = []
                for raw_line in response:
                    line = raw_line.decode().rstrip("\r\n")
                    if line.startswith("data:"):
                        data.append(line[5:].lstrip())
                    elif not line and data:
                        event_data = "\n".join(data)
                        data = []
                        if not event_data.strip():
                            continue
                        message = json.loads(event_data)
                        if message.get("id") == self.sequence:
                            break
                else:
                    raise RuntimeError("SSE stream ended without the JSON-RPC response")
            else:
                message = json.load(response)
        if "error" in message:
            raise RuntimeError(f"{method}: {message['error']}")
        if message.get("id") != self.sequence:
            raise RuntimeError("JSON-RPC response ID does not match the request")
        return message["result"]

    def call(self, tool, arguments, expect_error=False):
        result = self.rpc("tools/call", {"name": tool, "arguments": arguments})
        failed = bool(result.get("isError"))
        if failed != expect_error:
            raise RuntimeError(f"{tool}: expected error={expect_error}, got {result}")
        if failed:
            return "\n".join(item.get("text", "") for item in result.get("content", []))
        if "structuredContent" in result:
            return result["structuredContent"]
        return json.loads(result["content"][0]["text"])

    def close(self):
        if self.session:
            request = urllib.request.Request(self.url, method="DELETE", headers={
                "Mcp-Session-Id": self.session, "MCP-Protocol-Version": self.protocol,
            })
            try:
                with urllib.request.urlopen(request, timeout=5):
                    pass
            except urllib.error.HTTPError as error:
                if error.code not in (404, 405):
                    raise


def verify(url, observe_seconds=0):
    report = {"url": url, "checks": [], "passed": False}
    a = b = None
    owned = {}
    baseline = selection = None
    failure = None

    def check(name, condition):
        if not condition:
            raise RuntimeError(f"check failed: {name}")
        report["checks"].append(name)

    try:
        a = McpClient(url, "viamind-smoke-a")
        b = McpClient(url, "viamind-smoke-b")
        baseline = a.call("get_design", {})
        selection = a.call("get_selection", {})
        tools = {tool["name"] for tool in a.rpc("tools/list", {}).get("tools", [])}
        check("tool discovery", {"add_part", "move_part", "delete_part", "undo", "redo", "get_design", "get_selection", "set_selection", "query_parts"} <= tools)
        token = uuid.uuid4().hex[:12]
        args = {"designator": f"R_SMOKE_A_{token}", "value": "10k", "footprint": "R_0603", "x_mm": 60, "y_mm": 45}
        first = a.call("add_part", args)["created"][0]
        owned[first] = a
        second = b.call("add_part", {**args, "designator": f"R_SMOKE_B_{token}", "x_mm": 66})["created"][0]
        owned[second] = b
        check("agent placement visible to other agent", first in b.call("get_design", {})["board"]["parts"])
        a.call("undo", {})
        parts = b.call("get_design", {})["board"]["parts"]
        check("agent undo preserves other agent edits", first not in parts and second in parts)
        a.call("redo", {})
        check("agent redo restores its edit", first in b.call("get_design", {})["board"]["parts"])
        a.call("move_part", {"id": first, "x_mm": 62.125, "y_mm": 47.5})
        check("move uses exact nanometres", b.call("get_design", {})["board"]["parts"][first]["position"] == {"x": 62125000, "y": 47500000})
        filters = {"ids": [first, second], "value": "10k", "text": "R_SMOKE", "side": "top", "limit": 1}
        page = b.call("query_parts", filters)
        next_page = b.call("query_parts", {**filters, "after": page["next_after"]})
        check("query filters and stable cursor pages", page["total"] == 2 and next_page["total"] == 2
              and [part["id"] for part in page["parts"] + next_page["parts"]] == sorted([first, second])
              and next_page["next_after"] is None)
        bounded = b.call("query_parts", {"ids": [first, second], "bounds": {
            "min_x_mm": 62.125, "min_y_mm": 47.5, "max_x_mm": 62.125, "max_y_mm": 47.5,
        }})
        check("query bounds include exact part origin", bounded["total"] == 1 and bounded["parts"][0]["id"] == first)
        before_query = a.call("get_design", {})
        before_selection = a.call("get_selection", {})
        b.call("query_parts", {"limit": 0}, expect_error=True)
        check("query validation changes no shared state", a.call("get_design", {}) == before_query
              and a.call("get_selection", {}) == before_selection)
        before_invalid = a.call("get_design", {})
        error = a.call("add_part", args, expect_error=True)
        check("duplicate designator gives actionable error", "already used" in error and a.call("get_design", {}) == before_invalid)
        a.call("move_part", {"id": first, "x_mm": 10001, "y_mm": 0}, expect_error=True)
        check("out-of-range move changes nothing", a.call("get_design", {}) == before_invalid)
        selected = a.call("set_selection", {"ids": [first, second]})
        check("selection is shared across agents", b.call("get_selection", {}) == selected)
        check("selection identifies the agent", selected["origin"]["kind"] == "agent" and selected["origin"]["id"].startswith("viamind-smoke-a#"))
        a.call("set_selection", {"ids": [str(uuid.uuid4())], "mode": "add"}, expect_error=True)
        check("invalid selection is atomic", a.call("get_selection", {}) == selected)
        a.call("move_part", {"id": first, "x_mm": 63, "y_mm": 48})
        b.call("move_part", {"id": first, "x_mm": 64, "y_mm": 49})
        a.call("undo", {}, expect_error=True)
        check("overlapping agent edits reject selective undo", a.call("get_design", {})["board"]["parts"][first]["position"] == {"x": 64000000, "y": 49000000})
        b.call("undo", {})
        a.call("undo", {})
        check("undo works after resolving the conflict", a.call("get_design", {})["board"]["parts"][first]["position"] == {"x": 62125000, "y": 47500000})
        if observe_seconds:
            print(f"Temporary test parts selected for {observe_seconds:g}s; inspect the desktop now.", file=sys.stderr, flush=True)
            time.sleep(observe_seconds)
        a.call("delete_part", {"id": first})
        check("delete prunes shared selection", first not in b.call("get_selection", {})["ids"])
        a.call("undo", {})
        check("delete is undoable", first in b.call("get_design", {})["board"]["parts"])
        a.call("redo", {})
        check("delete is redoable", first not in b.call("get_design", {})["board"]["parts"])
    except Exception as error:
        failure = str(error)
    finally:
        if a is not None and baseline is not None:
            try:
                current = a.call("get_design", {})["board"]["parts"]
                for part_id, client in owned.items():
                    if part_id in current:
                        client.call("delete_part", {"id": part_id})
                final = a.call("get_design", {})
                # Only restore the selection if it is still the smoke client's selection
                # (or empty after its deletions); preserve unrelated user/agent selection.
                current_selection = a.call("get_selection", {})
                origin = current_selection.get("origin") or {}
                if not current_selection["ids"] or origin.get("id", "").startswith("viamind-smoke-"):
                    valid_ids = [part_id for part_id in selection["ids"] if part_id in final["board"]["parts"]]
                    a.call("set_selection", {"ids": valid_ids})
                check("pre-test design restored exactly", final == baseline)
                check("pre-test selection restored", a.call("get_selection", {})["ids"] == selection["ids"])
            except Exception as error:
                failure = f"{failure or ''}; cleanup: {error}".lstrip("; ")
        for client in (a, b):
            if client is not None:
                try:
                    client.close()
                except Exception as error:
                    failure = f"{failure or ''}; close: {error}".lstrip("; ")
    if failure:
        report["error"] = failure
    else:
        report["passed"] = True
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:7878/mcp")
    parser.add_argument("--report", type=Path)
    parser.add_argument("--observe-seconds", type=float, default=0)
    args = parser.parse_args()
    if not 0 <= args.observe_seconds <= 60:
        parser.error("--observe-seconds must be between 0 and 60")
    report = verify(args.url, args.observe_seconds)
    output = json.dumps(report, indent=2) + "\n"
    if args.report:
        args.report.write_text(output)
    print(output, end="")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())

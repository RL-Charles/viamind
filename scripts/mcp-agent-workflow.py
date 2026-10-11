#!/usr/bin/env python3
"""Run an original guarded component workflow in a fresh owned headless process."""
import argparse
from copy import deepcopy
from pathlib import Path
import tempfile

from mcp_reference import StdioClient, digest, json_bytes, publish_files, read_regular


def workflow(binary, directory):
    binary = Path(binary).resolve(strict=True)
    before = digest(binary)
    checks = []

    def check(name, condition):
        if not condition:
            raise ValueError(f"workflow failed: {name}")
        checks.append(name)

    path = Path(directory).resolve() / "agent-board.viamind.json"
    with StdioClient([str(binary), "--headless"], directory) as client:
        client.initialize()
        catalog = client.discover()
        tools = {tool["name"]: tool for tool in catalog["tools"]}
        required = {"new_project": {"name"}, "add_part": {"designator", "value", "footprint", "x_mm", "y_mm"},
                    "update_part": {"id", "expected", "value", "x_mm"},
                    "open_project": {"path", "expected_cursor"}, "save_project": {"path"}}
        for name, fields in required.items():
            check(f"discovered {name} request fields", fields <= tools[name]["inputSchema"].get("properties", {}).keys())
        client.call("new_project", {"name": "Agent connector placement example"})
        state = client.call("get_session_state")
        stale_cursor = state["cursor"]
        project_id = state["project"]["id"]
        ids = []
        for designator, x in [("J1", 10.125), ("J2", 30.125)]:
            result = client.call("add_part", {"designator": designator, "value": "2-pin connector",
                                 "footprint": "EXAMPLE_2PIN", "x_mm": x, "y_mm": 15.25,
                                 "expected_project": project_id})
            ids.append(result["created"][0])
        state = client.call("get_session_state")
        parts = state["project"]["board"]["parts"]
        check("two original placements have exact Nm coordinates", len(parts) == 2
              and parts[ids[0]]["position"] == {"x": 10125000, "y": 15250000}
              and parts[ids[1]]["position"] == {"x": 30125000, "y": 15250000})
        original = deepcopy(parts[ids[0]])
        client.call("update_part", {"id": ids[0], "expected": original, "value": "Input", "x_mm": 11.125})
        after_edit = client.call("get_session_state")
        check("guarded edit preserves second component", after_edit["project"]["board"]["parts"][ids[1]] == parts[ids[1]])
        check("guarded edit exact new coordinate", after_edit["project"]["board"]["parts"][ids[0]]["position"]["x"] == 11125000)
        client.call("update_part", {"id": ids[0], "expected": original, "value": "Stale"}, expect_error=True)
        check("stale part rejection preserves full session", client.call("get_session_state") == after_edit)
        client.call("undo")
        check("agent undo restores exact original part", client.call("get_session_state")["project"] == state["project"])
        client.call("redo")
        final = client.call("get_session_state")
        check("redo restores exact guarded document", final["project"] == after_edit["project"])
        client.call("save_project", {"path": str(path)})
        saved = read_regular(path)
        status = client.call("get_document_status")
        check("NEW save sets clean active native path", status["path"] == str(path) and not status["dirty"])
        before_stale = client.call("get_session_state")
        client.call("open_project", {"path": str(path), "expected_cursor": stale_cursor}, expect_error=True)
        check("stale full-cursor lifecycle rejection preserves session", client.call("get_session_state") == before_stale)
        check("stale lifecycle preserves native bytes", read_regular(path) == saved)
        client.call("save_project", {"path": str(path)}, expect_error=True)
        check("NEW destination collision preserves file and session", read_regular(path) == saved
              and client.call("get_session_state") == before_stale)
        client.call("open_project", {"path": str(path), "expected_cursor": before_stale["cursor"]})
        reopened = client.call("get_session_state")
        check("reopen preserves exact document and IDs", reopened["project"] == final["project"])
        check("reopen resets stream", reopened["cursor"]["stream_id"] != before_stale["cursor"]["stream_id"])
        client.finish()
    check("executable identity unchanged", digest(binary) == before)
    report = {"format": "viamind-agent-workflow", "version": 1, "passed": True,
              "binary_sha256": before, "tool_count": len(catalog["tools"]), "checks": checks,
              "native_sha256": digest(path),
              "scope": "Original component placement/properties; legacy graphical footprints, no explicit electrical pads or manufacturing approval."}
    return {"agent-board.viamind.json": saved, "verification.json": json_bytes(report, 1024 * 1024)}, report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    parser.add_argument("--output", type=Path, help="optional NEW directory retaining native design and report")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="viamind-agent-workflow-") as directory:
        files, report = workflow(args.binary, directory)
        if args.output:
            publish_files(files, args.output)
    print(f"Passed {len(report['checks'])} isolated agent workflow checks")


if __name__ == "__main__":
    main()

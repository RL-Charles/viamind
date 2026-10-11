#!/usr/bin/env python3
"""Generate NEW full-application MCP docs, or check committed reference bytes."""
import argparse
from pathlib import Path

from mcp_reference import generate, json_bytes, publish_files, read_regular


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, required=True)
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument("--output", type=Path, help="NEW directory for both reference files")
    mode.add_argument("--check", type=Path, help="existing reference directory; read-only exact check")
    parser.add_argument("--report", type=Path, help="optional NEW directory containing verification.json")
    args = parser.parse_args()
    files, report = generate(args.binary)
    if args.check:
        for name, expected in files.items():
            if read_regular(args.check / name) != expected:
                raise ValueError(f"MCP reference drift: {name}; regenerate from current full application")
    else:
        publish_files(files, args.output)
    if args.report:
        publish_files({"verification.json": json_bytes(report, 1024 * 1024)}, args.report)
    print(f"Verified {report['tool_count']} full-application tools ({report['protocolVersion']})")


if __name__ == "__main__":
    main()

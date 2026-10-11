"""Read-only checks of the fixed-size production-app inspection wrapper."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("--repo", type=Path, required=True)
parser.add_argument("--endpoint", required=True)
parser.add_argument("--directory", type=Path, required=True)
parser.add_argument("--production-report", type=Path, required=True)
args = parser.parse_args()
args.directory.mkdir(parents=True, exist_ok=False)
spec = importlib.util.spec_from_file_location("smoke", args.repo / "scripts/mcp-smoke.py")
smoke = importlib.util.module_from_spec(spec)
spec.loader.exec_module(smoke)
client = smoke.McpClient(args.endpoint, "sourcing-wrapper-observer")
report = {"passed": False, "endpoint": args.endpoint, "checks": []}


def check(name, condition):
    assert condition, name
    report["checks"].append(name)


try:
    before = client.call("get_session_state", {})
    selected = before["selection"]["ids"][0]
    metadata = before["project"]["board"]["parts"][selected]["assembly"]
    check("selected component metadata matches explicit fixture", metadata == {
        "manufacturer": 'User μ, "Example"', "mpn": "USER-ENTERED-MPN",
        "supplier": "Example supplier", "supplier_sku": "USER-ENTERED-SKU", "populate": False,
    })
    path = (args.directory / "sourcing.csv").resolve()
    result = client.call("export_assembly_csv", {
        "kind": "sourcing_bom", "path": str(path), "expected_cursor": before["cursor"],
    })
    check("generic sourcing CSV exported at full captured cursor", result["exported_cursor"] == before["cursor"])
    check("inspection/export leaves full shared session unchanged", client.call("get_session_state", {}) == before)
    digest = hashlib.sha256(path.read_bytes()).hexdigest()
    production = json.loads(args.production_report.read_text())
    check("wrapper CSV equals independently checked production CLI/HTTP", production["passed"] and digest == production["outputs"]["sourcing_bom"])
    report.update({
        "kind": "fixed-size production ViamindApp wrapper with one inspection-only egui scroll; no native Cua input claimed",
        "source_cursor": before["cursor"], "status": client.call("get_document_status", {}),
        "selected": before["selection"], "metadata": metadata, "csv_sha256": digest, "passed": True,
    })
finally:
    client.close()
    (args.directory / "report.json").write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
print(json.dumps(report, indent=2, ensure_ascii=False))

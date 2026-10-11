set -euo pipefail
cd /home/charlesmowbray/Development/viamind-t-0124
export PYTHONDONTWRITEBYTECODE=1 TMPDIR=/home/charlesmowbray/.cache/viamind/tmp
python3 scripts/mcp-agent-workflow.py --binary /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/viamind-33981e0532b7 --output /home/charlesmowbray/.cache/viamind/native-artifacts/t0124/combined-workflow

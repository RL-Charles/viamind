"""Owned fresh stdio boundary probe; NEW report only, no active desktop mutation."""
import argparse
import hashlib
import json
from pathlib import Path
import sys
import tempfile
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'scripts'))
from mcp_reference import StdioClient
parser = argparse.ArgumentParser()
parser.add_argument('--binary', required=True)
parser.add_argument('--report', required=True)
args = parser.parse_args()
binary = Path(args.binary).resolve(strict=True)
checks = []
with tempfile.TemporaryDirectory(prefix='viamind-predecode-') as temporary:
    directory = Path(temporary)
    with StdioClient([str(binary), '--headless'], directory) as client:
        client.initialize()
        before = client.call('get_session_state')
        status = client.call('get_document_status')
        cases = [
            ('save_footprint_definition', {'path':str(directory/'never.json'), 'footprint':{'name':'Explicit','pads':[None]*1025}}, '1024 pads before typed decoding'),
            ('save_footprint_definition', {'path':str(directory/'never.json'), 'footprint':{'name':'a'*513,'pads':[None]}}, '512 UTF-8 bytes before typed decoding'),
            ('save_footprint_definition', {'path':str(directory/'never.json'), 'footprint':{'name':'Explicit','pads':[{'number':'a'*513}]}}, '512 UTF-8 bytes before typed decoding'),
            ('save_footprint_definition', {'path':'a'*4097}, '4096 UTF-8 bytes'),
            ('load_footprint_definition', {'path':'a'*4097}, '4096 UTF-8 bytes'),
        ]
        for command, arguments, expected in cases:
            result = client.call(command, arguments, expect_error=True)
            assert expected in json.dumps(result), result
            checks.append({'command':command, 'expected':expected, 'passed':True})
        assert client.call('get_session_state') == before
        assert client.call('get_document_status') == status
        assert not (directory/'never.json').exists()
        checks.append({'session_status_and_no_output_preserved':True})
with open(args.report, 'x') as stream:
    json.dump({'passed':True,'binary_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'checks':checks}, stream, indent=2)
print('Passed six real production predecode boundary checks')

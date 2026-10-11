"""Protocol, resource bounds, deterministic rendering and NEW publication tests."""
import json
import os
from pathlib import Path
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1]))
import mcp_reference as ref

PEER = r'''
import json,sys,time
mode=sys.argv[1]
names=['get_session_state','add_part','update_part','undo','save_project']
def send(value):
 print(json.dumps(value),flush=True)
for line in sys.stdin:
 request=json.loads(line)
 if 'id' not in request: continue
 ident=request['id']; method=request['method']
 if mode=='timeout': time.sleep(20)
 if mode=='stderr':
  sys.stderr.buffer.write(b'x'*4096);sys.stderr.flush()
 if mode=='oversize':
  sys.stdout.buffer.write(b'x'*4096);sys.stdout.flush();time.sleep(20)
 if mode=='badjson': print('not json',flush=True);continue
 if mode=='wrongid': ident+=1
 if mode=='rpcerror':
  send({'jsonrpc':'2.0','id':ident,'error':{'code':-1,'message':'failed'}});continue
 if mode=='notification':
  send({'jsonrpc':'2.0','method':'notifications/progress','params':{}})
 if mode=='notificationlimit':
  for _ in range(129): send({'jsonrpc':'2.0','method':'notifications/progress'})
 if method=='initialize':
  result={'protocolVersion':'2025-03-26','capabilities':{'tools':{}}}
  if mode=='badcapabilities': result['capabilities']=True
  if mode=='badprotocol': result['protocolVersion']='unknown'
 else:
  tools=[{'name':name,'inputSchema':{'type':'object','$defs':{'Thing':{'type':'string','description':'μ ` ``` <tag>'}}},
          'annotations':{'readOnlyHint':True},'futureMetadata':{'enabled':True}} for name in names]
  if mode=='skeleton': tools=tools[:1]
  if mode=='duplicate': tools+=tools[:1]
  if mode=='toomany': tools += [{'name':str(i),'inputSchema':{'type':'object'}} for i in range(513)]
  if mode=='arrayschema':
   for tool in tools: tool['inputSchema']['type']='array'
  if mode=='repeat': result={'tools':tools if 'cursor' not in request['params'] else [],'nextCursor':''}
  elif mode=='pagelimit': result={'tools':[],'nextCursor':str(ident)}
  elif mode=='cursorlimit': result={'tools':tools,'nextCursor':'x'*4097}
  elif mode=='pages' and 'cursor' not in request['params']: result={'tools':tools[:2],'nextCursor':''}
  elif mode=='pages':
   assert request['params']['cursor']==''
   result={'tools':tools[2:]}
  else: result={'tools':tools}
 send({'jsonrpc':'2.0','id':ident,'result':result})
if mode=='badexit': sys.exit(7)
'''


class ReferenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.peer = self.root / "peer.py"
        self.peer.write_text(PEER)

    def tearDown(self):
        self.temporary.cleanup()

    def client(self, mode="normal", **kwargs):
        return ref.StdioClient([sys.executable, str(self.peer), mode], self.root, **kwargs)

    def test_paginated_empty_cursor_preserves_complete_metadata_and_clean_exit(self):
        with self.client("pages") as client:
            client.initialize()
            catalog = client.discover()
            client.finish()
            self.assertEqual(client.process.returncode, 0)
        self.assertEqual(len(catalog["tools"]), 5)
        self.assertEqual(catalog["tools"][0]["futureMetadata"], {"enabled": True})
        self.assertIn("Thing", catalog["tools"][0]["inputSchema"]["$defs"])

    def test_duplicate_and_repeated_cursor_and_skeleton_reject(self):
        for mode, message in [("duplicate", "duplicate"), ("repeat", "repeated"), ("skeleton", "engine tools")]:
            with self.subTest(mode=mode), self.client(mode) as client:
                client.initialize()
                with self.assertRaisesRegex(ValueError, message):
                    client.discover()
            self.assertIsNotNone(client.process.poll())

    def test_wrong_identity_protocol_error_and_invalid_json_reject(self):
        for mode in ["wrongid", "rpcerror", "badjson", "badcapabilities", "badprotocol", "notificationlimit"]:
            with self.subTest(mode=mode), self.client(mode) as client:
                with self.assertRaises(ValueError):
                    client.initialize()
            self.assertIsNotNone(client.process.poll())

    def test_page_tool_cursor_total_byte_budgets_and_failed_exit(self):
        for mode, message in [("pagelimit", "pages"), ("toomany", "count"), ("cursorlimit", "cursor")]:
            with self.subTest(mode=mode), self.client(mode) as client:
                client.initialize()
                with self.assertRaisesRegex(ValueError, message):
                    client.discover()
        with patch.object(ref, "OUTPUT_LIMIT", 16), self.client() as client:
            with self.assertRaisesRegex(ValueError, "stdout"):
                client.initialize()
        with self.client("badexit") as client:
            client.initialize()
            client.discover()
            with self.assertRaisesRegex(ValueError, "cleanly"):
                client.finish()

    def test_timeout_and_unterminated_stdout_and_stderr_bounded(self):
        before = time.monotonic()
        with self.client("timeout", timeout=0.2, response_timeout=0.1) as client:
            with self.assertRaisesRegex(ValueError, "timed out|deadline"):
                client.initialize()
        self.assertLess(time.monotonic() - before, 3)
        for mode, setting, message in [("oversize", "RECORD_LIMIT", "record"), ("stderr", "STDERR_LIMIT", "stderr")]:
            with patch.object(ref, setting, 1024), self.client(mode) as client:
                with self.assertRaisesRegex(ValueError, message):
                    client.initialize()
            self.assertIsNotNone(client.process.poll())

    def test_notifications_drain_and_normal_finish(self):
        with self.client("notification") as client:
            client.initialize()
            client.discover()
            client.finish()
            self.assertEqual(client.notifications, 2)

    def test_buffered_record_and_expensive_decode_respect_deadline(self):
        with self.client() as client:
            record = b'{"jsonrpc":"2.0","id":1,"result":{}}\n'
            client.buffer.extend(record)
            with self.assertRaisesRegex(ValueError, "deadline"):
                client._record(time.monotonic() - 1)
            self.assertEqual(client.buffer, record)
            original = ref.strict_json

            def decode(raw):
                result = original(raw)
                client.deadline = time.monotonic() - 1
                return result

            with patch.object(ref, "strict_json", decode), self.assertRaisesRegex(ValueError, "deadline"):
                client._record(time.monotonic() + 1)

    def test_object_input_schema_required(self):
        with self.client("arrayschema") as client:
            client.initialize()
            with self.assertRaisesRegex(ValueError, "tool definition"):
                client.discover()

    def test_tool_error_status_requires_actual_boolean(self):
        with self.client() as client:
            for status in ["false", "true", 0, 1, None, {}]:
                with patch.object(client, "rpc", return_value={"isError": status}), self.assertRaisesRegex(ValueError, "boolean"):
                    client.call("update_part", expect_error=True)
            with patch.object(client, "rpc", return_value={"isError": True}):
                self.assertEqual(client.call("update_part", expect_error=True), {"isError": True})

    def test_strict_json_duplicate_depth_nonfinite_utf8(self):
        for raw in [b'{"a":1,"a":2}', b'NaN', b'Infinity', b'1e999', b'"\xff"', b'['*65+b'0'+b']'*65]:
            with self.subTest(raw=raw[:25]), self.assertRaises(ValueError):
                ref.strict_json(raw)
        self.assertEqual(ref.strict_json(b'{"quote":"[{\\\""}'), {"quote": '[{"'})

    def test_deterministic_complete_markdown_escaping_and_injective_anchors(self):
        catalog = {"tools": [{"name": "a.b", "inputSchema": {"$defs": {"z": {"description": "μ ```"}}}},
                             {"name": "a-b", "inputSchema": {}},
                             {"name": "<unsafe>[x]", "inputSchema": {}}]}
        files = ref.render(catalog)
        self.assertEqual(files, ref.render(json.loads(json.dumps(catalog))))
        self.assertEqual(json.loads(files["mcp-tools.json"]), catalog)
        text = files["mcp-tools.md"].decode()
        self.assertIn("````json", text)
        self.assertIn("tool-612e62", text)
        self.assertIn("tool-612d62", text)
        self.assertIn(r"\<unsafe\>\[x\]", text)
        with patch.object(ref, "OUTPUT_LIMIT", 256), self.assertRaises(ValueError):
            ref.render(catalog)
        with self.assertRaisesRegex(ValueError, "byte cap"):
            ref.json_bytes({"large": "x" * 100}, 16)

    def test_new_publication_existing_paths_symlink_race_preserve(self):
        destination = self.root / "reference"
        files = {"mcp-tools.json": b"{}\n", "mcp-tools.md": b"# tools\n"}
        ref.publish_files(files, destination)
        with self.assertRaises(FileExistsError):
            ref.publish_files({"mcp-tools.json": b"changed"}, destination)
        self.assertEqual((destination / "mcp-tools.json").read_bytes(), b"{}\n")
        link = self.root / "symlink"
        link.symlink_to(self.root / "missing")
        with self.assertRaises(FileExistsError):
            ref.publish_files(files, link)
        original = ref.publish_new

        def race(source, target):
            target.mkdir()
            (target / "sentinel").write_bytes(b"keep")
            original(source, target)

        raced = self.root / "raced"
        with patch.object(ref, "publish_new", race), self.assertRaises(FileExistsError):
            ref.publish_files(files, raced)
        self.assertEqual((raced / "sentinel").read_bytes(), b"keep")
        self.assertFalse(list(self.root.glob(".mcp-stage-*")))

    def test_check_regular_same_fd_limits_fifo_and_symlink(self):
        path = self.root / "check"
        path.write_bytes(b"exact")
        self.assertEqual(ref.read_regular(path, 5), b"exact")
        with self.assertRaises(ValueError):
            ref.read_regular(path, 4)
        path.unlink()
        os.mkfifo(path)
        before = time.monotonic()
        with self.assertRaises(ValueError):
            ref.read_regular(path)
        self.assertLess(time.monotonic() - before, 1)
        path.unlink()
        path.symlink_to(self.peer)
        with self.assertRaises(OSError):
            ref.read_regular(path)


if __name__ == "__main__":
    unittest.main()

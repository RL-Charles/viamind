"""Bounded Linux stdio discovery and deterministic full tool reference."""
import hashlib
import json
import math
import os
from pathlib import Path
import re
import selectors
import stat
import subprocess
import tempfile
import time

from appimage_support import publish_new

RECORD_LIMIT = 4 * 1024 * 1024
OUTPUT_LIMIT = 32 * 1024 * 1024
STDERR_LIMIT = 1024 * 1024
PROTOCOL = "2025-03-26"


def digest(path):
    with open(path, "rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def strict_json(raw):
    text = raw.decode("utf-8", errors="strict")
    depth = 0
    quoted = escaped = False
    for char in text:
        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
        elif char in "[{":
            depth += 1
            if depth > 64:
                raise ValueError("JSON nesting exceeds64")
        elif char in "]}":
            depth -= 1

    def pairs(values):
        result = {}
        for key, value in values:
            if key in result:
                raise ValueError("duplicate JSON key")
            result[key] = value
        return result

    def constant(value):
        raise ValueError(f"nonfinite JSON value: {value}")

    def number(value):
        result = float(value)
        if not math.isfinite(result):
            raise ValueError("nonfinite JSON number")
        return result

    return json.loads(text, object_pairs_hook=pairs, parse_constant=constant, parse_float=number)


class StdioClient:
    """One owned child; all pipes/deadlines are bounded, no shell or desktop attach."""
    def __init__(self, command, cwd, timeout=60, response_timeout=15):
        self.deadline = time.monotonic() + timeout
        self.response_timeout = response_timeout
        self.sequence = self.notifications = self.stdout_bytes = self.stderr_bytes = 0
        self.buffer = bytearray()
        self.selector = selectors.DefaultSelector()
        self.process = subprocess.Popen(command, cwd=cwd, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            os.set_blocking(stream.fileno(), False)
        self.selector.register(self.process.stdout, selectors.EVENT_READ, "stdout")
        self.selector.register(self.process.stderr, selectors.EVENT_READ, "stderr")
        self.protocol = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=2)
        self.selector.close()
        for stream in (self.process.stdin, self.process.stdout, self.process.stderr):
            stream.close()

    def _pump(self, deadline):
        remaining = min(deadline, self.deadline) - time.monotonic()
        if remaining <= 0:
            raise ValueError("MCP deadline exceeded")
        events = self.selector.select(remaining)
        if not events:
            raise ValueError("MCP response timed out")
        for key, _ in events:
            if key.data == "stdin":
                continue
            block = os.read(key.fd, 65536)
            if not block:
                self.selector.unregister(key.fileobj)
                continue
            if key.data == "stdout":
                self.stdout_bytes += len(block)
                if self.stdout_bytes > OUTPUT_LIMIT:
                    raise ValueError("MCP stdout exceeds32MiB")
                self.buffer.extend(block)
                # Every complete and partial record must fit, even before JSON parsing.
                if any(len(record) > RECORD_LIMIT for record in self.buffer.split(b"\n")):
                    raise ValueError("MCP record exceeds4MiB")
            elif key.data == "stderr":
                self.stderr_bytes += len(block)
                if self.stderr_bytes > STDERR_LIMIT:
                    raise ValueError("MCP stderr exceeds1MiB")

    def _send(self, message, deadline):
        if time.monotonic() >= min(deadline, self.deadline):
            raise ValueError("MCP deadline exceeded")
        raw = json.dumps(message, allow_nan=False, ensure_ascii=False).encode() + b"\n"
        if len(raw) > RECORD_LIMIT:
            raise ValueError("MCP request too large")
        self.selector.register(self.process.stdin, selectors.EVENT_WRITE, "stdin")
        try:
            offset = 0
            while offset < len(raw):
                if time.monotonic() >= min(deadline, self.deadline):
                    raise ValueError("MCP deadline exceeded")
                try:
                    offset += os.write(self.process.stdin.fileno(), raw[offset:])
                except BlockingIOError:
                    self._pump(deadline)
        finally:
            self.selector.unregister(self.process.stdin)

    def _record(self, deadline):
        if time.monotonic() >= min(deadline, self.deadline):
            raise ValueError("MCP deadline exceeded")
        while b"\n" not in self.buffer:
            if not any(key.data == "stdout" for key in self.selector.get_map().values()):
                raise ValueError("MCP stdout closed before response")
            self._pump(deadline)
        raw, _, rest = self.buffer.partition(b"\n")
        self.buffer = bytearray(rest)
        message = strict_json(raw)
        if time.monotonic() >= min(deadline, self.deadline):
            raise ValueError("MCP deadline exceeded")
        if not isinstance(message, dict) or message.get("jsonrpc") != "2.0":
            raise ValueError("invalid JSON-RPC envelope")
        return message

    def _notification(self, message):
        if ("id" in message or not isinstance(message.get("method"), str)
                or not message["method"].startswith("notifications/")
                or "result" in message or "error" in message):
            raise ValueError("unexpected JSON-RPC response or request")
        self.notifications += 1
        if self.notifications > 128:
            raise ValueError("MCP notifications exceed128")

    def rpc(self, method, params=None, notification=False):
        deadline = min(self.deadline, time.monotonic() + self.response_timeout)
        self.sequence += 1
        message = {"jsonrpc": "2.0", "method": method, "params": params or {}}
        if not notification:
            message["id"] = self.sequence
        self._send(message, deadline)
        if notification:
            return None
        while True:
            message = self._record(deadline)
            if "id" not in message:
                self._notification(message)
                continue
            if type(message["id"]) is not int or message["id"] != self.sequence or "method" in message:
                raise ValueError("unexpected JSON-RPC response identity")
            if "error" in message:
                raise ValueError(f"MCP protocol error during {method}")
            if not isinstance(message.get("result"), dict):
                raise ValueError("MCP result must be an object")
            return message["result"]

    def initialize(self):
        info = self.rpc("initialize", {"protocolVersion": PROTOCOL, "capabilities": {},
                                      "clientInfo": {"name": "viamind-reference", "version": "1"}})
        capabilities = info.get("capabilities")
        if (info.get("protocolVersion") != PROTOCOL or not isinstance(capabilities, dict)
                or not isinstance(capabilities.get("tools"), dict)):
            raise ValueError("unsupported negotiated protocol or missing tools capability")
        self.protocol = info["protocolVersion"]
        self.rpc("notifications/initialized", notification=True)

    def discover(self):
        tools, cursors, params = {}, set(), {}
        for _ in range(64):
            page = self.rpc("tools/list", params)
            if not isinstance(page.get("tools"), list):
                raise ValueError("tools/list missing tool array")
            for tool in page["tools"]:
                if not isinstance(tool, dict):
                    raise ValueError("tool definition must be an object")
                name = tool.get("name")
                if (not isinstance(name, str) or not name or len(name) > 128
                        or any(ord(char) < 32 or ord(char) == 127 for char in name)
                        or name in tools or not isinstance(tool.get("inputSchema"), dict)
                        or tool["inputSchema"].get("type") != "object"):
                    raise ValueError("invalid or duplicate tool definition")
                tools[name] = tool
                if len(tools) > 512:
                    raise ValueError("tool count exceeds512")
            if "nextCursor" not in page:
                if not {"get_session_state", "add_part", "update_part", "undo", "save_project"} <= tools.keys():
                    raise ValueError("full application engine tools missing")
                return {"format": "viamind-mcp-reference", "version": 1,
                        "protocolVersion": self.protocol, "tools": [tools[name] for name in sorted(tools)]}
            cursor = page["nextCursor"]
            if not isinstance(cursor, str) or len(cursor.encode()) > 4096 or cursor in cursors:
                raise ValueError("invalid or repeated continuation cursor")
            cursors.add(cursor)
            params = {"cursor": cursor}
        raise ValueError("tool pages exceed64")

    def call(self, name, arguments=None, expect_error=False):
        result = self.rpc("tools/call", {"name": name, "arguments": arguments or {}})
        status = result.get("isError", False)
        if type(status) is not bool:
            raise ValueError("MCP isError must be boolean")
        if status != expect_error:
            raise ValueError(f"unexpected tool status: {name}")
        if expect_error:
            return result
        if "structuredContent" in result:
            return result["structuredContent"]
        return strict_json(result["content"][0]["text"].encode())

    def finish(self):
        self.process.stdin.close()
        deadline = min(self.deadline, time.monotonic() + self.response_timeout)
        while self.selector.get_map() or self.buffer:
            while b"\n" in self.buffer:
                self._notification(self._record(deadline))
            if not self.selector.get_map():
                if self.buffer:
                    raise ValueError("unterminated final MCP record")
                break
            self._pump(deadline)
        remaining = min(deadline, self.deadline) - time.monotonic()
        if remaining <= 0 or self.process.wait(timeout=remaining) != 0:
            raise ValueError("MCP child did not exit cleanly")


def json_bytes(value, limit=OUTPUT_LIMIT):
    output = bytearray()
    encoder = json.JSONEncoder(indent=2, sort_keys=True, ensure_ascii=False, allow_nan=False)
    for chunk in encoder.iterencode(value):
        raw = chunk.encode("utf-8")
        if len(raw) > limit - len(output) - 1:
            raise ValueError("generated JSON exceeds byte cap")
        output.extend(raw)
    output.extend(b"\n")
    return bytes(output)


def render(catalog):
    reference = json_bytes(catalog)
    output = bytearray()

    def append(text):
        raw = text.encode()
        if len(raw) > OUTPUT_LIMIT - len(output):
            raise ValueError("generated Markdown exceeds32MiB")
        output.extend(raw)

    def anchor(name):
        return "tool-" + name.encode().hex()

    append("# Viamind MCP tool reference\n\nGenerated from the full application with "
           "`scripts/generate-mcp-reference.py`. Do not edit by hand.\n\n"
           "Every JSON block preserves the complete discovered tool definition. "
           "Request units and guards are defined by each schema and description.\n\n")
    for tool in catalog["tools"]:
        label = re.sub(r"([\\`*_{}\[\]()<>#!|])", r"\\\1", tool["name"])
        append(f"- [{label}](#{anchor(tool['name'])})\n")
    for tool in catalog["tools"]:
        definition = json_bytes(tool).decode()
        fence = "`" * max(3, 1 + max((len(run) for run in re.findall(r"`+", definition)), default=0))
        label = re.sub(r"([\\`*_{}\[\]()<>#!|])", r"\\\1", tool["name"])
        append(f'\n<a id="{anchor(tool["name"])}"></a>\n\n## {label}\n\n')
        append(fence + "json\n")
        append(definition)
        append(fence + "\n")
    return {"mcp-tools.json": reference, "mcp-tools.md": bytes(output)}


def read_regular(path, limit=OUTPUT_LIMIT):
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
            raise ValueError("reference check requires bounded regular files")
        raw = stream.read(limit + 1)
        if len(raw) > limit:
            raise ValueError("reference check file exceeds cap")
        return raw


def publish_files(files, destination):
    destination = Path(destination).absolute()
    if os.path.lexists(destination):
        raise FileExistsError(f"NEW output already exists: {destination}")
    with tempfile.TemporaryDirectory(dir=destination.parent, prefix=".mcp-stage-") as temporary:
        stage = Path(temporary) / "output"
        stage.mkdir()
        for name, raw in files.items():
            if Path(name).name != name or len(raw) > OUTPUT_LIMIT:
                raise ValueError("invalid bounded output file")
            with (stage / name).open("xb") as stream:
                stream.write(raw)
                stream.flush()
                os.fsync(stream.fileno())
        publish_new(stage, destination)


def generate(binary):
    binary = Path(binary).resolve(strict=True)
    before = digest(binary)
    with tempfile.TemporaryDirectory(prefix="viamind-mcp-discovery-") as cwd:
        with StdioClient([str(binary), "--headless"], cwd) as client:
            client.initialize()
            catalog = client.discover()
            client.finish()
    if digest(binary) != before:
        raise ValueError("application executable changed during discovery")
    files = render(catalog)
    report = {"binary_sha256": before, "protocolVersion": catalog["protocolVersion"],
              "tool_count": len(catalog["tools"]), "discovery_only": True,
              "sha256": {name: hashlib.sha256(raw).hexdigest() for name, raw in files.items()}}
    return files, report

#!/usr/bin/env python3
"""Build and publish a new Linux tarball directory; requires Python 3.11+."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import sys
import tarfile
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]


def run(*args, **kwargs):
    return subprocess.run(args, cwd=ROOT, check=True, text=True, **kwargs)


def capture(*args):
    return run(*args, stdout=subprocess.PIPE).stdout.strip()


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def verify_headless(binary, env):
    process = subprocess.Popen([str(binary), "--headless"], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env, text=True)
    sequence = 0
    def rpc(method, params=None, notification=False):
        nonlocal sequence
        sequence += 1
        message = {"jsonrpc": "2.0", "method": method, "params": params or {}}
        if not notification:
            message["id"] = sequence
        process.stdin.write(json.dumps(message) + "\n")
        process.stdin.flush()
        if notification:
            return None
        deadline = time.monotonic() + 15
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not select.select([process.stdout], [], [], max(0, remaining))[0]:
                raise RuntimeError(f"packaged MCP timed out: {method}")
            line = process.stdout.readline()
            if not line:
                raise RuntimeError(f"packaged MCP closed before replying: {method}")
            response = json.loads(line)
            if response.get("id") == sequence:
                if "error" in response:
                    raise RuntimeError(f"packaged MCP {method}: {response['error']}")
                return response["result"]
    def call(name, arguments=None):
        result = rpc("tools/call", {"name": name, "arguments": arguments or {}})
        if result.get("isError"):
            raise RuntimeError(f"packaged MCP tool failed: {name}")
        return result.get("structuredContent") or json.loads(result["content"][0]["text"])
    try:
        rpc("initialize", {"protocolVersion": "2025-03-26", "capabilities": {},
                           "clientInfo": {"name": "linux-bundle-check", "version": "1"}})
        rpc("notifications/initialized", notification=True)
        names = {tool["name"] for tool in rpc("tools/list")["tools"]}
        if not {"add_part", "query_parts", "undo"}.issubset(names) or call("query_parts")["total"] != 0:
            raise RuntimeError("packaged MCP discovery or fresh design failed")
        call("add_part", {"designator": "R1", "value": "10k", "footprint": "R_0603", "x_mm": 10, "y_mm": 20})
        parts = call("query_parts")
        if parts["total"] != 1 or parts["parts"][0]["designator"] != "R1":
            raise RuntimeError("packaged MCP edit/query round trip failed")
        call("undo")
        if call("query_parts")["total"] != 0:
            raise RuntimeError("packaged MCP undo round trip failed")
        process.stdin.close()
        if process.wait(timeout=15) != 0:
            raise RuntimeError("packaged MCP did not exit cleanly")
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
        process.stdout.close()
        process.stderr.close()
        if not process.stdin.closed:
            process.stdin.close()


def application_dependencies(metadata):
    packages = {p["id"]: p for p in metadata["packages"]}
    nodes = {n["id"]: n for n in metadata["resolve"]["nodes"]}
    app = next(p for p in packages.values() if p["name"] == "viamind" and p["source"] is None)
    pending, visited = [app["id"]], set()
    while pending:
        current = pending.pop()
        if current in visited:
            continue
        visited.add(current)
        for dep in nodes[current]["deps"]:
            if any(kind["kind"] != "dev" for kind in dep["dep_kinds"]):
                pending.append(dep["pkg"])
    return sorted((packages[p] for p in visited), key=lambda p: (p["name"], p["version"]))


def copy_licenses(packages, destination):
    fallback_root = ROOT / "packaging/licenses"
    fallbacks = json.loads((fallback_root / "sources.json").read_text())
    index = []
    for package in packages:
        if package["source"] is None:
            continue  # Workspace license and notices are copied separately.
        source = Path(package["manifest_path"]).parent
        name = f'{package["name"]}-{package["version"]}'
        files = set()
        for path in source.rglob("*"):
            legal_name = path.name.lower()
            if path.is_file() and (any(word in legal_name for word in ("license", "licence", "copying", "notice", "copyright"))
                                   or legal_name in ("ofl.txt", "ufl.txt")
                                   or ("fonts" in path.parts and path.suffix == ".txt")):
                files.add(path)
        if package.get("license_file"):
            files.add(source / package["license_file"])
        fallback = fallbacks.get(name, {})
        if fallback:
            vcs = json.loads((source / ".cargo_vcs_info.json").read_text())
            if vcs["git"]["sha1"] != fallback["revision"]:
                raise RuntimeError(f"Pinned license revision mismatch for {name}")
        if not files and not fallback:
            raise RuntimeError(f"No license text found for {name}; resolve before packaging")
        copied = []
        for path in sorted(files):
            relative = path.relative_to(source)
            target = destination / name / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
            copied.append(str(Path(name) / relative))
        for entry in fallback.get("files", []):
            path = fallback_root / entry["path"]
            if digest(path) != entry["sha256"]:
                raise RuntimeError(f"Pinned license checksum mismatch for {name}")
            target = destination / name / "upstream" / Path(entry["path"]).name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(path, target)
            copied.append(str(target.relative_to(destination)))
        index.append({"name": package["name"], "version": package["version"],
                      "license": package["license"], "repository": package["repository"], "files": copied,
                      "pinned_upstream_licenses": fallback or None})
    (destination / "index.json").write_text(json.dumps(index, indent=2) + "\n")
    return len(index)


def publish_new(source, destination):
    # Linux renameat2 publishes the complete directory and refuses every existing
    # destination, including empty directories and dangling symlinks.
    libc = ctypes.CDLL(None, use_errno=True)
    rename = libc.renameat2
    rename.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    rename.restype = ctypes.c_int
    if rename(-100, os.fsencode(source), -100, os.fsencode(destination), 1) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), str(destination))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new output directory (parent must exist)")
    parser.add_argument("--offline", action="store_true", help="use only cached Cargo dependencies")
    args = parser.parse_args()
    if not sys.platform.startswith("linux"):
        parser.error("this initial bundle targets Linux")
    output = args.output.absolute()
    if os.path.lexists(output):
        parser.error("output already exists; choose a new directory")
    if os.environ.get("CARGO_BUILD_TARGET"):
        parser.error("unset CARGO_BUILD_TARGET; this bundle validates a native host build")
    flags = ["--locked"] + (["--offline"] if args.offline else [])
    compiler = capture("rustc", "-vV")
    host = next(line.split(": ", 1)[1] for line in compiler.splitlines() if line.startswith("host: "))
    if "linux-gnu" not in host:
        parser.error("this initial bundle requires a native GNU/Linux Rust host")
    run("cargo", "build", "--release", "-p", "viamind", *flags)
    metadata = json.loads(capture("cargo", "metadata", "--format-version", "1", "--filter-platform", host, *flags))
    executable = Path(metadata["target_directory"]) / "release" / "viamind"
    env = dict(os.environ)
    for key in ("DISPLAY", "WAYLAND_DISPLAY"):
        env.pop(key, None)
    version = run(str(executable), "--version", env=env, stdout=subprocess.PIPE).stdout.strip()
    run(str(executable), "--help", env=env, stdout=subprocess.DEVNULL)
    packages = application_dependencies(metadata)
    app = next(p for p in packages if p["name"] == "viamind" and p["source"] is None)
    if version != f'viamind {app["version"]}':
        raise RuntimeError("executable version does not match the application manifest")
    revision = capture("git", "rev-parse", "HEAD")
    name = f'viamind-{app["version"]}-{revision[:12]}-{host}'
    with tempfile.TemporaryDirectory(prefix=".viamind-bundle-", dir=output.parent) as temporary:
        stage = Path(temporary)
        bundle = stage / name
        bundle.mkdir()
        shutil.copyfile(executable, bundle / "viamind")
        (bundle / "viamind").chmod(0o755)
        for filename in ("LICENSE", "NOTICE", "ATTRIBUTION.md"):
            shutil.copyfile(ROOT / filename, bundle / filename)
        shutil.copyfile(ROOT / "docs/linux-bundle.md", bundle / "README.md")
        (bundle / "viamind.desktop").write_text(
            "[Desktop Entry]\nType=Application\nName=Viamind\n"
            "Comment=PCB design and shared agent workspace\nExec=viamind\n"
            "Terminal=false\nCategories=Development;Electronics;\n")
        count = copy_licenses(packages, bundle / "licenses")
        dependencies = capture("ldd", str(executable))
        if "not found" in dependencies:
            raise RuntimeError("executable has missing host runtime libraries")
        status = run("git", "status", "--porcelain", "--untracked-files=normal", stdout=subprocess.PIPE).stdout
        dirty = any(line[3:] != ".sherpa/" and not line[3:].startswith(".sherpa/") for line in status.splitlines())
        info = {"version": app["version"], "revision": revision, "source_dirty": dirty,
                "host": host, "compiler": compiler, "glibc": capture("getconf", "GNU_LIBC_VERSION"),
                "binary_sha256": digest(bundle / "viamind"), "runtime_libraries": dependencies,
                "dependency_license_entries": count}
        verify_headless(bundle / "viamind", env)
        info["headless_mcp_verified"] = True
        (bundle / "build-info.json").write_text(json.dumps(info, indent=2) + "\n")
        (bundle / "SHA256SUMS").write_text(f'{info["binary_sha256"]}  viamind\n')
        published = stage / "output"
        published.mkdir()
        archive = published / f"{name}.tar.gz"
        with tarfile.open(archive, "w:gz") as tar:
            tar.add(bundle, arcname=name)
        # Read the completed archive back and exercise the packaged executable.
        extracted = stage / "verify"
        with tarfile.open(archive) as tar:
            tar.extractall(extracted, filter="data")
        binary = extracted / name / "viamind"
        if digest(binary) != info["binary_sha256"]:
            raise RuntimeError("packaged executable checksum mismatch")
        run(str(binary), "--version", env=env, stdout=subprocess.DEVNULL)
        verify_headless(binary, env)
        (published / "SHA256SUMS").write_text(f"{digest(archive)}  {archive.name}\n")
        publish_new(published, output)
    print(f"Linux bundle: {output} ({count} dependency license entries)")


if __name__ == "__main__":
    try:
        main()
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        sys.exit(str(error))

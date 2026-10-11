#!/usr/bin/env python3
"""Publish a verified x86_64 AppImage and exact runtime source/relink companion."""
import argparse
import csv
import importlib.util
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile

from appimage_support import ROOT, cached_input, digest, extract_regular, pins, publish_new


def linux_bundle_module():
    spec = importlib.util.spec_from_file_location("viamind_linux_bundle", ROOT / "scripts/package-linux.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def apprun_text():
    return ('#!/bin/sh\nset -eu\n'
            'appdir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\n'
            'exec "$appdir/usr/bin/viamind" "$@"\n')


def check_runtime(runtime):
    if runtime.is_symlink() or not runtime.is_dir():
        raise ValueError("runtime must be a regular directory")
    manifest_path = runtime / "manifest.json"
    if manifest_path.is_symlink() or manifest_path.stat().st_size > 1024 * 1024:
        raise ValueError("runtime manifest type/size mismatch")
    manifest = json.loads(manifest_path.read_text())
    if manifest["configuration"] != pins() or not manifest["relink_from_companion_only"]:
        raise ValueError("runtime provenance does not match pinned configuration")
    expected_files = manifest["files_sha256"]
    actual_files = set()
    total = 0
    for count, path in enumerate(runtime.rglob("*"), 1):
        if count > 50000 or path.is_symlink() or not (path.is_dir() or path.is_file()):
            raise ValueError("runtime companion contains links/special files or too many entries")
        if path.is_file():
            actual_files.add(str(path.relative_to(runtime)))
            total += path.stat().st_size
            if total > 256 * 1024 * 1024:
                raise ValueError("runtime companion exceeds256MiB")
    if actual_files != set(expected_files) | {"manifest.json"}:
        raise ValueError("runtime companion inventory mismatch")
    for relative, expected in expected_files.items():
        path = runtime / relative
        if not path.resolve().is_relative_to(runtime.resolve()) or digest(path) != expected:
            raise ValueError("runtime companion checksum/path mismatch: " + relative)
    if "runtime-x86_64" not in expected_files:
        raise ValueError("runtime binary missing")
    return manifest


def package(output, runtime, cache, bundle=None, offline=False):
    if sys.version_info < (3, 12) or sys.platform != "linux" or platform.machine() != "x86_64":
        raise ValueError("AppImage packaging requires Python3.12+ on x86_64 Linux")
    output, runtime = Path(output).absolute(), Path(runtime).absolute()
    if os.path.lexists(output):
        raise ValueError("output already exists; choose a new directory")
    check_runtime(runtime)
    tool = cached_input(pins()["tool"], cache, offline)
    tool.chmod(0o755)
    with tempfile.TemporaryDirectory(dir=output.parent, prefix=".appimage-build-") as temporary:
        stage = Path(temporary)
        # Copy links as links, then reject them from the task-owned snapshot.
        # All subsequent verification and packaging use these same immutable inputs.
        shutil.copytree(runtime, stage / "runtime-snapshot", symlinks=True)
        runtime = stage / "runtime-snapshot"
        runtime_manifest = check_runtime(runtime)
        if bundle is None:
            bundle = stage / "linux-bundle"
            subprocess.run([sys.executable, str(ROOT / "scripts/package-linux.py"), str(bundle)] + (["--offline"] if offline else []), check=True)
        bundle = Path(bundle).absolute()
        archives = list(bundle.glob("*.tar.gz"))
        if len(archives) != 1 or not archives[0].is_file() or archives[0].is_symlink() or archives[0].stat().st_size > 256 * 1024 * 1024:
            raise ValueError("expected one bounded Linux bundle archive")
        archive = archives[0]
        if (bundle / "SHA256SUMS").read_text() != f"{digest(archive)}  {archive.name}\n":
            raise ValueError("Linux bundle archive checksum mismatch")
        extract_regular(archive, stage / "bundle")
        roots = list((stage / "bundle").iterdir())
        if len(roots) != 1 or not roots[0].is_dir():
            raise ValueError("unexpected Linux bundle layout")
        payload = roots[0]
        info = json.loads((payload / "build-info.json").read_text())
        if (info["host"] != "x86_64-unknown-linux-gnu" or not info["headless_mcp_verified"]
                or digest(payload / "viamind") != info["binary_sha256"]):
            raise ValueError("Linux bundle host or application provenance mismatch")
        appdir = stage / "Viamind.AppDir"
        (appdir / "usr/bin").mkdir(parents=True)
        (appdir / "usr/lib").mkdir()
        shutil.copytree(payload, appdir / "usr/lib/viamind")
        shutil.copyfile(payload / "viamind", appdir / "usr/bin/viamind")
        (appdir / "usr/bin/viamind").chmod(0o755)
        (appdir / "AppRun").write_text(apprun_text())
        (appdir / "AppRun").chmod(0o755)
        (appdir / "viamind.desktop").write_text(
            "[Desktop Entry]\nType=Application\nName=Viamind\n"
            "Comment=PCB design and shared agent workspace\nExec=viamind %F\nIcon=viamind\n"
            "Terminal=false\nCategories=Development;Electronics;\n")
        for name in ["viamind.svg", ".DirIcon"]:
            shutil.copyfile(ROOT / "packaging/appimage/viamind.svg", appdir / name)
        shutil.copytree(runtime, appdir / "usr/share/viamind-runtime")
        published = stage / "output"
        published.mkdir()
        name = f"viamind-{info['version']}-{info['revision'][:12]}-x86_64"
        image = published / (name + ".AppImage")
        env = dict(os.environ, ARCH="x86_64", SOURCE_DATE_EPOCH="0")
        for key in ["DISPLAY", "WAYLAND_DISPLAY"]:
            env.pop(key, None)
        subprocess.run([str(tool), "--appimage-extract-and-run", "--no-appstream", "--runtime-file",
                        str(runtime / "runtime-x86_64"), "--mksquashfs-opt=-processors", "--mksquashfs-opt=2", str(appdir), str(image)], check=True, env=env)
        if image.stat().st_size > 256 * 1024 * 1024:
            raise ValueError("AppImage exceeds256MiB publication cap")
        with image.open("rb") as stream:
            header = stream.read(20)
        if header[:4] != b"\x7fELF" or header[8:11] != b"AI\x02" or int.from_bytes(header[18:20], "little") != 62:
            raise ValueError("expected an x86_64 Type-2 AppImage")
        image.chmod(0o755)
        extracted = stage / "image-readback"
        extracted.mkdir()
        subprocess.run([str(image), "--appimage-extract"], check=True, cwd=extracted, stdout=subprocess.DEVNULL, env=env)
        readback = extracted / "squashfs-root"
        for relative in ["usr/bin/viamind", "usr/lib/viamind/viamind"]:
            if digest(readback / relative) != info["binary_sha256"]:
                raise ValueError("image readback application checksum mismatch")
        for relative, expected in runtime_manifest["files_sha256"].items():
            if digest(readback / "usr/share/viamind-runtime" / relative) != expected:
                raise ValueError("image readback runtime companion mismatch")
        subprocess.run([str(readback / "usr/share/viamind-runtime/relink/relink.sh"), str(stage / "readback-runtime")], check=True)
        subprocess.run([str(stage / "readback-runtime"), "--appimage-version"], check=True, stdout=subprocess.DEVNULL)
        # These calls execute the image's runtime extraction route, not AppDir directly.
        invoke = [str(image), "--appimage-extract-and-run"]
        version = subprocess.check_output(invoke + ["--version"], text=True, env=env).strip()
        if version != "viamind " + info["version"]:
            raise ValueError("actual image version mismatch")
        subprocess.run(invoke + ["--help"], check=True, env=env, stdout=subprocess.DEVNULL)
        wrapper = stage / "actual-image.sh"
        wrapper.write_text("#!/bin/sh\nexec " + shlex.quote(str(image)) + ' --appimage-extract-and-run "$@"\n')
        wrapper.chmod(0o755)
        linux_bundle_module().verify_headless(wrapper, env)
        cwd = stage / "unrelated working directory with spaces"
        cwd.mkdir()
        shutil.copyfile(ROOT / "fixtures/fabrication/asymmetric.viamind.json", cwd / "input board.viamind.json")
        common = ["--export-csv", "bom", "--open", "input board.viamind.json", "--output"]
        subprocess.run(invoke + common + ["image output table.csv"], check=True, cwd=cwd, env=env)
        subprocess.run([str(payload / "viamind")] + common + ["direct output table.csv"], check=True, cwd=cwd, env=env)
        data = (cwd / "image output table.csv").read_bytes()
        if data != (cwd / "direct output table.csv").read_bytes():
            raise ValueError("relative image export differs from packaged application")
        rows = list(csv.reader(data.decode().splitlines()))
        if not any("U_TOP" in cell for row in rows for cell in row) or not any("U_BOTTOM" in cell for row in rows for cell in row):
            raise ValueError("relative project arguments did not load expected fixture")
        companion = published / (name + "-runtime-source-relink.tar.gz")
        with tarfile.open(companion, "w:gz") as target:
            target.add(runtime, arcname="viamind-runtime")
        extract_regular(companion, stage / "companion-readback")
        subprocess.run([str(stage / "companion-readback/viamind-runtime/relink/relink.sh"), str(stage / "companion-only-runtime")], check=True)
        subprocess.run([str(stage / "companion-only-runtime"), "--appimage-version"], check=True, stdout=subprocess.DEVNULL)
        metadata = {"application": info, "appimage_sha256": digest(image), "runtime_sha256": digest(runtime / "runtime-x86_64"),
                    "runtime_manifest_sha256": digest(runtime / "manifest.json"), "companion_sha256": digest(companion),
                    "tool": pins()["tool"], "actual_image_no_display_help_version_mcp_add_query_undo": True,
                    "relative_project_output_arguments_and_cwd": True, "embedded_and_external_companion_relink": True,
                    "runtime_environment_exceptions": ["APPIMAGE", "APPDIR", "documented extraction variables"],
                    "host_requirements": "Recorded application glibc/libraries plus system GPU and Wayland/X11 services; no universal portability claim"}
        manifest = published / "appimage-info.json"
        manifest.write_text(json.dumps(metadata, indent=2) + "\n")
        (published / "SHA256SUMS").write_text("".join(f"{digest(p)}  {p.name}\n" for p in [image, companion, manifest]))
        publish_new(published, output)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--runtime", required=True, type=Path, help="verified build-appimage-runtime.py output")
    parser.add_argument("--bundle", type=Path, help="reuse an explicitly supplied verified Linux bundle instead of rebuilding")
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/viamind/appimage-inputs")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    print(package(args.output, args.runtime, args.cache, args.bundle, args.offline))


if __name__ == "__main__":
    main()

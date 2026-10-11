#!/usr/bin/env python3
"""Build an isolated pinned runtime and a source/object companion for relinking."""
import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile

from appimage_support import ROOT, cached_input, digest, extract_regular, pins, publish_new


def build(output, cache, offline=False):
    if sys.version_info < (3, 12) or not sys.platform.startswith("linux") or os.uname().machine != "x86_64":
        raise ValueError("runtime preparation requires Python3.12+ on x86_64 Linux")
    output = Path(output).absolute()
    if os.path.lexists(output):
        raise ValueError("runtime output already exists; choose a new directory")
    configuration = pins()
    sources = [cached_input(x, cache, offline) for x in configuration["runtime_sources"]]
    with tempfile.TemporaryDirectory(dir=output.parent, prefix=".runtime-build-") as scratch:
        scratch = Path(scratch)
        context = scratch / "context"
        context.mkdir()
        extract_regular(sources[0], scratch / "runtime-source", max_bytes=8 * 1024 * 1024)
        roots = list((scratch / "runtime-source").iterdir())
        if len(roots) != 1 or not roots[0].is_dir():
            raise ValueError("unexpected pinned runtime source layout")
        shutil.move(roots[0], context / "source")
        # The archive's generated version file is replaced by its full source pin.
        (context / "source/src/runtime/version").write_text(configuration["runtime_commit"] + "\n")
        (context / "inputs").mkdir()
        for source in sources:
            shutil.copyfile(source, context / "inputs" / source.name)
        for name in ["Dockerfile", "build-runtime.sh"]:
            shutil.copyfile(ROOT / "packaging/appimage" / name, context / name)
        recipe = {name: digest(context / name) for name in ["Dockerfile", "build-runtime.sh"]}
        key = __import__("hashlib").sha256(json.dumps([configuration, recipe], sort_keys=True).encode()).hexdigest()
        image = "viamind-appimage-runtime:" + key[:24]
        if offline:
            subprocess.run(["docker", "image", "inspect", image], check=True, stdout=subprocess.DEVNULL)
        else:
            subprocess.run(["docker", "build", "--build-arg", "BASE_IMAGE=" + configuration["base_image"],
                            "--tag", image, str(context)], check=True)
        container = subprocess.check_output(["docker", "create", image], text=True).strip()
        try:
            result = scratch / "result"
            result.mkdir()
            subprocess.run(["docker", "cp", container + ":/result/.", str(result)], check=True)
        finally:
            subprocess.run(["docker", "rm", container], check=True, stdout=subprocess.DEVNULL)
        installed = (result / "installed-packages.txt").read_text()
        for expected in ["musl-1.2.5", "zlib-1.3.2", "zstd-1.5.6", "mimalloc2-2.1.7"]:
            if not re.search(r"^" + re.escape(expected) + r"-r\d+$", installed, re.MULTILINE):
                raise ValueError("runtime dependency differs from included source: " + expected)
        notices = result / "licenses"
        notices.mkdir()
        for source in sources:
            copied = 0
            # Read only legal text from pinned source archives. Never extract their
            # unrelated test symlinks (the zstd archive contains two such links).
            total = 0
            with tarfile.open(source) as archive:
                for member in archive:
                    name = PurePosixPath(member.name)
                    if name.is_absolute() or ".." in name.parts:
                        raise ValueError("escaping source archive path")
                    if member.isfile() and any(x in name.name.lower() for x in ["license", "licence", "copying", "copyright", "lgpl", "gpl2"]):
                        total += member.size
                        if total > 4 * 1024 * 1024:
                            raise ValueError("license inventory exceeds cap")
                        target = notices / source.name / member.name
                        target.parent.mkdir(parents=True, exist_ok=True)
                        with archive.extractfile(member) as stream:
                            target.write_bytes(stream.read(member.size))
                        copied += 1
            if not copied:
                raise ValueError("source lacks retained license text: " + source.name)
        shutil.copyfile(ROOT / "packaging/appimage/build-runtime.sh", result / "build-runtime.sh")
        shutil.copyfile(ROOT / "packaging/appimage/Dockerfile", result / "Dockerfile")
        (result / "inputs.json").write_text(json.dumps(configuration, indent=2) + "\n")
        (result / "README.txt").write_text(
            "Viamind AppImage runtime source and relink companion\n\n"
            "The sources directory retains the exact upstream source archives and libfuse patches.\n"
            "licenses retains their actual license texts; installed-package metadata records resolved Alpine packages.\n"
            "To relink, use an x86_64 Linux host with GNU ld, Python3 and a POSIX shell:\n"
            "  ./relink/relink.sh /absolute/path/to/NEW-runtime\n"
            "The relink directory contains the runtime object, CRT objects and all static libraries.\n"
            "Replace relink/lib/libfuse3.a with a compatible rebuilt libfuse to modify that library.\n"
            "Do not modify the application or source files to relink. The runtime is independent of Viamind.\n"
            "The Dockerfile/build-runtime.sh and inputs.json document the original isolated build.\n"
            "Resolved package revisions are recorded, not a claim of reproducible mutable repository state.\n")
        # Verify from only the distributable directory, outside the build context.
        runtime = result / "runtime-x86_64"
        subprocess.run([str(result / "relink/relink.sh"), str(runtime)], check=True, cwd=result)
        checked = subprocess.run([str(runtime), "--appimage-version"], check=True, text=True, capture_output=True, cwd=result)
        if configuration["runtime_commit"] not in checked.stdout + checked.stderr:
            raise ValueError("relinked runtime does not identify its source pin")
        files = {str(p.relative_to(result)): digest(p) for p in sorted(result.rglob("*")) if p.is_file()}
        metadata = {"configuration": configuration, "recipe_sha256": recipe, "docker_image": image,
                    "relink_from_companion_only": True, "runtime_version_output": checked.stdout + checked.stderr,
                    "files_sha256": files}
        (result / "manifest.json").write_text(json.dumps(metadata, indent=2) + "\n")
        publish_new(result, output)
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--cache", type=Path, default=Path.home() / ".cache/viamind/appimage-inputs")
    parser.add_argument("--offline", action="store_true")
    args = parser.parse_args()
    print(build(args.output, args.cache, args.offline))


if __name__ == "__main__":
    main()

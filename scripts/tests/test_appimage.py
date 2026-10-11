"""Adversarial publication/input checks and real AppRun process semantics."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1]))
import appimage_support as support
spec = importlib.util.spec_from_file_location("package_appimage", Path(__file__).parents[1] / "package-appimage.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class AppImageTests(unittest.TestCase):
    def test_apprun_preserves_exact_arguments_cwd_env_streams_and_exit(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            appdir = root / "image directory with spaces"
            binary = appdir / "usr/bin/viamind"
            binary.parent.mkdir(parents=True)
            binary.write_text("#!/usr/bin/env python3\nimport os,sys,json\n"
                              "print(json.dumps([sys.argv[1:],os.getcwd(),os.environ['VIAMIND_TEST_VALUE'],os.environ['appdir'],sys.stdin.read()]))\n"
                              "print('diagnostic',file=sys.stderr)\nsys.exit(17)\n")
            binary.chmod(0o755)
            launcher = appdir / "AppRun"
            launcher.write_text(package.apprun_text())
            launcher.chmod(0o755)
            cwd = root / "unrelated working directory"
            cwd.mkdir()
            args = ["--open", "relative board.json", "", "literal $HOME;`false`", "μΩ"]
            result = subprocess.run([str(launcher), *args], cwd=cwd, env=dict(os.environ, VIAMIND_TEST_VALUE="kept μ", appdir="caller sentinel"),
                                    input="protocol input\n", text=True, capture_output=True)
            self.assertEqual(result.returncode, 17)
            self.assertEqual(json.loads(result.stdout), [args, str(cwd), "kept μ", "caller sentinel", "protocol input\n"])
            self.assertEqual(result.stderr, "diagnostic\n")

    def test_bad_cached_tool_bytes_and_symlinks_refuse_before_execution(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            path = root / "tool"
            entry = dict(name="tool", sha256=hashlib.sha256(b"expected").hexdigest(), max_bytes=16, url="https://example.invalid/tool")
            path.write_bytes(b"corrupt")
            with patch("urllib.request.urlopen", side_effect=AssertionError("must not download")):
                with self.assertRaisesRegex(ValueError, "checksum/type"):
                    support.cached_input(entry, root)
                path.unlink()
                path.symlink_to(root / "absent")
                with self.assertRaisesRegex(ValueError, "checksum/type"):
                    support.cached_input(entry, root)
                path.unlink()
                with self.assertRaisesRegex(ValueError, "offline input missing"):
                    support.cached_input(entry, root, offline=True)

    def test_archive_paths_links_duplicates_and_expansion_are_bounded(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            cases = [("../escape", tarfile.REGTYPE), ("/escape", tarfile.REGTYPE),
                     ("symbolic", tarfile.SYMTYPE), ("hard", tarfile.LNKTYPE), ("fifo", tarfile.FIFOTYPE)]
            for index, (name, kind) in enumerate(cases):
                archive = root / f"bad{index}.tar"
                with tarfile.open(archive, "w") as target:
                    member = tarfile.TarInfo(name)
                    member.type, member.linkname = kind, "outside"
                    target.addfile(member)
                with self.assertRaisesRegex(ValueError, "unsafe"):
                    support.extract_regular(archive, root / f"extract{index}")
            archive = root / "cap.tar"
            with tarfile.open(archive, "w") as target:
                member = tarfile.TarInfo("large")
                member.size = 4
                target.addfile(member, io.BytesIO(b"four"))
                target.addfile(member, io.BytesIO(b"four"))
            for cap, count in [(3, 10), (100, 1), (100, 10)]:
                with self.assertRaises(ValueError):
                    support.extract_regular(archive, root / f"cap{cap}-{count}", max_bytes=cap, max_members=count)
            self.assertFalse((root / "escape").exists())

    def test_atomic_publish_preserves_racing_destination(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "stage"
            source.mkdir()
            (source / "image").write_text("new")
            for kind in ["directory", "file", "dangling"]:
                destination = root / kind
                if kind == "directory":
                    destination.mkdir()
                elif kind == "file":
                    destination.write_text("keep")
                else:
                    destination.symlink_to(root / "absent")
                inode = destination.lstat().st_ino
                with self.assertRaises(FileExistsError):
                    support.publish_new(source, destination)
                self.assertEqual(destination.lstat().st_ino, inode)
                self.assertEqual((source / "image").read_text(), "new")
            support.publish_new(source, root / "new")
            self.assertEqual((root / "new/image").read_text(), "new")

    def test_runtime_inventory_digest_and_failed_package_cleanup(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            runtime = root / "runtime"
            runtime.mkdir()
            (runtime / "runtime-x86_64").write_bytes(b"runtime")
            manifest = dict(configuration=support.pins(), relink_from_companion_only=True,
                            files_sha256={"runtime-x86_64": support.digest(runtime / "runtime-x86_64")})
            (runtime / "manifest.json").write_text(json.dumps(manifest))
            package.check_runtime(runtime)
            (runtime / "extra").write_text("unlisted")
            with self.assertRaisesRegex(ValueError, "inventory"):
                package.check_runtime(runtime)
            (runtime / "extra").unlink()
            (runtime / "runtime-x86_64").write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "checksum"):
                package.check_runtime(runtime)
            (runtime / "runtime-x86_64").write_bytes(b"runtime")
            (runtime / "link").symlink_to(root / "absent")
            with self.assertRaisesRegex(ValueError, "links"):
                package.check_runtime(runtime)
            (runtime / "link").unlink()
            # A bundle failure after staging must clean only its own stage and publish nothing.
            bundle = root / "bundle"
            bundle.mkdir()
            tool = root / "unused-tool"
            tool.write_text("unused")
            with patch.object(package, "cached_input", return_value=tool):
                with self.assertRaisesRegex(ValueError, "bundle archive"):
                    package.package(root / "output", runtime, root / "cache", bundle, offline=True)
            self.assertFalse((root / "output").exists())
            self.assertFalse(list(root.glob(".appimage-build-*")))
            self.assertEqual((runtime / "runtime-x86_64").read_bytes(), b"runtime")

    def test_bundle_provenance_cannot_control_output_paths_or_wrong_architecture(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "viamind").write_bytes(b"application")
            info = dict(host="x86_64-unknown-linux-gnu", headless_mcp_verified=True, version="0.1.0", revision="a"*40,
                        binary_sha256=support.digest(root / "viamind"))
            (root / "build-info.json").write_text(json.dumps(info))
            self.assertEqual(package.read_bundle_info(root), info)
            for field, value in [("host", "aarch64-unknown-linux-gnu"), ("version", "../../outside"),
                                 ("revision", "../outside"), ("binary_sha256", "0"*64), ("headless_mcp_verified", "yes")]:
                (root / "build-info.json").write_text(json.dumps(dict(info, **{field:value})))
                with self.assertRaisesRegex(ValueError, "provenance"):
                    package.read_bundle_info(root)

    def test_relink_refuses_destination_created_after_early_check(self):
        # Exercise the exact distributed shell program with a linker double
        # that creates a racing destination after the shell's initial check.
        source = (support.ROOT / "packaging/appimage/build-runtime.sh").read_text()
        shell = source.split("cat > /result/relink/relink.sh <<'EOF'\n", 1)[1].split("\nEOF", 1)[0] + "\n"
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            kit = root / "kit with spaces"
            kit.mkdir()
            relink = kit / "relink.sh"
            relink.write_text(shell)
            relink.chmod(0o755)
            tools = root / "tools"
            tools.mkdir()
            linker = tools / "ld"
            linker.write_text("#!/usr/bin/env python3\nimport os,pathlib,sys\n"
                              "pathlib.Path(sys.argv[sys.argv.index('-o')+1]).write_bytes(bytes(20))\n"
                              "pathlib.Path(os.environ['RACE_DESTINATION']).write_text('racing artifact')\n")
            linker.chmod(0o755)
            out = root / "NEW runtime"
            result = subprocess.run([str(relink), str(out)], env=dict(os.environ, PATH=str(tools)+os.pathsep+os.environ['PATH'],
                                      RACE_DESTINATION=str(out)), capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(out.read_text(), "racing artifact")
            self.assertFalse(list(root.glob("NEW runtime.relink.*")))

    def test_wrong_architecture_refuses_before_inputs_or_publication(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with patch.object(package.platform, "machine", return_value="aarch64"), patch.object(package, "check_runtime", side_effect=AssertionError("input read")):
                with self.assertRaisesRegex(ValueError, "x86_64"):
                    package.package(root / "out", root / "runtime", root / "cache")
            self.assertEqual(list(root.iterdir()), [])


if __name__ == "__main__":
    unittest.main()

"""Packaging guarantees that need adversarial inputs rather than a build alone."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("package_linux", Path(__file__).parents[1] / "package-linux.py")
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class LinuxBundleTests(unittest.TestCase):
    def test_atomic_publication_preserves_every_existing_destination(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            source = root / "staged"
            source.mkdir()
            (source / "archive").write_text("completed")
            for kind in ("directory", "file", "dangling-symlink"):
                destination = root / kind
                if kind == "directory":
                    destination.mkdir()
                elif kind == "file":
                    destination.write_text("keep")
                else:
                    destination.symlink_to(root / "absent")
                before = destination.lstat()
                with self.assertRaises(FileExistsError):
                    package.publish_new(source, destination)
                self.assertEqual(destination.lstat().st_ino, before.st_ino)
                self.assertEqual((source / "archive").read_text(), "completed")
            destination = root / "new"
            package.publish_new(source, destination)
            self.assertEqual((destination / "archive").read_text(), "completed")
            self.assertFalse(source.exists())

    def test_dependency_closure_keeps_shared_normal_and_build_dependencies(self):
        def dependency(name, *kinds):
            return {"pkg": name, "dep_kinds": [{"kind": kind} for kind in kinds]}

        metadata = {
            "packages": [{"id": name, "name": name, "version": "1", "source": None if name == "viamind" else "registry"}
                         for name in ("viamind", "normal", "build", "shared", "dev", "dev-child")],
            "resolve": {"nodes": [
                {"id": "viamind", "deps": [dependency("normal", None), dependency("build", "build"), dependency("dev", "dev")]},
                {"id": "normal", "deps": [dependency("shared", None, "dev")]},
                {"id": "build", "deps": [dependency("shared", None)]},
                {"id": "shared", "deps": []},
                {"id": "dev", "deps": [dependency("dev-child", None)]},
                {"id": "dev-child", "deps": []},
            ]},
        }
        self.assertEqual([p["name"] for p in package.application_dependencies(metadata)],
                         ["build", "normal", "shared", "viamind"])


if __name__ == "__main__":
    unittest.main()

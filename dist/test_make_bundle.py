"""Tests for make-bundle.sh. Run: python3 -m unittest dist/test_make_bundle.py

Needs a built inker (core/target/debug/inker or $INKER_BIN).
"""

import json
import os
import pathlib
import subprocess
import tempfile
import unittest
import zipfile

HERE = pathlib.Path(__file__).resolve().parent
SCRIPT = HERE / "make-bundle.sh"
BIN = pathlib.Path(
    os.environ.get("INKER_BIN", HERE.parent / "core" / "target" / "debug" / "inker")
)


@unittest.skipUnless(BIN.exists(), f"build inker first ({BIN})")
class MakeBundle(unittest.TestCase):
    def run_script(self, *args, env=None):
        full = {**os.environ, "INKER_BIN": str(BIN), **(env or {})}
        return subprocess.run(
            [str(SCRIPT), *args], capture_output=True, text=True, env=full, check=False
        )

    def test_writes_a_bundle_at_the_given_path(self):
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d) / "x" / "INKER.mcpb"
            r = self.run_script(str(out), "4.5.6")
            self.assertEqual(r.returncode, 0, r.stderr)
            with zipfile.ZipFile(out) as z:
                self.assertEqual(
                    sorted(z.namelist()), ["README.md", "inker", "manifest.json"]
                )
                m = json.loads(z.read("manifest.json"))
                self.assertEqual(m["name"], "inker")
                self.assertEqual(m["version"], "4.5.6")
                self.assertEqual(z.read("inker"), BIN.read_bytes())
            self.assertIn("not signed", r.stderr)

    def test_refuses_a_missing_binary(self):
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d) / "INKER.mcpb"
            r = self.run_script(str(out), env={"INKER_BIN": str(pathlib.Path(d) / "nope")})
            self.assertNotEqual(r.returncode, 0)
            self.assertIn("nope", r.stderr)
            self.assertFalse(out.exists())


if __name__ == "__main__":
    unittest.main()

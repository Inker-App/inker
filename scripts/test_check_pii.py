#!/usr/bin/env python3
"""Tests for scripts/check-pii.py. Run: python3 -I scripts/test_check_pii.py

Every value here is synthetic. Values that must *trigger* the checker are
assembled at run time from pieces (see `j`), so this file itself never contains
a match and passes the checker it tests.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.dont_write_bytecode = True  # keep __pycache__ out of the repo
HERE = Path(__file__).resolve().parent


def load_checker():
    spec = importlib.util.spec_from_file_location("check_pii", HERE / "check-pii.py")
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    sys.modules["check_pii"] = mod
    spec.loader.exec_module(mod)
    return mod


cp = load_checker()


def j(*parts: str) -> str:
    """Join pieces so the source never contains the assembled value."""
    return "".join(parts)


# Structurally valid but synthetic: the SSN is the SSA's own example of one that
# was never issued (1-2-3...9); the EIN is the one in the design boards; the
# routing number is 12345678 plus its computed check digit; 4111... is the
# card networks' published test number.
VALID_SSN = j("123", "-45-", "6789")
VALID_EIN = j("12", "-", "3456789")
VALID_ROUTING = j("1234", "56780")
VALID_CARD = j("4111", "1111", "1111", "1111")
VALID_CARD_SPACED = j("4111 ", "1111 ", "1111 ", "1111")


class WithoutPublishedSamples(unittest.TestCase):
    """The test values below are themselves published samples, which the
    checker ignores by default. These tests switch that allowlist off so the
    structural rules are what is under test."""

    def setUp(self):
        self._saved = cp.PUBLISHED_SAMPLES
        cp.PUBLISHED_SAMPLES = frozenset()

    def tearDown(self):
        cp.PUBLISHED_SAMPLES = self._saved


def kinds(findings) -> list[str]:
    return [f.kind for f in findings]


class Rules(unittest.TestCase):
    def test_ssn_rules(self):
        self.assertTrue(cp.ssn_valid("123", "45", "6789"))
        self.assertFalse(cp.ssn_valid("000", "12", "3456"))
        self.assertFalse(cp.ssn_valid("666", "12", "3456"))
        self.assertFalse(cp.ssn_valid("900", "12", "3456"))
        self.assertFalse(cp.ssn_valid("999", "12", "3456"))
        self.assertFalse(cp.ssn_valid("123", "00", "3456"))
        self.assertFalse(cp.ssn_valid("123", "45", "0000"))

    def test_ein_prefixes(self):
        for p in ("00", "07", "08", "09", "17", "18", "19", "28", "29", "49", "69", "70", "78", "79", "89", "96", "97"):
            self.assertFalse(cp.ein_valid(p), p)
        for p in ("01", "06", "10", "12", "16", "20", "27", "30", "48", "50", "68", "71", "77", "80", "88", "90", "95", "98", "99"):
            self.assertTrue(cp.ein_valid(p), p)

    def test_aba_checksum_and_district(self):
        self.assertTrue(cp.aba_valid(VALID_ROUTING))
        self.assertFalse(cp.aba_valid("123456789"))  # checksum fails
        # checksum passes but the prefix is not a Federal Reserve district
        self.assertFalse(cp.aba_valid(j("5", "00000005")))
        self.assertTrue(cp.aba_checksum(j("5", "00000005")))
        self.assertFalse(cp.aba_valid(j("0000", "00000")))

    def test_luhn(self):
        self.assertTrue(cp.luhn_valid(VALID_CARD))
        self.assertFalse(cp.luhn_valid("4111111111111112"))

    def test_mask_keeps_separators_and_last_two(self):
        self.assertEqual(cp.mask(VALID_SSN), "***-**-**89")
        self.assertEqual(cp.mask(VALID_EIN), "**-*****89")
        self.assertEqual(cp.mask(VALID_CARD_SPACED), "**** **** **** **11")


class ScanText(WithoutPublishedSamples):
    def scan(self, text: str, path: str = "core/src/lib.rs"):
        return cp.scan_text(path, text)

    def test_finds_valid_ssn(self):
        f = self.scan(f"let x = \"{VALID_SSN}\";\n")
        self.assertEqual(kinds(f), ["ssn"])
        self.assertEqual(f[0].line, 1)
        self.assertEqual(f[0].masked, "***-**-**89")

    def test_ignores_synthetic_ssn(self):
        self.assertEqual(self.scan("000-12-3456 666-48-2210 900-12-3456"), [])

    def test_finds_ein_with_campus_prefix_only(self):
        self.assertEqual(kinds(self.scan(f"EIN {VALID_EIN}")), ["ein"])
        self.assertEqual(self.scan("EIN 07-1234567 and 89-7654321"), [])

    def test_finds_routing(self):
        f = self.scan(f"line one\nrouting {VALID_ROUTING}\n")
        self.assertEqual(kinds(f), ["routing"])
        self.assertEqual(f[0].line, 2)
        self.assertEqual(self.scan("routing 123456789"), [])

    def test_finds_card(self):
        self.assertEqual(kinds(self.scan(f"card {VALID_CARD}")), ["card"])
        self.assertEqual(kinds(self.scan(f"card {VALID_CARD_SPACED}")), ["card"])
        self.assertEqual(self.scan("card 4111111111111112"), [])

    def test_bare_nine_digit_ssn_only_next_to_a_label(self):
        bare = VALID_SSN.replace("-", "")
        self.assertEqual(kinds(self.scan(f"SSN: {bare}")), ["ssn"])
        self.assertEqual(kinds(self.scan(f"Recipient's TIN {bare}")), ["ssn"])
        self.assertEqual(self.scan(f"order {bare}"), [])
        self.assertEqual(self.scan("SSN: 900123456"), [])

    def test_digits_inside_identifiers_are_ignored(self):
        self.assertEqual(self.scan(f"sha a{VALID_CARD}b x{VALID_ROUTING}y"), [])
        self.assertEqual(self.scan(f"1{VALID_SSN}"), [])

    def test_output_never_contains_value(self):
        text = f"{VALID_SSN} {VALID_EIN} {VALID_ROUTING} {VALID_CARD}\n"
        findings = self.scan(text)
        self.assertEqual(sorted(kinds(findings)), ["card", "ein", "routing", "ssn"])
        for f in findings:
            rendered = cp.render(f)
            for v in (VALID_SSN, VALID_EIN, VALID_ROUTING, VALID_CARD):
                self.assertNotIn(v, rendered)
            self.assertIn("core/src/lib.rs:1", rendered)


class Paths(WithoutPublishedSamples):
    def test_allowed_dirs_skip_content_checks(self):
        text = f"{VALID_SSN}\n"
        for path in (
            "samples/synthetic/w2.txt",
            "docs/handoff/design/AppPopReady.dc.html",
            "core/crates/inker-detect/tests/fixtures/x.txt",
            "core/crates/inker-detect/fixtures/x.txt",
        ):
            self.assertEqual(cp.check_file(path, text.encode()), [], path)

    def test_samples_outside_synthetic_are_scanned(self):
        self.assertEqual(kinds(cp.check_file("samples/README.md", f"{VALID_SSN}\n".encode())), ["ssn"])

    def test_binary_documents_outside_allowed_dirs(self):
        for name in ("a.pdf", "b.heic", "c.JPG", "d.jpeg", "e.png", "f.HEIC"):
            self.assertEqual(kinds(cp.check_file(f"core/{name}", b"\x00\x01")), ["document-file"], name)
        self.assertEqual(cp.check_file("samples/synthetic/w2.pdf", b"%PDF-1.7"), [])
        self.assertEqual(cp.check_file("docs/handoff/design/x.png", b"\x89PNG"), [])
        self.assertEqual(cp.check_file("docs/screenshot.png", b"\x89PNG"), [])
        # The app icon is artwork, not a document.
        self.assertEqual(cp.check_file("app/Resources/AppIcon-1024.png", b"\x89PNG"), [])
        self.assertEqual(cp.check_file("app/dmg/background@2x.png", b"\x89PNG"), [])
        self.assertEqual(kinds(cp.check_file("app/Resources/scan.pdf", b"%PDF")), ["document-file"])

    def test_fixture_dirs_still_reject_documents(self):
        self.assertEqual(kinds(cp.check_file("core/tests/fixtures/a.pdf", b"%PDF")), ["document-file"])

    def test_binary_content_is_not_scanned(self):
        self.assertEqual(cp.check_file("core/blob.bin", b"\x00" + VALID_SSN.encode()), [])


class PublishedSamples(unittest.TestCase):
    def test_ssa_sample_ssns_are_ignored_in_any_form(self):
        for v in (j("078", "-05-", "1120"), j("219", "-09-", "9999"), VALID_SSN):
            self.assertEqual(cp.scan_text("x.rs", f"ssn {v}\n"), [], v)
            self.assertEqual(cp.scan_text("x.rs", f"ssn {v.replace('-', '')}\n"), [], v)

    def test_network_test_cards_are_ignored(self):
        for v in (VALID_CARD, VALID_CARD_SPACED, j("5555", "5555", "5555", "4444"), j("3782", "822463", "10005")):
            self.assertEqual(cp.scan_text("x.rs", f"card {v}\n"), [], v)

    def test_other_values_are_still_found(self):
        self.assertEqual(kinds(cp.scan_text("x.rs", f"{VALID_EIN}\n")), ["ein"])


class AllowMarker(WithoutPublishedSamples):
    def test_marker_suppresses_only_its_line(self):
        text = f"a {VALID_ROUTING}  // pii-check: allow (checksum fixture)\nb {VALID_ROUTING}\n"
        f = cp.scan_text("x.rs", text)
        self.assertEqual([(x.kind, x.line) for x in f], [("routing", 2)])

    def test_marker_does_not_excuse_document_files(self):
        self.assertEqual(kinds(cp.check_file("core/a.pdf", b"%PDF pii-check: allow")), ["document-file"])


class SelfCheck(unittest.TestCase):
    def test_this_file_and_the_checker_pass(self):
        for name in ("test_check_pii.py", "check-pii.py"):
            self.assertEqual(cp.check_file(f"scripts/{name}", (HERE / name).read_bytes()), [], name)


class Cli(WithoutPublishedSamples):
    def run_main(self, args):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = cp.main(args)
        return code, out.getvalue() + err.getvalue()

    def test_paths_mode_fails_on_match_and_masks(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "notes.txt"
            p.write_text(f"ok\nssn {VALID_SSN}\n")
            code, output = self.run_main([str(p)])
            self.assertEqual(code, 1)
            self.assertIn("notes.txt:2: ssn", output)
            self.assertNotIn(VALID_SSN, output)
            self.assertIn("***-**-**89", output)

    def test_paths_mode_passes_clean(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "notes.txt"
            p.write_text("nothing to see 000-12-3456\n")
            self.assertEqual(self.run_main([str(p)])[0], 0)

    def test_staged_mode_reads_the_index(self):
        with tempfile.TemporaryDirectory() as d:
            env = {**os.environ, "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null"}

            def git(*a):
                subprocess.run(["git", "-C", d, *a], check=True, capture_output=True, env=env)

            git("init", "-q")
            Path(d, "a.txt").write_text(f"{VALID_EIN}\n")
            git("add", "a.txt")
            # The working copy is clean, but the staged blob is not.
            Path(d, "a.txt").write_text("clean\n")
            Path(d, "b.pdf").write_bytes(b"%PDF")
            git("add", "-f", "b.pdf")
            prev = os.getcwd()
            os.chdir(d)
            try:
                code, output = self.run_main([])
            finally:
                os.chdir(prev)
            self.assertEqual(code, 1)
            self.assertIn("a.txt:1: ein", output)
            self.assertIn("b.pdf: document-file", output)


class InstallHooks(WithoutPublishedSamples):
    def test_symlinks_checker_as_pre_commit_and_hook_runs(self):
        import shutil

        with tempfile.TemporaryDirectory() as d:
            env = {**os.environ, "GIT_CONFIG_GLOBAL": "/dev/null", "GIT_CONFIG_SYSTEM": "/dev/null"}
            subprocess.run(["git", "init", "-q", d], check=True, env=env)
            shutil.copytree(HERE, Path(d, "scripts"), ignore=shutil.ignore_patterns("__pycache__"))
            subprocess.run(["sh", "scripts/install-hooks.sh"], cwd=d, check=True, capture_output=True, env=env)
            hook = Path(d, ".git", "hooks", "pre-commit")
            self.assertTrue(hook.is_symlink())
            self.assertEqual(hook.resolve(), Path(d, "scripts", "check-pii.py").resolve())
            # Running twice is fine.
            subprocess.run(["sh", "scripts/install-hooks.sh"], cwd=d, check=True, capture_output=True, env=env)
            # A subprocess: the published-samples allowlist is live, so use the EIN.
            Path(d, "bad.txt").write_text(f"{VALID_EIN}\n")
            subprocess.run(["git", "add", "bad.txt"], cwd=d, check=True, env=env)
            r = subprocess.run([str(hook)], cwd=d, capture_output=True, text=True, env=env)
            self.assertEqual(r.returncode, 1, r.stderr)
            self.assertIn("bad.txt:1: ein", r.stdout)


if __name__ == "__main__":
    sys.exit(unittest.main(verbosity=1))

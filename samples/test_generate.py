#!/usr/bin/env python3
"""Tests for samples/generate.py. Run: python3 -I samples/test_generate.py"""

from __future__ import annotations

import importlib.util
import json
import re
import struct
import sys
import tempfile
import unittest
import zlib
from pathlib import Path

sys.dont_write_bytecode = True  # keep __pycache__ out of the repo
HERE = Path(__file__).resolve().parent


def load_generator():
    spec = importlib.util.spec_from_file_location("generate", HERE / "generate.py")
    assert spec and spec.loader
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


gen = load_generator()

STEMS = [
    "w2-acme-2025",
    "1099-int-harbor-2025",
    "1099-nec-bluefin-2025",
    "1099-div-northwind-2025",
    "1098-harbor-2025",
    "k1-tidewater-2025",
    "bank-statement-harbor-2025-03",
    "closing-statement-granite-2025",
    "receipt-cornerstone-2025-03-09",
    "1040-example-2023",
]
BINARIES = [
    "w2-acme-2025.pdf",
    "1099-int-harbor-2025.pdf",
    "w2-bluefin-2025-fillable.pdf",
    "w2-acme-2025-scan.pdf",
    "receipt-cornerstone-2025-03-09-photo.png",
]
SIDECARS_FOR_BINARIES = ["w2-acme-2025-scan.ocr.json", "receipt-cornerstone-2025-03-09-photo.ocr.json"]

UNISSUED_EIN_PREFIXES = {"00", "07", "08", "09", "17", "18", "19", "28", "29", "49", "69", "70", "78", "79", "89"}

SSN_RE = re.compile(r"(?<![\dA-Za-z])(\d{3})-(\d{2})-(\d{4})(?![\dA-Za-z])")
EIN_RE = re.compile(r"(?<![\dA-Za-z])(\d{2})-(\d{7})(?![\dA-Za-z])")
NINE_RE = re.compile(r"(?<![\dA-Za-z])(\d{9})(?![\dA-Za-z])")


def aba_ok(d: str) -> bool:
    w = [3, 7, 1] * 3
    return sum(int(c) * k for c, k in zip(d, w)) % 10 == 0


class Corpus(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.out = Path(cls.tmp.name)
        gen.main(cls.out)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def read(self, name: str) -> str:
        return (self.out / name).read_text(encoding="utf-8")

    def all_text(self) -> str:
        parts = []
        for p in sorted(self.out.iterdir()):
            if p.suffix in (".txt", ".json"):
                parts.append(p.read_text(encoding="utf-8"))
            elif p.suffix == ".pdf":
                parts.append(p.read_bytes().decode("latin-1"))
        return "\n".join(parts)

    def test_every_expected_file_exists(self):
        expected = {f"{s}.txt" for s in STEMS} | {f"{s}.ocr.json" for s in STEMS}
        expected |= set(BINARIES) | set(SIDECARS_FOR_BINARIES)
        self.assertEqual(expected, {p.name for p in self.out.iterdir()})

    def test_corpus_under_two_megabytes(self):
        self.assertLess(sum(p.stat().st_size for p in self.out.iterdir()), 2 * 1024 * 1024)

    def test_deterministic(self):
        with tempfile.TemporaryDirectory() as other:
            gen.main(Path(other))
            for p in self.out.iterdir():
                self.assertEqual(p.read_bytes(), (Path(other) / p.name).read_bytes(), p.name)

    def test_checked_in_corpus_is_current(self):
        checked_in = HERE / "synthetic"
        if not checked_in.exists():
            self.skipTest("samples/synthetic not generated")
        for p in self.out.iterdir():
            self.assertEqual(p.read_bytes(), (checked_in / p.name).read_bytes(), f"{p.name}: rerun generate.py")

    def test_ssns_are_never_issuable(self):
        found = SSN_RE.findall(self.all_text())
        self.assertTrue(found)
        for area, _group, _serial in found:
            self.assertTrue(area in ("000", "666") or area.startswith("9"), area)

    def test_eins_use_unissued_prefixes(self):
        found = EIN_RE.findall(self.all_text())
        self.assertTrue(found)
        for prefix, _rest in found:
            self.assertIn(prefix, UNISSUED_EIN_PREFIXES)

    def test_nine_digit_runs_fail_aba_checksum(self):
        for (run,) in [(m,) for m in NINE_RE.findall(self.all_text())]:
            area = run[:3]
            if area in ("000", "666") or area.startswith("9"):
                continue  # an SSN with separators dropped by OCR
            self.assertFalse(aba_ok(run), "a 9-digit run passes the routing checksum")

    def test_w2_uses_irs_box_labels(self):
        w2 = self.read("w2-acme-2025.txt")
        for label in [
            "a Employee's social security number",
            "b Employer identification number (EIN)",
            "c Employer's name, address, and ZIP code",
            "1 Wages, tips, other compensation",
            "2 Federal income tax withheld",
            "12a See instructions for box 12",
            "20 Locality name",
        ]:
            self.assertIn(label, w2)

    def test_other_forms_use_irs_labels(self):
        self.assertIn("1 Interest income", self.read("1099-int-harbor-2025.txt"))
        self.assertIn("1 Nonemployee compensation", self.read("1099-nec-bluefin-2025.txt"))
        self.assertIn("1a Total ordinary dividends", self.read("1099-div-northwind-2025.txt"))
        self.assertIn("1 Mortgage interest received from payer(s)/borrower(s)", self.read("1098-harbor-2025.txt"))
        self.assertIn("A Partnership's employer identification number", self.read("k1-tidewater-2025.txt"))
        self.assertIn("E Partner's SSN or TIN", self.read("k1-tidewater-2025.txt"))

    def test_1040_is_a_joint_return_with_dependent_deposit_and_preparer(self):
        f = self.read("1040-example-2023.txt")
        for label in [
            "U.S. Individual Income Tax Return",
            "Your first name and middle initial",
            "Your social security number",
            "If joint return, spouse's first name and middle initial",
            "Spouse's social security number",
            "Home address (number and street). If you have a P.O. box, see instructions.",
            "(2) Social security number",
            "9 Add lines 1z, 2b, 3b, 4b, 5b, 6b, 7, and 8. This is your total income",
            "11 Subtract line 10 from line 9. This is your adjusted gross income",
            "b Routing number",
            "d Account number",
            "Designee's name",
            "Paid Preparer Use Only",
            "Firm's EIN",
            "Form 1040 (2023)",
        ]:
            self.assertIn(label, f)
        for v in (gen.JORDAN["ssn"], gen.RILEY["ssn"], gen.DEPENDENT["ssn"], gen.PREPARER["ein"],
                  gen.PREPARER["ptin"], gen.HARBOR["routing"], gen.JORDAN_CHECKING):
            self.assertIn(v, f)

    def test_1040_columns_sit_under_their_labels(self):
        # Values are typed under their column heads, as on the printed form
        # (a row of sub-heads may come between).
        lines = self.read("1040-example-2023.txt").splitlines()
        for label, value in [
            ("Your social security number", gen.JORDAN["ssn"]),
            ("Spouse's social security number", gen.RILEY["ssn"]),
            ("(2) Social security number", gen.DEPENDENT["ssn"]),
            ("Your occupation", gen.JORDAN_1040["occupation"]),
            ("PTIN", gen.PREPARER["ptin"]),
        ]:
            i = next(i for i, l in enumerate(lines) if label in l)
            col = lines[i].index(label)
            below = next(l for l in lines[i + 1 :] if l[col : col + 1].strip())
            self.assertEqual(below[col : col + len(value)], value, label)

    def test_same_ssn_on_w2_and_k1(self):
        ssn = gen.JORDAN["ssn"]
        self.assertIn(ssn, self.read("w2-acme-2025.txt"))
        self.assertIn(ssn, self.read("k1-tidewater-2025.txt"))

    def test_two_people_two_employers(self):
        text = self.all_text()
        for v in (gen.JORDAN["full"], gen.RILEY["last"], gen.ACME["name"], gen.BLUEFIN["name"]):
            self.assertIn(v, text)

    def test_ocr_sidecars_have_schema_and_noise(self):
        for stem in STEMS:
            data = json.loads(self.read(f"{stem}.ocr.json"))
            self.assertEqual([1], [p["page"] for p in data["pages"]])
            self.assertIsInstance(data["pages"][0]["text"], str)
            self.assertNotEqual(data["pages"][0]["text"].strip(), self.read(f"{stem}.txt").strip(), stem)

    def test_ocr_w2_drops_ssn_separators_and_swaps_o_for_zero(self):
        text = json.loads(self.read("w2-acme-2025.ocr.json"))["pages"][0]["text"]
        self.assertIn(gen.JORDAN["ssn"].replace("-", ""), text)
        self.assertIn("84,25O.OO", text)

    def test_ocr_closing_statement_breaks_wire_routing_line(self):
        text = json.loads(self.read("closing-statement-granite-2025.ocr.json"))["pages"][0]["text"]
        self.assertNotIn(gen.TITLE_CO["routing"], text)
        self.assertIn("Routing num-\nber:", text)

    def test_noise_target_missing_is_an_error(self):
        with self.assertRaises(SystemExit):
            gen.replace_once("abc", "zzz", "y")


class PdfStructure(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        cls.out = Path(cls.tmp.name)
        gen.main(cls.out)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def check_xref(self, data: bytes):
        self.assertTrue(data.startswith(b"%PDF-1."))
        self.assertTrue(data.rstrip().endswith(b"%%EOF"))
        startxref = int(data.rsplit(b"startxref", 1)[1].split()[0])
        self.assertEqual(data[startxref : startxref + 4], b"xref")
        lines = data[startxref:].split(b"\n")
        count = int(lines[1].split()[1])
        for i in range(1, count):
            off = int(lines[2 + i][:10])
            self.assertTrue(data[off:].startswith(f"{i} 0 obj".encode()), f"object {i}")
            # every stream's /Length matches
        for m in re.finditer(rb"/Length (\d+) >>\nstream\n", data):
            n = int(m.group(1))
            self.assertEqual(data[m.end() + n : m.end() + n + 10], b"\nendstream")

    def test_all_pdfs_have_valid_xref_and_stream_lengths(self):
        for name in BINARIES:
            if name.endswith(".pdf"):
                with self.subTest(name):
                    self.check_xref((self.out / name).read_bytes())

    def test_text_pdf_carries_text_operators(self):
        data = (self.out / "w2-acme-2025.pdf").read_bytes()
        self.assertIn(b"/BaseFont /Helvetica", data)
        self.assertIn(b"(1 Wages, tips, other compensation", data)

    def test_scan_pdf_has_no_text(self):
        data = (self.out / "w2-acme-2025-scan.pdf").read_bytes()
        self.assertNotIn(b"BT", data)
        self.assertNotIn(b"Tj", data)

    def test_fillable_pdf_has_acroform_with_kids_and_utf16(self):
        data = (self.out / "w2-bluefin-2025-fillable.pdf").read_bytes()
        self.assertIn(b"/AcroForm 6 0 R", data)
        self.assertIn(b"/T (w2) /Kids [", data)
        for name, *_ in gen.FILLABLE_W2_FIELDS:
            self.assertIn(f"/T ({name})".encode(), data)
        self.assertIn(b"/FT /Ch", data)
        self.assertIn(b"<FEFF", data)
        # values are not in the text layer
        self.assertNotIn(b"(96,400.00) Tj", data)

    def test_pdf_literal_escapes(self):
        self.assertEqual(gen.pdf_literal("a(b)c\\"), b"(a\\(b\\)c\\\\)")

    def test_pdf_utf16_has_bom(self):
        self.assertEqual(gen.pdf_utf16("Ñ"), b"<FEFF00D1>")

    def test_png_is_valid(self):
        data = (self.out / "receipt-cornerstone-2025-03-09-photo.png").read_bytes()
        self.assertTrue(data.startswith(b"\x89PNG\r\n\x1a\n"))
        pos = 8
        kinds = []
        while pos < len(data):
            (length,) = struct.unpack(">I", data[pos : pos + 4])
            kind = data[pos + 4 : pos + 8]
            body = data[pos + 8 : pos + 8 + length]
            (crc,) = struct.unpack(">I", data[pos + 8 + length : pos + 12 + length])
            self.assertEqual(crc, zlib.crc32(kind + body))
            kinds.append(kind)
            pos += 12 + length
        self.assertEqual(kinds, [b"IHDR", b"IDAT", b"IEND"])


if __name__ == "__main__":
    sys.exit(unittest.main(verbosity=1))

#!/usr/bin/env -S python3 -I
"""Reject anything that looks like real PII before it is committed.

Usage:
    python3 -I scripts/check-pii.py            # staged files (pre-commit hook)
    python3 -I scripts/check-pii.py --all      # every tracked or untracked, non-ignored file
    python3 -I scripts/check-pii.py PATH...    # the given files

Fails (exit 1) when a file outside the allowed directories contains:
  - an SSN-shaped string that passes the SSA's rules (area not 000, 666 or
    9xx; group not 00; serial not 0000), dashed or spaced, or a bare 9-digit
    run on a line that mentions an SSN or TIN;
  - an EIN-shaped string (NN-NNNNNNN) whose prefix the IRS assigns;
  - a 9-digit routing-shaped string that passes the ABA 3-7-1 checksum and
    starts with a Federal Reserve routing symbol;
  - a 13-19 digit Luhn-valid card-shaped string;
or when a .pdf, .heic, .jpg, .jpeg or .png lives outside samples/synthetic/
and docs/.

Allowed for content: samples/synthetic/ (generated synthetic corpus),
docs/handoff/ (the design boards carry synthetic sample numbers, one of them
with a real EIN prefix), and test fixture directories (any path component
named fixtures, testdata or test-fixtures).

Matched values are never printed; only a mask keeping the last two
characters.
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from dataclasses import dataclass, field
from pathlib import Path

# ---------------------------------------------------------------------------
# Rules
# ---------------------------------------------------------------------------

# IRS campus EIN prefixes (irs.gov "How EINs are assigned and valid EIN
# prefixes"): Andover 10, 12; Atlanta 60, 67; Austin 50, 53; Brookhaven 01-06,
# 11, 13, 14, 16, 21-23, 25, 34, 51, 52, 54-59, 65; Cincinnati 30, 32, 35-38,
# 61; Fresno 15, 24; Kansas City 40, 44; Memphis 94, 95; Ogden 80, 90;
# Philadelphia 33, 39, 41-43, 46, 48, 62-64, 66, 68, 71-77, 81-88, 91-93,
# 98, 99; Internet 20, 26, 27, 45, 46, 47, 81-88, 92, 93, 98, 99; Small
# Business/Self Employed 31.
VALID_EIN_PREFIXES = frozenset(
    f"{n:02d}"
    for n in [
        *range(1, 7),
        *range(10, 17),
        *range(20, 28),
        *range(30, 49),
        *range(50, 69),
        *range(71, 78),
        *range(80, 89),
        *range(90, 96),
        98,
        99,
    ]
)

# First two digits of an ABA routing number: 01-12 Federal Reserve districts,
# 21-32 thrifts, 61-72 electronic, 80 traveler's cheques.
VALID_ROUTING_PREFIXES = frozenset(
    f"{n:02d}" for n in [*range(1, 13), *range(21, 33), *range(61, 73), 80]
)


# Values published precisely because they are not real: the SSA's never-valid
# SSNs (the 1938 Woolworth wallet insert, a 1940s advertisement, the
# sequential example), ignored as SSNs and as bare 9-digit runs, and the card
# networks' test numbers, ignored as cards. Not applied to EINs.
PUBLISHED_SAMPLES = frozenset(
    {
        "078051120",
        "219099999",
        "123456789",
        "4111111111111111",
        "4242424242424242",
        "4012888888881881",
        "5555555555554444",
        "5105105105105100",
        "2223003122003222",
        "378282246310005",
        "371449635398431",
        "6011111111111117",
        "6011000990139424",
        "3566002020360505",
        "3530111333300000",
        "30569309025904",
        "38520000023237",
    }
)

# A line containing this marker is not scanned. For test vectors that must
# pass a real checksum; say why next to it.
ALLOW_MARKER = "pii-check: allow"


def ssn_valid(area: str, group: str, serial: str) -> bool:
    return area not in ("000", "666") and not area.startswith("9") and group != "00" and serial != "0000"


def ein_valid(prefix: str) -> bool:
    return prefix in VALID_EIN_PREFIXES


def aba_checksum(digits: str) -> bool:
    weights = (3, 7, 1) * 3
    return sum(int(d) * w for d, w in zip(digits, weights)) % 10 == 0


def aba_valid(digits: str) -> bool:
    return len(digits) == 9 and digits[:2] in VALID_ROUTING_PREFIXES and aba_checksum(digits)


def luhn_valid(digits: str) -> bool:
    total = 0
    for i, ch in enumerate(reversed(digits)):
        d = int(ch)
        if i % 2 == 1:
            d *= 2
            if d > 9:
                d -= 9
        total += d
    return total % 10 == 0


def mask(value: str) -> str:
    """Replace every digit but the last two with '*', keeping separators."""
    digit_positions = [i for i, c in enumerate(value) if c.isdigit()]
    keep = set(digit_positions[-2:])
    return "".join("*" if c.isdigit() and i not in keep else c for i, c in enumerate(value))


# ---------------------------------------------------------------------------
# Scanning
# ---------------------------------------------------------------------------

# No letter, digit, underscore or joining punctuation on either side, so digit
# runs inside hashes, identifiers and longer numbers do not match.
_L = r"(?<![\w.-])"
_R = r"(?![\w]|[.-]\d)"

SSN_RE = re.compile(_L + r"(\d{3})([- ])(\d{2})\2(\d{4})" + _R)
EIN_RE = re.compile(_L + r"(\d{2})-(\d{7})" + _R)
NINE_RE = re.compile(_L + r"(\d{9})" + _R)
CARD_RE = re.compile(_L + r"(\d{13,19}|\d{4}([ -])\d{4}\2\d{4}\2\d{1,7}|\d{4}([ -])\d{6}\3\d{5})" + _R)
SSN_LABEL_RE = re.compile(r"\b(ssn|social security|tin)\b", re.IGNORECASE)


@dataclass(frozen=True)
class Finding:
    path: str
    line: int | None
    kind: str
    masked: str = ""
    digits: str = field(default="", repr=False, compare=False)


def _digits(s: str) -> str:
    return re.sub(r"\D", "", s)


def scan_text(path: str, text: str) -> list[Finding]:
    findings: list[Finding] = []
    for lineno, line in enumerate(text.splitlines(), start=1):
        if ALLOW_MARKER in line:
            continue
        for m in SSN_RE.finditer(line):
            if ssn_valid(m.group(1), m.group(3), m.group(4)):
                findings.append(Finding(path, lineno, "ssn", mask(m.group(0)), _digits(m.group(0))))
        for m in EIN_RE.finditer(line):
            if ein_valid(m.group(1)):
                findings.append(Finding(path, lineno, "ein", mask(m.group(0)), _digits(m.group(0))))
        labelled = bool(SSN_LABEL_RE.search(line))
        for m in NINE_RE.finditer(line):
            d = m.group(1)
            if aba_valid(d):
                findings.append(Finding(path, lineno, "routing", mask(d), d))
            elif labelled and ssn_valid(d[:3], d[3:5], d[5:]):
                findings.append(Finding(path, lineno, "ssn", mask(d), d))
        for m in CARD_RE.finditer(line):
            digits = _digits(m.group(0))
            if luhn_valid(digits):
                findings.append(Finding(path, lineno, "card", mask(m.group(0)), digits))
    return [f for f in findings if f.kind == "ein" or f.digits not in PUBLISHED_SAMPLES]


DOCUMENT_EXTS = frozenset({".pdf", ".heic", ".jpg", ".jpeg", ".png"})
CONTENT_ALLOWED_PREFIXES = ("samples/synthetic/", "docs/handoff/")
DOCUMENT_ALLOWED_PREFIXES = ("samples/synthetic/", "docs/")
# Artwork the app ships (icons), images only; a PDF here is still refused.
ARTWORK_ALLOWED_PREFIXES = ("app/Resources/",)
ARTWORK_EXTS = frozenset({".png"})
FIXTURE_DIRS = frozenset({"fixtures", "testdata", "test-fixtures"})


def _norm(path: str) -> str:
    p = path.replace(os.sep, "/")
    return p[2:] if p.startswith("./") else p


def content_allowed(path: str) -> bool:
    p = _norm(path)
    if p.startswith(CONTENT_ALLOWED_PREFIXES):
        return True
    return any(part in FIXTURE_DIRS for part in p.split("/")[:-1])


def check_file(path: str, data: bytes) -> list[Finding]:
    """Check one file. `path` is relative to the repository root."""
    p = _norm(path)
    if Path(p).suffix.lower() in DOCUMENT_EXTS:
        if p.startswith(DOCUMENT_ALLOWED_PREFIXES):
            return []
        if Path(p).suffix.lower() in ARTWORK_EXTS and p.startswith(ARTWORK_ALLOWED_PREFIXES):
            return []
        return [Finding(p, None, "document-file")]
    if content_allowed(p) or b"\x00" in data[:8192]:
        return []
    return scan_text(p, data.decode("utf-8", errors="replace"))


def render(f: Finding) -> str:
    if f.line is None:
        return f"{f.path}: {f.kind} (documents and images belong in samples/synthetic/ or docs/)"
    return f"{f.path}:{f.line}: {f.kind} {f.masked}"


# ---------------------------------------------------------------------------
# Git plumbing and CLI
# ---------------------------------------------------------------------------


def _git(*args: str) -> bytes:
    return subprocess.run(["git", *args], check=True, capture_output=True).stdout


def repo_root() -> Path | None:
    try:
        return Path(_git("rev-parse", "--show-toplevel").decode().strip())
    except (subprocess.CalledProcessError, FileNotFoundError):
        return None


def staged_files() -> list[tuple[str, bytes]]:
    names = _git("diff", "--cached", "--name-only", "-z", "--diff-filter=ACMR").split(b"\0")
    return [(n.decode(), _git("show", f":{n.decode()}")) for n in names if n]


def all_files(root: Path) -> list[tuple[str, bytes]]:
    names = _git("-C", str(root), "ls-files", "-z", "--cached", "--others", "--exclude-standard").split(b"\0")
    out = []
    for n in names:
        if not n:
            continue
        rel = n.decode()
        full = root / rel
        if full.is_file():
            out.append((rel, full.read_bytes()))
    return out


def given_files(paths: list[str], root: Path | None) -> list[tuple[str, bytes]]:
    out = []
    for raw in paths:
        full = Path(raw).resolve()
        rel = raw
        if root is not None:
            try:
                rel = full.relative_to(root.resolve()).as_posix()
            except ValueError:
                rel = raw
        out.append((rel, full.read_bytes()))
    return out


def main(argv: list[str]) -> int:
    root = repo_root()
    if argv == ["--all"]:
        if root is None:
            print("check-pii: --all needs a git repository", file=sys.stderr)
            return 2
        files = all_files(root)
    elif argv:
        files = given_files(argv, root)
    else:
        if root is None:
            print("check-pii: not in a git repository; pass paths or run inside one", file=sys.stderr)
            return 2
        files = staged_files()

    findings = [f for path, data in files for f in check_file(path, data)]
    for f in findings:
        print(render(f))
    if findings:
        print(
            f"check-pii: {len(findings)} possible real value(s) in {len({f.path for f in findings})} file(s). "
            "Use synthetic values (see samples/README.md).",
            file=sys.stderr,
        )
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))

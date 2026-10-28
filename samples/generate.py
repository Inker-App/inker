#!/usr/bin/env python3
"""Generate INKER's synthetic sample corpus into samples/synthetic/.

Run from anywhere with `python3 -I samples/generate.py`. Dependency-free and
deterministic: running it twice produces byte-identical files.

Every value in this corpus is invented. SSNs use area 000, 666 or 9xx (never
issued); EINs use prefixes the IRS does not issue (07, 19, 70, 78, 89); the
routing numbers fail the ABA 3-7-1 checksum; people, employers, banks and
addresses are made up. See samples/README.md.
"""

from __future__ import annotations

import json
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent / "synthetic"

# ---------------------------------------------------------------------------
# The synthetic cast. Two people, two employers. Jordan's SSN appears on the
# W-2 *and* the K-1 (and the 1099-INT and 1098) so token-stability tests can
# check that one value gets one token across documents.
# ---------------------------------------------------------------------------

JORDAN = {
    "first": "Jordan A",
    "last": "Example",
    "full": "Jordan A Example",
    "ssn": "900-12-3456",  # area 9xx: never issued as an SSN
    "street": "48 Lantern Way Apt 3",
    "city": "Springfield, OR 97477",
}
RILEY = {
    "first": "Riley",
    "last": "Sample",
    "full": "Riley Sample",
    "ssn": "666-48-2210",  # area 666: never issued
    "street": "1720 Quarry Road",
    "city": "Lakewood, CO 80226",
}
ACME = {
    "name": "Acme Robotics Inc",
    "ein": "07-1234567",  # prefix 07: not issued
    "street": "500 Gearwheel Blvd",
    "city": "Portland, OR 97201",
    "state_id": "OR 0000123-4",
}
BLUEFIN = {
    "name": "Bluefin Analytics LLC",
    "ein": "89-7654321",  # prefix 89: not issued
    "street": "77 Harbor Light Dr Suite 210",
    "city": "Denver, CO 80202",
    "state_id": "CO 00-000-1188",
}
HARBOR = {
    "name": "Harbor Street Credit Union",
    "ein": "19-2468013",  # prefix 19: not issued
    "street": "9 Harbor Street",
    "city": "Portland, OR 97209",
    "phone": "(503) 555-0142",
    "routing": "123456789",  # fails the 3-7-1 checksum
}
TIDEWATER = {
    "name": "Tidewater Partners LP",
    "ein": "78-1357924",  # prefix 78: not issued
    "street": "300 Estuary Plaza",
    "city": "Astoria, OR 97103",
}
NORTHWIND = {
    "name": "Northwind Index Fund",
    "ein": "70-2244668",  # prefix 70: not issued
    "street": "PO Box 0000",
    "city": "Boston, MA 02101",
}
TITLE_CO = {
    "name": "Granite Key Title & Escrow",
    "routing": "987654321",  # fails the 3-7-1 checksum
    "account": "0000-7713-9902",
}

JORDAN_CHECKING = "00012345678"
JORDAN_SAVINGS_INT_ACCT = "00098765432"
RILEY_BROKERAGE = "NW-0000-5521"
JORDAN_MORTGAGE = "ML-0000-88231"

# ---------------------------------------------------------------------------
# Plain-text renditions. Layout follows the IRS forms: the box letter or number
# precedes the label, and the value sits on the line below.
# ---------------------------------------------------------------------------


def w2_text(emp: dict, ee: dict, amounts: dict, control: str) -> str:
    return f"""\
Form W-2 Wage and Tax Statement 2025
Department of the Treasury - Internal Revenue Service
Copy B - To Be Filed With Employee's FEDERAL Tax Return

a Employee's social security number
  {ee["ssn"]}
b Employer identification number (EIN)
  {emp["ein"]}
c Employer's name, address, and ZIP code
  {emp["name"]}
  {emp["street"]}
  {emp["city"]}
d Control number
  {control}
e Employee's first name and initial    Last name    Suff.
  {ee["first"]}    {ee["last"]}
f Employee's address and ZIP code
  {ee["street"]}
  {ee["city"]}

1 Wages, tips, other compensation          2 Federal income tax withheld
  {amounts["1"]:>12}                              {amounts["2"]:>12}
3 Social security wages                    4 Social security tax withheld
  {amounts["3"]:>12}                              {amounts["4"]:>12}
5 Medicare wages and tips                  6 Medicare tax withheld
  {amounts["5"]:>12}                              {amounts["6"]:>12}
7 Social security tips                     8 Allocated tips
          0.00                                      0.00
9                                          10 Dependent care benefits
                                                    0.00
11 Nonqualified plans                      12a See instructions for box 12
          0.00                               D  {amounts["12a"]:>10}
13 Statutory employee  Retirement plan  Third-party sick pay
          [ ]                 [X]              [ ]
14 Other
  {amounts["14"]}
15 State  Employer's state ID number       16 State wages, tips, etc.
  {emp["state_id"]}                           {amounts["16"]:>12}
17 State income tax                        18 Local wages, tips, etc.
  {amounts["17"]:>12}                                      0.00
19 Local income tax                        20 Locality name
          0.00
"""


def f1099_int_text() -> str:
    j, h = JORDAN, HARBOR
    return f"""\
Form 1099-INT Interest Income 2025
Copy B For Recipient

PAYER'S name, street address, city or town, state or province, country, ZIP
or foreign postal code, and telephone no.
  {h["name"]}
  {h["street"]}
  {h["city"]}
  {h["phone"]}
PAYER'S TIN                         RECIPIENT'S TIN
  {h["ein"]}                          {j["ssn"]}
RECIPIENT'S name
  {j["full"]}
Street address (including apt. no.)
  {j["street"]}
City or town, state or province, country, and ZIP or foreign postal code
  {j["city"]}
Account number (see instructions)
  {JORDAN_SAVINGS_INT_ACCT}

1 Interest income                            $      412.88
2 Early withdrawal penalty                   $        0.00
3 Interest on U.S. Savings Bonds and Treasury obligations
                                             $        0.00
4 Federal income tax withheld                $        0.00
5 Investment expenses                        $        0.00
6 Foreign tax paid                           $        0.00
7 Foreign country or U.S. possession
8 Tax-exempt interest                        $        0.00
9 Specified private activity bond interest   $        0.00
10 Market discount                           $        0.00
11 Bond premium                              $        0.00
12 Bond premium on Treasury obligations      $        0.00
13 Bond premium on tax-exempt bond           $        0.00
14 Tax-exempt and tax credit bond CUSIP no.
15 State   16 State identification no.   17 State tax withheld
   OR         {h["ein"]}                  $        0.00
"""


def f1099_nec_text() -> str:
    r, b = RILEY, BLUEFIN
    return f"""\
Form 1099-NEC Nonemployee Compensation 2025
Copy B For Recipient

PAYER'S name, street address, city or town, state or province, country, ZIP
or foreign postal code, and telephone no.
  {b["name"]}
  {b["street"]}
  {b["city"]}
  (303) 555-0187
PAYER'S TIN                         RECIPIENT'S TIN
  {b["ein"]}                          {r["ssn"]}
RECIPIENT'S name
  {r["full"]}
Street address (including apt. no.)
  {r["street"]}
City or town, state or province, country, and ZIP or foreign postal code
  {r["city"]}
Account number (see instructions)
  BF-CONTRACTOR-0042

1 Nonemployee compensation                   $   18,500.00
2 Payer made direct sales totaling $5,000 or more of consumer
  products to recipient for resale            [ ]
3 Excess golden parachute payments           $        0.00
4 Federal income tax withheld                $        0.00
5 State tax withheld   6 State/Payer's state no.   7 State income
  $ 0.00                 {b["state_id"]}             $   18,500.00
"""


def f1099_div_text() -> str:
    r, n = RILEY, NORTHWIND
    return f"""\
Form 1099-DIV Dividends and Distributions 2025
Copy B For Recipient

PAYER'S name, street address, city or town, state or province, country, ZIP
or foreign postal code, and telephone no.
  {n["name"]}
  {n["street"]}
  {n["city"]}
  (617) 555-0110
PAYER'S TIN                         RECIPIENT'S TIN
  {n["ein"]}                          {r["ssn"]}
RECIPIENT'S name
  {r["full"]}
Street address (including apt. no.)
  {r["street"]}
City or town, state or province, country, and ZIP or foreign postal code
  {r["city"]}
Account number (see instructions)
  {RILEY_BROKERAGE}

1a Total ordinary dividends                  $    1,284.16
1b Qualified dividends                       $    1,102.40
2a Total capital gain distr.                 $      310.00
2b Unrecap. Sec. 1250 gain                   $        0.00
2c Section 1202 gain                         $        0.00
2d Collectibles (28%) gain                   $        0.00
2e Section 897 ordinary dividends            $        0.00
2f Section 897 capital gain                  $        0.00
3 Nondividend distributions                  $        0.00
4 Federal income tax withheld                $        0.00
5 Section 199A dividends                     $       42.75
6 Investment expenses                        $        0.00
7 Foreign tax paid                           $       18.22
8 Foreign country or U.S. possession          Various
9 Cash liquidation distributions             $        0.00
10 Noncash liquidation distributions         $        0.00
11 FATCA filing requirement                   [ ]
12 Exempt-interest dividends                 $        0.00
13 Specified private activity bond interest dividends
                                             $        0.00
14 State   15 State identification no.   16 State tax withheld
   CO                                      $        0.00
"""


def f1098_text() -> str:
    j, h = JORDAN, HARBOR
    return f"""\
Form 1098 Mortgage Interest Statement 2025
Copy B For Payer/Borrower

RECIPIENT'S/LENDER'S name, street address, city or town, state or province,
country, ZIP or foreign postal code, and telephone no.
  {h["name"]}
  {h["street"]}
  {h["city"]}
  {h["phone"]}
RECIPIENT'S/LENDER'S TIN            PAYER'S/BORROWER'S TIN
  {h["ein"]}                          {j["ssn"]}
PAYER'S/BORROWER'S name
  {j["full"]}
Street address (including apt. no.)
  {j["street"]}
City or town, state or province, country, and ZIP or foreign postal code
  {j["city"]}

1 Mortgage interest received from payer(s)/borrower(s)
                                             $   11,204.37
2 Outstanding mortgage principal             $  318,450.00
3 Mortgage origination date                     06/14/2021
4 Refund of overpaid interest                $        0.00
5 Mortgage insurance premiums                $        0.00
6 Points paid on purchase of principal residence
                                             $        0.00
7 If address of property securing mortgage is the same as
  PAYER'S/BORROWER'S address, the box is checked   [X]
8 Address or description of property securing mortgage
9 Number of properties securing the mortgage     1
10 Other   Real estate taxes paid $ 4,118.02
11 Mortgage acquisition date
Account number (see instructions)
  {JORDAN_MORTGAGE}
"""


def k1_text() -> str:
    j, t = JORDAN, TIDEWATER
    return f"""\
Schedule K-1 (Form 1065) 2025
Partner's Share of Income, Deductions, Credits, etc.
[X] Final K-1   [ ] Amended K-1

Part I    Information About the Partnership
A Partnership's employer identification number
  {t["ein"]}
B Partnership's name, address, city, state, and ZIP code
  {t["name"]}
  {t["street"]}
  {t["city"]}
C IRS center where partnership filed return
  e-file
D [ ] Check if this is a publicly traded partnership (PTP)

Part II   Information About the Partner
E Partner's SSN or TIN (Do not use TIN of a disregarded entity. See instructions.)
  {j["ssn"]}
F Name, address, city, state, and ZIP code for partner entered in E.
  {j["full"]}
  {j["street"]}
  {j["city"]}
G [ ] General partner or LLC member-manager   [X] Limited partner or other LLC member
H1 [X] Domestic partner   [ ] Foreign partner
I1 What type of entity is this partner?   Individual
J Partner's share of profit, loss, and capital (see instructions):
              Beginning        Ending
  Profit        2.500000 %     2.500000 %
  Loss          2.500000 %     2.500000 %
  Capital       2.500000 %     2.500000 %
K1 Partner's share of liabilities:
  Nonrecourse                $ 0.00
  Qualified nonrecourse      $ 12,400.00
  Recourse                   $ 0.00
L Partner's Capital Account Analysis
  Beginning capital account  $ 25,000.00
  Capital contributed        $ 0.00
  Current year net income    $ 3,212.00
  Withdrawals & distributions $ (1,500.00)
  Ending capital account     $ 26,712.00

Part III  Partner's Share of Current Year Income, Deductions, Credits, and Other Items
1 Ordinary business income (loss)            2,861.00
2 Net rental real estate income (loss)         351.00
3 Other net rental income (loss)                 0.00
4a Guaranteed payments for services              0.00
4b Guaranteed payments for capital               0.00
4c Total guaranteed payments                     0.00
5 Interest income                               14.00
6a Ordinary dividends                            0.00
6b Qualified dividends                           0.00
7 Royalties                                      0.00
8 Net short-term capital gain (loss)             0.00
9a Net long-term capital gain (loss)             0.00
10 Net section 1231 gain (loss)                  0.00
11 Other income (loss)
12 Section 179 deduction                         0.00
13 Other deductions
14 Self-employment earnings (loss)   A           0.00
19 Distributions                     A       1,500.00
20 Other information                 Z       2,861.00
"""


def bank_statement_text() -> str:
    j, h = JORDAN, HARBOR
    return f"""\
{h["name"]}
{h["street"]}, {h["city"]}   {h["phone"]}
Member Statement   March 1, 2025 - March 31, 2025

{j["full"]}
{j["street"]}
{j["city"]}

Member number: 0000-4471
Account: Free Checking   Account number: {JORDAN_CHECKING}
Routing number: {h["routing"]}

Summary
  Beginning balance  03/01/2025           $  4,210.55
  Deposits and credits (3)                $  6,902.18
  Withdrawals and debits (6)              $  5,118.40
  Ending balance     03/31/2025           $  5,994.33

Date        Description                                   Amount       Balance
03/03/2025  PAYROLL DEPOSIT ACME ROBOTICS INC          3,451.09      7,661.64
03/05/2025  ACH DEBIT HARBOR ST CU MORTGAGE PMT      -1,842.17      5,819.47
03/09/2025  POS CORNERSTONE HARDWARE CO                 -86.41      5,733.06
03/14/2025  ZELLE TO RILEY SAMPLE                      -250.00      5,483.06
03/17/2025  PAYROLL DEPOSIT ACME ROBOTICS INC          3,451.09      8,934.15
03/21/2025  ONLINE TRANSFER TO SAVINGS ****5432      -2,000.00      6,934.15
03/28/2025  CHECK 1043                                 -939.82      5,994.33
03/31/2025  DIVIDEND EARNED                               0.00      5,994.33

Questions? Call {h["phone"]} or visit any branch.
"""


def closing_statement_text() -> str:
    j, r, t = JORDAN, RILEY, TITLE_CO
    return f"""\
{t["name"]}
Closing Disclosure / Settlement Statement
File No. GK-2025-000417        Settlement date: 04/18/2025

Property: 1720 Quarry Road, Lakewood, CO 80226
Buyer:    {r["full"]}   SSN {r["ssn"]}
Seller:   {j["full"]}   SSN {j["ssn"]}
Lender:   {HARBOR["name"]}   Loan No. {JORDAN_MORTGAGE}

                                         Buyer                Seller
                                    Debit      Credit    Debit       Credit
Contract sales price            425,000.00                         425,000.00
Earnest money deposit                       10,000.00
New loan amount                            340,000.00
Payoff of first mortgage                                318,450.00
County taxes 01/01-04/18           1,231.40                          1,231.40
Title insurance premium            1,890.00
Recording fees                       142.00
Real estate commission                                   21,250.00
Totals                         428,263.40  350,000.00  339,700.00  426,231.40
Cash due from buyer                         78,263.40
Cash due to seller                                       86,531.40

WIRE INSTRUCTIONS (verify by phone before sending; we never change wire
instructions by email)
  Bank:            {HARBOR["name"]}
  Routing number:  {t["routing"]}
  Account name:    {t["name"]} Escrow Trust
  Account number:  {t["account"]}
  Reference:       GK-2025-000417 / {r["full"]}

Buyer signature: ______________________   Seller signature: ______________________
"""


def receipt_text() -> str:
    return """\
CORNERSTONE HARDWARE CO
2250 Mill Pond Rd, Springfield, OR 97477
(541) 555-0163

03/09/2025  14:22   Register 3   Cashier: Dana
---------------------------------------------
CORDLESS DRILL 18V            1 @  69.99   69.99
DRILL BIT SET 21PC            1 @  11.49   11.49
---------------------------------------------
SUBTOTAL                                   81.48
TAX 0.00%                                   0.00
TOTAL                                      81.48
VISA CREDIT   ************4242       USD   81.48
AUTH 000417   Chip read
Customer: JORDAN A EXAMPLE
Member rewards: ****5432

Thank you for shopping local!
"""


# ---------------------------------------------------------------------------
# OCR noise. Each function takes the clean text and returns what a mediocre
# scan through OCR would plausibly yield. The changes are fixed, not random,
# so the corpus is deterministic and tests can rely on specific damage.
# ---------------------------------------------------------------------------


def replace_once(text: str, old: str, new: str) -> str:
    if old not in text:
        raise SystemExit(f"generate.py: OCR noise target not found: {old!r}")
    return text.replace(old, new, 1)


def ocr_generic(text: str) -> str:
    """Collapse runs of spaces the way OCR flattens column layouts."""
    out_lines = []
    for line in text.splitlines():
        stripped = line.strip()
        while "     " in stripped:
            stripped = stripped.replace("     ", "   ")
        out_lines.append(stripped)
    return "\n".join(out_lines).strip() + "\n"


def noisy(text: str, edits: list[tuple[str, str]]) -> str:
    for old, new in edits:
        text = replace_once(text, old, new)
    return ocr_generic(text)


# ---------------------------------------------------------------------------
# Minimal PDF writer: Helvetica, uncompressed content streams, correct xref.
# ---------------------------------------------------------------------------


def pdf_literal(s: str) -> bytes:
    """A PDF literal string in PDFDocEncoding (ASCII here) with escapes."""
    b = s.encode("latin-1")
    b = b.replace(b"\\", b"\\\\").replace(b"(", b"\\(").replace(b")", b"\\)")
    return b"(" + b + b")"


def pdf_utf16(s: str) -> bytes:
    """A PDF hex string in UTF-16BE with BOM."""
    return b"<FEFF" + s.encode("utf-16-be").hex().upper().encode() + b">"


def text_stream(lines: list[str], size: int = 9, top: int = 760, leading: int = 11) -> bytes:
    ops = [b"BT", f"/F1 {size} Tf".encode(), f"{leading} TL".encode(), f"36 {top} Td".encode()]
    for line in lines:
        ops.append(pdf_literal(line) + b" Tj T*")
    ops.append(b"ET")
    return b"\n".join(ops) + b"\n"


def build_pdf(objects: list[bytes]) -> bytes:
    """objects[i] is the body of object i+1. Object 1 must be the catalog."""
    out = bytearray(b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n")
    offsets = []
    for i, body in enumerate(objects, start=1):
        offsets.append(len(out))
        out += f"{i} 0 obj\n".encode() + body + b"\nendobj\n"
    xref_at = len(out)
    out += f"xref\n0 {len(objects) + 1}\n".encode()
    out += b"0000000000 65535 f \n"
    for off in offsets:
        out += f"{off:010d} 00000 n \n".encode()
    out += f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n".encode()
    return bytes(out)


def stream_obj(data: bytes) -> bytes:
    return f"<< /Length {len(data)} >>\nstream\n".encode() + data + b"\nendstream"


FONT = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"


def text_pdf(text: str) -> bytes:
    """One-page PDF whose text layer is the given text, line by line."""
    lines = text.rstrip("\n").split("\n")
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
        FONT,
        stream_obj(text_stream(lines, size=8, leading=10)),
    ]
    return build_pdf(objects)


def scan_pdf() -> bytes:
    """One-page PDF with no text layer: just grey boxes, like a bad scan."""
    boxes = b"0.85 g\n36 600 540 150 re f\n36 420 260 160 re f\n316 420 260 160 re f\n"
    objects = [
        b"<< /Type /Catalog /Pages 2 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << >> /Contents 4 0 R >>",
        stream_obj(boxes),
    ]
    return build_pdf(objects)


# Field mapping for the fillable W-2. The IRS fillable W-2 uses opaque names
# like f2_01[0]; we use readable ones and record the box each stands for.
# Kept in sync with samples/README.md.
FILLABLE_W2_FIELDS = [
    # (partial name under parent "w2", W-2 box, value, encoding)
    ("box_a_ssn", "a", RILEY["ssn"], "pdfdoc"),
    ("box_b_ein", "b", BLUEFIN["ein"], "pdfdoc"),
    ("box_c_employer", "c", f"{BLUEFIN['name']}, {BLUEFIN['street']}, {BLUEFIN['city']}", "pdfdoc"),
    ("box_e_name", "e", "Riley Sample Ñandú", "utf16"),
    ("box_f_address", "f", f"{RILEY['street']}, {RILEY['city']}", "pdfdoc"),
    ("box_1_wages", "1", "96,400.00", "pdfdoc"),
    ("box_2_fed_withheld", "2", "13,880.00", "pdfdoc"),
]
FILLABLE_W2_CHOICE = ("box_15_state", "15", "CO", ["CO", "OR", "WA"])


def fillable_w2_pdf() -> bytes:
    """A W-2 with an AcroForm. Static labels are in the text layer; the values
    live only in the form fields. Fields hang under one parent field "w2" so
    readers must walk /Kids to find them (fully qualified: w2.box_a_ssn)."""
    labels = [
        "Form W-2 Wage and Tax Statement 2025 (fillable)",
        "a Employee's social security number",
        "b Employer identification number (EIN)",
        "c Employer's name, address, and ZIP code",
        "e Employee's first name and initial    Last name",
        "f Employee's address and ZIP code",
        "1 Wages, tips, other compensation",
        "2 Federal income tax withheld",
        "15 State",
    ]
    # Object numbers:
    # 1 catalog, 2 pages, 3 page, 4 font, 5 content, 6 acroform, 7 parent field,
    # 8.. kid fields (text fields), last = choice field
    n_kids = len(FILLABLE_W2_FIELDS) + 1
    first_kid = 8
    kid_refs = " ".join(f"{first_kid + i} 0 R" for i in range(n_kids))
    objects: list[bytes] = [
        b"<< /Type /Catalog /Pages 2 0 R /AcroForm 6 0 R >>",
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] "
        b"/Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R "
        + f"/Annots [{kid_refs}] >>".encode(),
        FONT,
        stream_obj(text_stream(labels, size=9, leading=40)),
        b"<< /Fields [7 0 R] /NeedAppearances true /DA (/Helv 0 Tf 0 g) "
        b"/DR << /Font << /Helv 4 0 R >> >> >>",
        b"<< /T (w2) /Kids [" + kid_refs.encode() + b"] >>",
    ]
    y = 735
    for name, _box, value, enc in FILLABLE_W2_FIELDS:
        v = pdf_utf16(value) if enc == "utf16" else pdf_literal(value)
        objects.append(
            b"<< /Type /Annot /Subtype /Widget /FT /Tx /Parent 7 0 R /P 3 0 R "
            + f"/T {pdf_literal(name).decode('latin-1')} ".encode()
            + b"/V "
            + v
            + f" /Rect [300 {y} 576 {y + 14}] /DA (/Helv 9 Tf 0 g) /F 4 >>".encode()
        )
        y -= 40
    cname, _box, cval, opts = FILLABLE_W2_CHOICE
    opt_arr = b" ".join(pdf_literal(o) for o in opts)
    objects.append(
        b"<< /Type /Annot /Subtype /Widget /FT /Ch /Ff 131072 /Parent 7 0 R /P 3 0 R "
        + b"/T "
        + pdf_literal(cname)
        + b" /V "
        + pdf_literal(cval)
        + b" /Opt ["
        + opt_arr
        + f"] /Rect [300 {y} 400 {y + 14}] /DA (/Helv 9 Tf 0 g) /F 4 >>".encode()
    )
    return build_pdf(objects)


# ---------------------------------------------------------------------------
# A tiny valid PNG: the "photo" of the receipt. Its text comes from the OCR
# sidecar; the pixels are a grey gradient.
# ---------------------------------------------------------------------------


def tiny_png(width: int = 16, height: int = 24) -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    raw = b"".join(b"\x00" + bytes((200 + (x + y) % 40) for x in range(width)) for y in range(height))
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 0, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


# ---------------------------------------------------------------------------


def ocr_json(text: str) -> str:
    return json.dumps({"pages": [{"page": 1, "text": text}]}, indent=2, ensure_ascii=False) + "\n"


def main(out: Path = OUT) -> None:
    out.mkdir(parents=True, exist_ok=True)

    w2_acme = w2_text(
        ACME,
        JORDAN,
        {
            "1": "84,250.00",
            "2": "11,912.40",
            "3": "88,750.00",
            "4": "5,502.50",
            "5": "88,750.00",
            "6": "1,286.88",
            "12a": "4,500.00",
            "14": "OR STT 63.19",
            "16": "84,250.00",
            "17": "6,318.75",
        },
        control="A0042-2025",
    )

    docs: dict[str, tuple[str, str]] = {
        "w2-acme-2025": (
            w2_acme,
            noisy(
                w2_acme,
                [
                    (JORDAN["ssn"], JORDAN["ssn"].replace("-", "")),  # dropped separators
                    ("84,250.00", "84,25O.OO"),  # O for 0
                    ("Employer identification number", "Employer identiﬁcation nurnber"),
                ],
            ),
        ),
        "1099-int-harbor-2025": (
            f1099_int_text(),
            noisy(
                f1099_int_text(),
                [
                    (HARBOR["ein"], HARBOR["ein"].replace("-", " ")),
                    ("412.88", "4l2.88"),
                    (JORDAN_SAVINGS_INT_ACCT, "O0098765432"),
                ],
            ),
        ),
        "1099-nec-bluefin-2025": (
            f1099_nec_text(),
            noisy(
                f1099_nec_text(),
                [
                    (RILEY["ssn"], "666 48-2210"),
                    ("18,500.00", "18,5OO.00"),
                ],
            ),
        ),
        "1099-div-northwind-2025": (
            f1099_div_text(),
            noisy(
                f1099_div_text(),
                [
                    (NORTHWIND["ein"], NORTHWIND["ein"].replace("-", "")),
                    ("1,284.16", "1,284.l6"),
                    (RILEY_BROKERAGE, "NW-OOOO-5521"),
                ],
            ),
        ),
        "1098-harbor-2025": (
            f1098_text(),
            noisy(
                f1098_text(),
                [
                    (JORDAN["ssn"], "9OO-12-3456"),
                    ("318,450.00", "318.450.00"),
                ],
            ),
        ),
        "k1-tidewater-2025": (
            k1_text(),
            noisy(
                k1_text(),
                [
                    (JORDAN["ssn"], "900-12 3456"),
                    (TIDEWATER["ein"], "78-l357924"),
                    ("2,861.00", "2,86l.00"),
                ],
            ),
        ),
        "bank-statement-harbor-2025-03": (
            bank_statement_text(),
            noisy(
                bank_statement_text(),
                [
                    (f"Account number: {JORDAN_CHECKING}", f"Account nurnber:{JORDAN_CHECKING}"),
                    (f"Routing number: {HARBOR['routing']}", "Routing number: 1234567B9"),
                    ("5,994.33", "5,994.3З"),
                ],
            ),
        ),
        "closing-statement-granite-2025": (
            closing_statement_text(),
            noisy(
                closing_statement_text(),
                [
                    # The wire block breaks across lines and loses its labels'
                    # alignment, as OCR of a two-column scan often does.
                    (
                        f"  Routing number:  {TITLE_CO['routing']}\n",
                        f"  Routing num-\nber: {TITLE_CO['routing'][:5]}\n{TITLE_CO['routing'][5:]}\n",
                    ),
                    (
                        f"  Account number:  {TITLE_CO['account']}\n",
                        f"  Account number:  {TITLE_CO['account'][:9]}\n  {TITLE_CO['account'][9:]}\n",
                    ),
                    (RILEY["ssn"], RILEY["ssn"].replace("-", "")),
                    ("425,000.00", "425,0OO.00"),
                ],
            ),
        ),
        "receipt-cornerstone-2025-03-09": (
            receipt_text(),
            noisy(
                receipt_text(),
                [
                    ("************4242", "**** **** **** 4242"),
                    ("81.48\nAUTH", "8l.48\nAUTH"),
                    ("AUTH 000417", "AUTH OOO417"),
                ],
            ),
        ),
    }

    written: list[Path] = []
    for stem, (clean, ocr) in docs.items():
        p = out / f"{stem}.txt"
        p.write_text(clean, encoding="utf-8")
        written.append(p)
        p = out / f"{stem}.ocr.json"
        p.write_text(ocr_json(ocr), encoding="utf-8")
        written.append(p)

    binaries = {
        # Text-layer PDFs of the same documents as the .txt files.
        "w2-acme-2025.pdf": text_pdf(docs["w2-acme-2025"][0]),
        "1099-int-harbor-2025.pdf": text_pdf(docs["1099-int-harbor-2025"][0]),
        # Fillable W-2 with AcroForm fields (values only in the fields).
        "w2-bluefin-2025-fillable.pdf": fillable_w2_pdf(),
        # A scan: no text layer. Its OCR output is the .ocr.json beside it.
        "w2-acme-2025-scan.pdf": scan_pdf(),
        # A phone photo of the receipt. OCR output in the sidecar.
        "receipt-cornerstone-2025-03-09-photo.png": tiny_png(),
    }
    for name, data in binaries.items():
        p = out / name
        p.write_bytes(data)
        written.append(p)

    (out / "w2-acme-2025-scan.ocr.json").write_text(ocr_json(docs["w2-acme-2025"][1]), encoding="utf-8")
    written.append(out / "w2-acme-2025-scan.ocr.json")
    (out / "receipt-cornerstone-2025-03-09-photo.ocr.json").write_text(
        ocr_json(docs["receipt-cornerstone-2025-03-09"][1]), encoding="utf-8"
    )
    written.append(out / "receipt-cornerstone-2025-03-09-photo.ocr.json")

    total = sum(p.stat().st_size for p in written)
    print(f"wrote {len(written)} files, {total} bytes, to {out}")


if __name__ == "__main__":
    main()

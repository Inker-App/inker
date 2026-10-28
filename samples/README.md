# samples

Everything in this directory is synthetic. No document here was ever a real
person's, employer's or bank's. Never add a real document, scan or photo, even
redacted.

## How the corpus is made

`samples/synthetic/` is written by `generate.py`, a dependency-free Python
script. It is deterministic: running it twice gives byte-identical files.

```
python3 -I samples/generate.py       # regenerate samples/synthetic/
python3 -I samples/test_generate.py  # check the corpus, including that it is current
```

Edit `generate.py`, not the generated files. CI fails if the checked-in
corpus differs from what the generator writes.

## The synthetic values

| What | Values | Why they are safe |
| --- | --- | --- |
| SSNs | `900-12-3456` (Jordan A Example), `666-48-2210` (Riley Sample) | Areas 9xx and 666 are never issued |
| EINs | `07-…` Acme Robotics Inc, `89-…` Bluefin Analytics LLC, `19-…` Harbor Street Credit Union, `78-…` Tidewater Partners LP, `70-…` Northwind Index Fund | Prefixes the IRS does not assign |
| Routing numbers | `123456789`, `987654321` | Fail the ABA 3-7-1 checksum |
| Card | `************4242` | Masked, as on a real receipt |
| People, companies, addresses | invented | |

Jordan's SSN appears on the W-2, the K-1, the 1099-INT, the 1098 and the
closing statement, so tests can check that one value maps to one token across
documents. Riley's appears on the 1099-NEC, the 1099-DIV, the closing
statement and the fillable W-2.

## Files

For each of W-2, 1099-INT, 1099-NEC, 1099-DIV, 1098, Schedule K-1 (Form
1065), a bank statement, a closing statement and a receipt:

- `<name>.txt`: clean text rendition with the IRS box labels and layout.
- `<name>.ocr.json`: what OCR of a scan of the same document might return,
  as `{ "pages": [ { "page": 1, "text": "..." } ] }`. The noise is fixed, not
  random: dropped separators in SSNs and EINs, `O` for `0` and `l` for `1` in
  a few amounts, a `ﬁ` ligature, and in the closing statement the wire
  instructions break across lines mid-number.

Plus:

| File | What it tests |
| --- | --- |
| `w2-acme-2025.pdf`, `1099-int-harbor-2025.pdf` | One-page PDFs with a text layer (Helvetica, uncompressed), same text as the `.txt` |
| `w2-bluefin-2025-fillable.pdf` | AcroForm reading: labels in the text layer, values only in form fields |
| `w2-acme-2025-scan.pdf` + `.ocr.json` | A PDF with no text layer, so extraction falls back to OCR |
| `receipt-cornerstone-2025-03-09-photo.png` + `.ocr.json` | An image, OCR only |

The `.ocr.json` sidecars are what `inker_extract::FixtureOcr` reads on Linux
in place of Apple Vision: for `dir/name.ext` it reads `dir/name.ocr.json`.

## Fillable W-2 field map

The IRS fillable W-2 uses opaque names such as `f2_01[0]`. This sample uses
readable partial names under one parent field `w2`, so readers must walk
`/Kids` and return fully qualified names:

| Field (fully qualified) | W-2 box | Type | Value encoding |
| --- | --- | --- | --- |
| `w2.box_a_ssn` | a Employee's social security number | text | PDFDocEncoding |
| `w2.box_b_ein` | b Employer identification number (EIN) | text | PDFDocEncoding |
| `w2.box_c_employer` | c Employer's name, address, and ZIP code | text | PDFDocEncoding |
| `w2.box_e_name` | e Employee's first name and initial, Last name | text | UTF-16BE (`Riley Sample Ñandú`) |
| `w2.box_f_address` | f Employee's address and ZIP code | text | PDFDocEncoding |
| `w2.box_1_wages` | 1 Wages, tips, other compensation | text | PDFDocEncoding |
| `w2.box_2_fed_withheld` | 2 Federal income tax withheld | text | PDFDocEncoding |
| `w2.box_15_state` | 15 State | choice (`CO`, `OR`, `WA`) | PDFDocEncoding |

## The PII check

`scripts/check-pii.py` skips the contents of `samples/synthetic/` (and allows
PDFs and images there only). Everything else in `samples/`, including this
README and the generator, is scanned like any other file, so keep real-shaped
values out of them.

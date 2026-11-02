"""Tests for make-dmg-background.py: the layout, the octopus, the PNG."""

import importlib.util
import json
import pathlib
import struct
import tempfile
import unittest
import zlib

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("make_dmg_background", HERE / "make-dmg-background.py")
bg = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bg)


def decode_png(data):
    """(width, height, rows of (r, g, b)) from an 8-bit RGB PNG with filter 0."""
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    pos, chunks = 8, {}
    while pos < len(data):
        (n,) = struct.unpack(">I", data[pos : pos + 4])
        kind = data[pos + 4 : pos + 8]
        chunks.setdefault(kind, b"")
        chunks[kind] += data[pos + 8 : pos + 8 + n]
        pos += 12 + n
    w, h, depth, ctype = struct.unpack(">IIBB", chunks[b"IHDR"][:10])
    assert (depth, ctype) == (8, 2)
    raw = zlib.decompress(chunks[b"IDAT"])
    stride = 1 + 3 * w
    rows = []
    for y in range(h):
        line = raw[y * stride : (y + 1) * stride]
        assert line[0] == 0
        rows.append([tuple(line[1 + 3 * x : 4 + 3 * x]) for x in range(w)])
    return w, h, rows


class LayoutTests(unittest.TestCase):
    def test_icons_sit_on_one_row_with_inker_left_of_applications(self):
        L = bg.LAYOUT
        self.assertEqual(L["inker"][1], L["applications"][1])
        self.assertLess(L["inker"][0], L["applications"][0])
        # Both icons, with their labels, fit inside the window.
        half = L["icon_size"] / 2
        for x, y in (L["inker"], L["applications"]):
            self.assertGreaterEqual(x - half, 0)
            self.assertLessEqual(x + half, L["width"])
            self.assertGreaterEqual(y - half, 0)
            self.assertLessEqual(y + half + 2 * L["text_size"], L["height"])

    def test_arrow_runs_between_the_icons_on_their_row(self):
        L = bg.LAYOUT
        xs = [x for x, _, _ in bg.arrow_dots()]
        ys = {y for _, y, _ in bg.arrow_dots()}
        self.assertTrue(xs)
        self.assertGreater(min(xs), L["inker"][0] + L["icon_size"] / 2)
        self.assertLess(max(xs), L["applications"][0] - L["icon_size"] / 2)
        self.assertIn(L["inker"][1], ys)

    def test_eyes_look_toward_applications(self):
        lx, ly = bg.look_at(bg.LAYOUT["applications"])
        self.assertGreater(lx, 0)
        self.assertLessEqual(abs(lx), 1)
        self.assertLessEqual(abs(ly), 1)


class OctopusTests(unittest.TestCase):
    def test_sdf_is_negative_inside_the_head_and_positive_far_away(self):
        tentacles = bg.build(1.7)
        inside, _, _ = bg.sdf(0.0, -0.55, 0.0, tentacles)
        outside, _, _ = bg.sdf(0.0, 3.0, 0.0, tentacles)
        self.assertLess(inside, 0)
        self.assertGreater(outside, 0)

    def test_eye_material_is_reported_at_the_eye_centres(self):
        tentacles = bg.build(1.7)
        for ex, ey, ez in bg.EYE:
            _, material, eye = bg.sdf(ex, ey, ez, tentacles)
            self.assertEqual(material, bg.MATERIAL_EYE)
            self.assertEqual(bg.EYE[eye], (ex, ey, ez))

    def test_render_yields_dots_only_where_the_octopus_is(self):
        dots = bg.octopus_dots(width=120, height=80, cell=8, centre=(60, 50), scale=22, time=1.7, look=(0, 0))
        self.assertTrue(dots)
        xs = [x for x, _, _ in dots]
        self.assertGreater(min(xs), 0)
        self.assertLess(max(xs), 120)
        # The brightest dots fill their cell, no dot spills past it.
        for _, _, r in dots:
            self.assertGreater(r, 0)
            self.assertLessEqual(r, 4 * 1.05 + 1e-9)


class PngTests(unittest.TestCase):
    def test_png_has_the_window_size_at_each_scale(self):
        cx, cy, r = 10.5, 10.5, 3.7
        for scale in (1, 2):
            w, h, rows = decode_png(bg.png(40, 20, [(cx, cy, r)], scale, bg.BACKGROUND, bg.PAPER))
            self.assertEqual((w, h), (40 * scale, 20 * scale))
            self.assertEqual(rows[0][0], (0x0D, 0x0F, 0x10))
            self.assertEqual(rows[int(cy * scale)][int(cx * scale)], (0xE6, 0xE4, 0xDE))
            # The dot's edge is anti-aliased: the pixel the rim crosses is a blend.
            edge = rows[int(cy * scale)][int((cx + r) * scale)]
            self.assertNotEqual(edge, (0x0D, 0x0F, 0x10))
            self.assertNotEqual(edge, (0xE6, 0xE4, 0xDE))

    def test_main_writes_both_images_and_the_layout(self):
        with tempfile.TemporaryDirectory() as d:
            out = pathlib.Path(d)
            bg.main(["--out", str(out), "--cell", "24", "--quick"])
            w1, h1, _ = decode_png((out / "background.png").read_bytes())
            w2, h2, _ = decode_png((out / "background@2x.png").read_bytes())
            self.assertEqual((w1, h1), (bg.LAYOUT["width"], bg.LAYOUT["height"]))
            self.assertEqual((w2, h2), (2 * w1, 2 * h1))
            layout = json.loads((out / "layout.json").read_text())
            self.assertEqual(layout, bg.LAYOUT)


class SettingsTests(unittest.TestCase):
    """dmg/settings.py, run the way dmgbuild runs it: exec with `defines`."""

    def load(self):
        dmg = HERE.parent / "dmg"
        ns = {"defines": {"app": "/x/INKER.app", "background": str(dmg / "background.png"), "layout": str(dmg / "layout.json"), "icon": "/x/AppIcon.icns"}}
        exec(compile((dmg / "settings.py").read_text(), "settings.py", "exec"), ns, ns)
        return ns

    def test_icons_land_where_the_picture_expects_them(self):
        ns = self.load()
        self.assertEqual(ns["icon_locations"], {"INKER.app": tuple(bg.LAYOUT["inker"]), "Applications": tuple(bg.LAYOUT["applications"])})
        self.assertEqual(ns["window_rect"][1], (bg.LAYOUT["width"], bg.LAYOUT["height"]))
        self.assertEqual(ns["icon_size"], bg.LAYOUT["icon_size"])
        self.assertEqual(ns["text_size"], bg.LAYOUT["text_size"])

    def test_window_is_bare_and_the_app_sits_beside_an_applications_link(self):
        ns = self.load()
        self.assertEqual(ns["files"], ["/x/INKER.app"])
        self.assertEqual(ns["symlinks"], {"Applications": "/Applications"})
        self.assertEqual(ns["icon"], "/x/AppIcon.icns")
        self.assertEqual(ns["format"], "UDZO")
        for flag in ("show_status_bar", "show_toolbar", "show_pathbar", "show_sidebar", "show_tab_view"):
            self.assertFalse(ns[flag], flag)

    def test_committed_images_match_the_layout(self):
        dmg = HERE.parent / "dmg"
        w, h, _ = decode_png((dmg / "background.png").read_bytes())
        self.assertEqual((w, h), (bg.LAYOUT["width"], bg.LAYOUT["height"]))
        w2, h2, _ = decode_png((dmg / "background@2x.png").read_bytes())
        self.assertEqual((w2, h2), (2 * w, 2 * h))
        self.assertEqual(json.loads((dmg / "layout.json").read_text()), bg.LAYOUT)


if __name__ == "__main__":
    unittest.main()

"""Tests for make-icon.py: the mark, the template geometry, the colours."""

import importlib.util
import pathlib
import re
import unittest

HERE = pathlib.Path(__file__).parent
spec = importlib.util.spec_from_file_location("make_icon", HERE / "make-icon.py")
make_icon = importlib.util.module_from_spec(spec)
spec.loader.exec_module(make_icon)


class IconTests(unittest.TestCase):
    def test_every_inked_cell_is_a_dot(self):
        inked = sum(line.count("#") for line in make_icon.MARK)
        self.assertEqual(inked, 35)
        self.assertEqual(len(make_icon.dots()), inked)

    def test_dots_sit_inside_the_square_and_touch_their_neighbours(self):
        lo, hi = make_icon.MARGIN, make_icon.MARGIN + make_icon.SQUARE
        for cx, cy, r in make_icon.dots():
            self.assertGreaterEqual(cx - r, lo)
            self.assertLessEqual(cx + r, hi)
            self.assertGreaterEqual(cy - r, lo)
            self.assertLessEqual(cy + r, hi)
        xs = sorted({cx for cx, _, _ in make_icon.dots()})
        r = make_icon.dots()[0][2]
        self.assertAlmostEqual(xs[1] - xs[0], 2 * r, places=1)

    def test_svg_follows_apples_template(self):
        s = make_icon.svg()
        self.assertIn('viewBox="0 0 1024 1024"', s)
        self.assertIn('x="100" y="100" width="824" height="824" rx="185"', s)
        self.assertIn('fill="#0D0F10"', s)
        self.assertIn('fill="#E6E4DE"', s)
        self.assertEqual(len(re.findall(r"<circle ", s)), 35)

    def test_html_wraps_the_svg_on_a_transparent_1024_page(self):
        h = make_icon.html()
        self.assertIn("background:transparent", h)
        self.assertIn("width:1024px;height:1024px", h)
        self.assertIn(make_icon.svg().strip(), h)


if __name__ == "__main__":
    unittest.main()

# dmgbuild settings for the INKER disk image: the window the user drags
# from. Icon positions and the window size come from layout.json, which
# scripts/make-dmg-background.py writes beside the background it draws, so
# the arrow in the picture and the icons Finder places agree.
#
#   dmgbuild -s app/dmg/settings.py -D app=build/INKER.app \
#       -D background=app/dmg/background.png [-D icon=build/AppIcon.icns] \
#       INKER INKER.dmg
#
# dmgbuild finds background@2x.png beside background.png and joins them into
# a Retina TIFF itself. scripts/make-dmg.sh runs this.

import json
import os

_layout = json.load(open(defines["layout"]))  # noqa: F821 (dmgbuild injects `defines`)

application = defines["app"]  # noqa: F821
appname = os.path.basename(application)

format = "UDZO"
size = None
files = [application]
symlinks = {"Applications": "/Applications"}

# The mounted volume's icon: the INKER mark, when the build passed one.
icon = defines.get("icon")  # noqa: F821
background = defines["background"]  # noqa: F821

show_status_bar = False
show_tab_view = False
show_toolbar = False
show_pathbar = False
show_sidebar = False
sidebar_width = 180

window_rect = ((160, 120), (_layout["width"], _layout["height"]))
default_view = "icon-view"
show_icon_preview = False
include_icon_view_settings = "auto"
include_list_view_settings = "auto"

arrange_by = None
grid_offset = (0, 0)
grid_spacing = 100
scroll_position = (0, 0)
label_pos = "bottom"
text_size = _layout["text_size"]
icon_size = _layout["icon_size"]
icon_locations = {
    appname: tuple(_layout["inker"]),
    "Applications": tuple(_layout["applications"]),
}

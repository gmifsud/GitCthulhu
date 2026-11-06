# Logos

- `git-cthulhu-icon.png` — the app icon, 256 px, taken from the 256 px frame of `app/assets/git_cthulhu.ico`: an ember circuit-tentacled Cthulhu on a charcoal rounded square ringed in ember, the name `GitCthulhu` set beneath it. The window, taskbar and installer icon. Do not redraw or recolour it; the header uses a typeset mark instead.
- `git-cthulhu-window-icon-64.png` — the 64 px window icon the running app sets (`gui_app/assets/icon-64.rgba`, converted losslessly to PNG). Same artwork, hinted for 64 px.
- `cthulhu.jpg` — the 1024 px source artwork the icon was cut from (`app/assets/cthulhu.jpg`). For splash or about screens only; never inline it in the chrome.

All three are raster and already on a dark ground: place them on `bg-app` or `bg-chrome`, never on a light surface.

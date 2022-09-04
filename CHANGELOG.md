## Unreleased (0.5.2)
- `ab_glyph` & `crossfont` titles will use gnome "titlebar-font" config if available.
- `ab_glyph` titles are now more consistent with `crossfont` titles both using system sans
  if no better font config is available.

## 0.5.1
- Use dbus org.freedesktop.portal.Settings to automatically choose light or dark theming.
- Double click detection fix.
- Apply button click on release instead of press.

## 0.5.0
- `title` feature got removed
- `ab_glyph` default feature got added (for `ab_glyph` based title rendering)
- `crossfont` feature got added (for `crossfont` based title rendering)
    - Can be enable like this: 
        ```toml
        sctk-adwaita = { default-features = false, features = ["crossfont"] }
        ```

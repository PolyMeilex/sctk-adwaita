# Adwaita-like SCTK Frame

|   |   |
|---|---|
|![active](https://i.imgur.com/WdO8e0i.png)|![hover](https://i.imgur.com/TkUq2WF.png)|
![inactive](https://i.imgur.com/MTFdSjK.png)|

### Dark mode:
![image](https://user-images.githubusercontent.com/20758186/169424673-3b9fa022-f112-4928-8360-305a714ba979.png)

## Title text: crossfont
Enable title text drawn with _crossfont_ crate with feature **title**. This adds a requirement on _freetype_.

```toml
sctk-adwaita = { features = ["title"] }
```

## Title text: ab_glyph
Alternatively title text may be drawn with _ab_glyph_ crate with feature **ab_glyph**. This requires no additional dynamically linked dependencies.

```toml
sctk-adwaita = { features = ["ab_glyph"] }
```

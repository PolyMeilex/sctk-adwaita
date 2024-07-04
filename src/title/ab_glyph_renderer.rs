//! Title renderer using ab_glyph.
//!
//! Requires no dynamically linked dependencies.
//!
//! Can fallback to a embedded Cantarell-Regular.ttf font (SIL Open Font Licence v1.1)
//! if the system font doesn't work.
use crate::title::{config, font_preference::FontPreference};
use ab_glyph::{point, Font, FontRef, Glyph, PxScale, PxScaleFont, ScaleFont, VariableFont};
use std::{fs::File, process::Command};
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};

const CANTARELL: &[u8] = include_bytes!("Cantarell-Regular.ttf");

#[derive(Debug)]
pub struct AbGlyphTitleText {
    title: String,
    font: Option<(memmap2::Mmap, FontPreference)>,
    original_px_size: f32,
    size: PxScale,
    color: Color,
    pixmap: Option<Pixmap>,
}

impl AbGlyphTitleText {
    pub fn new(color: Color) -> Self {
        let font_pref = config::titlebar_font().unwrap_or_default();
        let font_pref_pt_size = font_pref.pt_size;
        let font = font_file_matching(&font_pref)
            .and_then(|f| mmap(&f))
            .map(|mmap| (mmap, font_pref));

        let size = parse_font(&font)
            .pt_to_px_scale(font_pref_pt_size)
            .unwrap_or_else(|| {
                log::error!("invalid font units_per_em");
                PxScale { x: 17.6, y: 17.6 }
            });

        Self {
            title: <_>::default(),
            font,
            original_px_size: size.x,
            size,
            color,
            pixmap: None,
        }
    }

    pub fn update_scale(&mut self, scale: u32) {
        let new_scale = PxScale::from(self.original_px_size * scale as f32);
        if (self.size.x - new_scale.x).abs() > f32::EPSILON {
            self.size = new_scale;
            self.pixmap = self.render();
        }
    }

    pub fn update_title(&mut self, title: impl Into<String>) {
        let new_title = title.into();
        if new_title != self.title {
            self.title = new_title;
            self.pixmap = self.render();
        }
    }

    pub fn update_color(&mut self, color: Color) {
        if color != self.color {
            self.color = color;
            self.pixmap = self.render();
        }
    }

    pub fn pixmap(&self) -> Option<&Pixmap> {
        self.pixmap.as_ref()
    }

    /// Render returning the new `Pixmap`.
    fn render(&self) -> Option<Pixmap> {
        let font = parse_font(&self.font);
        let font = font.as_scaled(self.size);

        let glyphs: Vec<_> = self
            .layout(&font)
            .into_iter()
            .filter_map(|g| font.outline_glyph(g))
            .collect();

        // calc combined px bound coordinates of the rendered glyphs
        // Note: It is possible for min.x to be negative, e.g. the first glyph's
        //       outline extends a little out to the left further than the layout x origin 0.0
        let all_px_bounds = glyphs.iter().map(|g| g.px_bounds()).reduce(|mut b, next| {
            b.min.x = b.min.x.min(next.min.x);
            b.max.x = b.max.x.max(next.max.x);
            // min(0.0): consistently allocate enough for the whole ascent even
            //           if all glyphs don't need that much, makes positioning easier later.
            //           If removed the pixmap will be exactly sized, but we'd need a
            //           vertical offset to render, say "Tg" vs "gg", consistently
            b.min.y = b.min.y.min(next.min.y).min(0.0);
            b.max.y = b.max.y.max(next.max.y);
            b
        })?;

        let width = all_px_bounds.width() as _;
        let mut pixmap = Pixmap::new(width, all_px_bounds.height() as _)?;
        let pixels = pixmap.pixels_mut();

        for glyph in glyphs {
            let bounds = glyph.px_bounds();
            // calc top/left ords in pixmap space
            // pixmap-x=0 means the *left most pixel*, equivalent to
            // px_bounds.min.x which *may be non-zero* (and similarly with y)
            // so `- px_bounds.min` converts the left-most/top-most to 0
            let pixmap_left = bounds.min.x as u32 - all_px_bounds.min.x as u32;
            let pixmap_top = bounds.min.y as u32 - all_px_bounds.min.y as u32;
            glyph.draw(|x, y, c| {
                // `ab_glyph` may return values greater than 1.0, but they are defined to be
                // same as 1.0. For our purposes, we need to constrain this value.
                let c = c.min(1.0);

                let p_idx = (pixmap_top + y) * width + pixmap_left + x;
                let Some(pixel) = pixels.get_mut(p_idx as usize) else {
                    debug_assert!(
                        false,
                        "oob pixel: x={x} y={y} top={pixmap_top} left={pixmap_left}, w={width}"
                    );
                    return;
                };

                let old_alpha_u8 = pixel.alpha();

                let new_alpha = c + (old_alpha_u8 as f32 / 255.0);
                if let Some(px) = PremultipliedColorU8::from_rgba(
                    (self.color.red() * new_alpha * 255.0) as _,
                    (self.color.green() * new_alpha * 255.0) as _,
                    (self.color.blue() * new_alpha * 255.0) as _,
                    (new_alpha * 255.0) as _,
                ) {
                    *pixel = px;
                }
            })
        }

        Some(pixmap)
    }

    /// Simple single-line glyph layout starting from `(0, ascent)`.
    fn layout(&self, font: &PxScaleFont<impl Font>) -> Vec<Glyph> {
        let mut caret = point(0.0, font.ascent());
        let mut last_glyph: Option<Glyph> = None;
        let mut target = Vec::new();
        for c in self.title.chars() {
            if c.is_control() {
                continue;
            }
            let mut glyph = font.scaled_glyph(c);
            if let Some(previous) = last_glyph.take() {
                caret.x += font.kern(previous.id, glyph.id);
            }
            glyph.position = caret;

            last_glyph = Some(glyph.clone());
            caret.x += font.h_advance(glyph.id);

            target.push(glyph);
        }
        target
    }
}

/// Parse the memmapped system font or fallback to built-in cantarell.
fn parse_font(sys_font: &Option<(memmap2::Mmap, FontPreference)>) -> FontRef<'_> {
    match sys_font {
        Some((mmap, font_pref)) => {
            FontRef::try_from_slice(mmap)
                .map(|mut f| {
                    // basic "bold" handling for variable fonts
                    if font_pref
                        .style
                        .as_deref()
                        .map_or(false, |s| s.eq_ignore_ascii_case("bold"))
                    {
                        f.set_variation(b"wght", 700.0);
                    }
                    f
                })
                .unwrap_or_else(|_| {
                    // We control the default font, so I guess it's fine to unwrap it
                    #[allow(clippy::unwrap_used)]
                    FontRef::try_from_slice(CANTARELL).unwrap()
                })
        }
        // We control the default font, so I guess it's fine to unwrap it
        #[allow(clippy::unwrap_used)]
        _ => FontRef::try_from_slice(CANTARELL).unwrap(),
    }
}

/// Font-config without dynamically linked dependencies
fn font_file_matching(pref: &FontPreference) -> Option<File> {
    let mut pattern = pref.name.clone();
    if let Some(style) = &pref.style {
        pattern.push(':');
        pattern.push_str(style);
    }
    Command::new("fc-match")
        .arg("-f")
        .arg("%{file}")
        .arg(&pattern)
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .and_then(|path| File::open(path.trim()).ok())
}

fn mmap(file: &File) -> Option<memmap2::Mmap> {
    // Safety: System font files are not expected to be mutated during use
    unsafe { memmap2::Mmap::map(file).ok() }
}

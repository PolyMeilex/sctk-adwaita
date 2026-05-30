//! Title renderer using ab_glyph.
//!
//! Requires no dynamically linked dependencies.
//!
//! Can fallback to a embedded Cantarell-Regular.ttf font (SIL Open Font Licence v1.1)
//! if the system font doesn't work.
//!
//! Per-codepoint fallback faces (emoji, CJK, …) are discovered via `fc-match`;
//! see [`crate::title::font_fallback`].
use crate::title::{config, font_fallback::FallbackCache, font_preference::FontPreference};
use ab_glyph::{point, Font, FontRef, Glyph, PxScale, PxScaleFont, ScaleFont, VariableFont};
use std::{fs::File, process::Command};
use tiny_skia::{Color, Pixmap, PremultipliedColorU8};

const CANTARELL: &[u8] = include_bytes!("Cantarell-Regular.ttf");

#[derive(Debug)]
pub struct AbGlyphTitleText {
    title: String,
    font: Option<(memmap2::Mmap, FontPreference)>,
    fallbacks: FallbackCache,
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
            fallbacks: FallbackCache::default(),
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
            self.discover_fallbacks();
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

    /// Load additional faces for any title codepoint not covered by the primary
    /// font or any previously-loaded fallback.
    fn discover_fallbacks(&mut self) {
        let primary = parse_font(&self.font);
        self.fallbacks.extend(&self.title, |c, loaded| {
            covers(&primary, c)
                || loaded
                    .iter()
                    .any(|m| FontRef::try_from_slice(m).is_ok_and(|fr| covers(&fr, c)))
        });
    }

    /// Render returning the new `Pixmap`.
    fn render(&self) -> Option<Pixmap> {
        let primary = parse_font(&self.font).into_scaled(self.size);
        let fallbacks: Vec<_> = self
            .fallbacks
            .fonts
            .iter()
            .filter_map(|m| FontRef::try_from_slice(m).ok())
            .map(|fr| fr.into_scaled(self.size))
            .collect();

        let placements = layout(&primary, &fallbacks, &self.title);
        let last = placements.last()?;
        let last_font = font_at(&primary, &fallbacks, last.font_idx);
        // + 2 because ab_glyph likes to draw outside of its area,
        // so we add 1px border around the pixmap
        let width = (last.glyph.position.x + last_font.h_advance(last.glyph.id)).ceil() as u32 + 2;
        let height = primary.height().ceil() as u32 + 2;

        let mut pixmap = Pixmap::new(width, height)?;

        let pixels = pixmap.pixels_mut();

        for Placement { font_idx, glyph } in placements {
            let font = font_at(&primary, &fallbacks, font_idx);
            if let Some(outline) = font.outline_glyph(glyph) {
                let bounds = outline.px_bounds();
                let left = bounds.min.x as u32;
                let top = bounds.min.y as u32;
                outline.draw(|x, y, c| {
                    // `ab_glyph` may return values greater than 1.0, but they are defined to be
                    // same as 1.0. For our purposes, we need to constrain this value.
                    let c = c.min(1.0);

                    // offset the index by 1, so it is in the center of the pixmap
                    let p_idx = (top + y + 1) * width + (left + x + 1);
                    let Some(pixel) = pixels.get_mut(p_idx as usize) else {
                        // Expected when a fallback glyph (emoji, CJK) draws
                        // outside the primary font's height bound.
                        log::debug!("Ignoring out of range pixel (pixel id: {p_idx})");
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
        }

        Some(pixmap)
    }
}

/// One positioned glyph plus the index of the font it came from
/// (`0` = primary, `1..=n` = `fallbacks[idx-1]`).
struct Placement {
    font_idx: usize,
    glyph: Glyph,
}

fn font_at<'a, F: Font>(
    primary: &'a PxScaleFont<F>,
    fallbacks: &'a [PxScaleFont<F>],
    idx: usize,
) -> &'a PxScaleFont<F> {
    idx.checked_sub(1)
        .and_then(|i| fallbacks.get(i))
        .unwrap_or(primary)
}

/// Pick the first font (primary, then fallbacks) that has a glyph for `c`.
/// Returns the font index and the scaled glyph. Falls back to the primary
/// (`.notdef`) when no font covers `c`.
fn select<F: Font>(
    primary: &PxScaleFont<F>,
    fallbacks: &[PxScaleFont<F>],
    c: char,
) -> (usize, Glyph) {
    let g = primary.scaled_glyph(c);
    if g.id.0 != 0 {
        return (0, g);
    }
    for (i, f) in fallbacks.iter().enumerate() {
        let g = f.scaled_glyph(c);
        if g.id.0 != 0 {
            return (i + 1, g);
        }
    }
    (0, primary.scaled_glyph(c))
}

fn covers<F: Font>(font: &F, c: char) -> bool {
    font.glyph_id(c).0 != 0
}

/// Single-line layout across the primary + fallback fonts. Line metrics come
/// from the primary; per-glyph advance comes from the covering font. Kerning
/// is applied only between adjacent glyphs from the same font.
fn layout<F: Font>(
    primary: &PxScaleFont<F>,
    fallbacks: &[PxScaleFont<F>],
    title: &str,
) -> Vec<Placement> {
    let mut caret = point(0.0, primary.ascent());
    let mut last: Option<(usize, Glyph)> = None;
    let mut out = Vec::new();
    for c in title.chars() {
        if c.is_control() {
            continue;
        }
        let (idx, mut glyph) = select(primary, fallbacks, c);
        let font = font_at(primary, fallbacks, idx);
        if let Some((prev_idx, prev_glyph)) = last.take() {
            if prev_idx == idx {
                caret.x += font.kern(prev_glyph.id, glyph.id);
            }
        }
        glyph.position = caret;
        caret.x += font.h_advance(glyph.id);
        last = Some((idx, glyph.clone()));
        out.push(Placement { font_idx: idx, glyph });
    }
    out
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
                        .is_some_and(|s| s.eq_ignore_ascii_case("bold"))
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn new_renderer(title: &str) -> AbGlyphTitleText {
        let mut renderer = AbGlyphTitleText {
            title: String::new(),
            font: None, // forces fallback to bundled Cantarell
            fallbacks: FallbackCache::default(),
            original_px_size: 17.6,
            size: PxScale { x: 17.6, y: 17.6 },
            color: Color::BLACK,
            pixmap: None,
        };
        renderer.update_title(title);
        renderer
    }

    /// Regression: '★' is not in Cantarell. Before this fix it silently
    /// rendered as Cantarell's `.notdef` glyph. The fix must load a fallback
    /// face that has it. Verified separately on pre-fix code (rendering a
    /// missing codepoint and a different missing codepoint produced
    /// byte-identical pixmaps — both `.notdef`).
    #[test]
    fn loads_fallback_for_missing_codepoint() {
        let renderer = new_renderer("★");
        assert!(
            !renderer.fallbacks.fonts.is_empty(),
            "'★' is not in Cantarell — a fallback face must have been loaded"
        );
    }

    /// Pure-ASCII titles must not trigger any fallback lookups.
    #[test]
    fn no_fallback_for_ascii_title() {
        let renderer = new_renderer("hello");
        assert!(
            renderer.fallbacks.fonts.is_empty(),
            "ASCII fits in Cantarell, no fallback should be loaded"
        );
    }
}

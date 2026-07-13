use crate::title::{config, font_preference::FontPreference};
use cosmic_text::{
    Attrs, AttrsList, BufferLine, Color as CosmicColor, Family, FontSystem, Hinting, LineEnding,
    Shaping, Style, SwashCache, Weight, Wrap,
};
use tiny_skia::{Color, ColorU8, Pixmap, PremultipliedColorU8};

// Use arbitrarily large width, then calculate max/min x from resulting glyphs
const MAX_WIDTH: f32 = 1024.0 * 1024.0;

// Title text never contains tabs; this is just the value `shape()` requires.
const TAB_WIDTH: u16 = 8;

fn attrs_from_font_pref(font_preference: &FontPreference) -> Attrs<'_> {
    let mut attrs = Attrs::new().family(Family::Name(&font_preference.name));
    // The GNOME `titlebar-font` style is a free-form descriptor such as
    // "Bold", "Italic", or "Bold Italic"; map the keywords we understand onto
    // cosmic-text weight/style and ignore the rest.
    if let Some(style) = &font_preference.style {
        let style = style.to_ascii_lowercase();
        if style.contains("bold") {
            attrs = attrs.weight(Weight::BOLD);
        }
        if style.contains("italic") {
            attrs = attrs.style(Style::Italic);
        } else if style.contains("oblique") {
            attrs = attrs.style(Style::Oblique);
        }
    }
    attrs
}

pub struct CosmicTextTitleText {
    buffer_line: BufferLine,
    cache: SwashCache,
    color: Color,
    scale: u32,
    pixmap: Option<Pixmap>,
    font_pref: FontPreference,
    font_system: FontSystem,
}

impl std::fmt::Debug for CosmicTextTitleText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TitleText")
            .field("color", &self.color)
            .field("scale", &self.scale)
            .field("pixmap", &self.pixmap)
            .field("font_pref", &self.font_pref)
            .finish()
    }
}

impl CosmicTextTitleText {
    pub fn new(color: Color) -> Self {
        let buffer_line = BufferLine::new(
            "",
            LineEnding::default(),
            AttrsList::new(&Attrs::new()),
            Shaping::Advanced,
        );
        let cache = SwashCache::new();
        let font_pref = config::titlebar_font().unwrap_or_default();
        Self {
            buffer_line,
            cache,
            color,
            scale: 1,
            pixmap: None,
            font_pref,
            font_system: FontSystem::new(),
        }
    }

    pub fn update_scale(&mut self, scale: u32) {
        self.scale = scale;
        self.update_pixmap();
    }

    pub fn update_title<S: Into<String>>(&mut self, title: S) {
        let attrs = attrs_from_font_pref(&self.font_pref);
        self.buffer_line
            .set_text(title.into(), LineEnding::default(), AttrsList::new(&attrs));
        self.update_pixmap();
    }

    pub fn update_color(&mut self, color: Color) {
        self.color = color;
        self.update_pixmap();
    }

    pub fn pixmap(&self) -> Option<&Pixmap> {
        self.pixmap.as_ref()
    }

    fn update_pixmap(&mut self) {
        self.pixmap = None;

        let shape_line = self.buffer_line.shape(&mut self.font_system, TAB_WIDTH);

        // Font size in device pixels: points -> pixels at 96 DPI, scaled by
        // the integer output scale. This is the em size handed to `layout()`,
        // NOT the line box height.
        let font_size = self.scale as f32 * self.font_pref.pt_size * 96.0 / 72.0;
        let layout_lines = shape_line.layout(
            font_size,
            Some(MAX_WIDTH),
            Wrap::Word,
            None,
            None,
            Hinting::Disabled,
        );
        let Some(layout_line) = layout_lines.first() else {
            return;
        };
        let min_x = layout_line
            .glyphs
            .iter()
            .map(|i| i.x.floor() as i32)
            .min()
            .unwrap_or(0);
        let width = layout_line.w.ceil() as u32;

        // Size the pixmap from the run's actual ascent + descent and place the
        // baseline at `max_ascent`. Using the font size as the height (with the
        // baseline pinned to the bottom) clips descenders.
        let baseline = layout_line.max_ascent.ceil() as i32;
        let height = (layout_line.max_ascent + layout_line.max_descent).ceil() as u32;

        let Some(mut pixmap) = Pixmap::new(width, height) else {
            return;
        };
        let pixels = pixmap.pixels_mut();

        let color = self.color.to_color_u8();
        let color = CosmicColor::rgba(color.red(), color.green(), color.blue(), color.alpha());
        for glyph in &layout_line.glyphs {
            let physical_glyph = glyph.physical((0., 0.), 1.); // TODO scale
            self.cache.with_pixels(
                &mut self.font_system,
                physical_glyph.cache_key,
                color,
                |pixel_x, pixel_y, color| {
                    if color.a() == 0 {
                        return;
                    }

                    let y = baseline + physical_glyph.y + pixel_y;
                    let x = physical_glyph.x + pixel_x - min_x;
                    if x < 0 || x >= width as i32 {
                        return;
                    }
                    let idx = y * width as i32 + x;
                    if idx < 0 {
                        return;
                    }
                    if let Some(pixel) = pixels.get_mut(idx as usize) {
                        let color = ColorU8::from_rgba(color.r(), color.g(), color.b(), color.a());
                        *pixel = alpha_blend(color.premultiply(), *pixel);
                    }
                },
            );
        }

        self.pixmap = Some(pixmap);
    }
}

// `a` over `b`
// This should be correct but not especially efficient.
fn alpha_blend(a: PremultipliedColorU8, b: PremultipliedColorU8) -> PremultipliedColorU8 {
    let inv_alpha = 255 - u16::from(a.alpha());
    let blend_channel = |channel_a: u8, channel_b: u8| -> u8 {
        let blended = u16::from(channel_a) + u16::from(channel_b) * inv_alpha / 255;
        blended.min(255) as u8
    };
    // Channels stay <= alpha through the blend, so this can't actually be
    // `None`; fall back to the destination pixel rather than panicking.
    PremultipliedColorU8::from_rgba(
        blend_channel(a.red(), b.red()),
        blend_channel(a.green(), b.green()),
        blend_channel(a.blue(), b.blue()),
        blend_channel(a.alpha(), b.alpha()),
    )
    .unwrap_or(b)
}

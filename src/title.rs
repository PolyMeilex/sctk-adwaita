use tiny_skia::{Color, Pixmap};

#[cfg(any(feature = "crossfont", feature = "ab_glyph"))]
mod config;
#[cfg(any(feature = "crossfont", feature = "ab_glyph"))]
mod font_preference;

#[cfg(feature = "crossfont")]
mod crossfont_renderer;

#[cfg(feature = "cosmic-text")]
mod cosmic_text_renderer;

#[cfg(all(
    not(feature = "crossfont"),
    not(feature = "cosmic-text"),
    feature = "ab_glyph"
))]
mod ab_glyph_renderer;

#[cfg(all(
    not(feature = "crossfont"),
    not(feature = "cosmic-text"),
    not(feature = "ab_glyph")
))]
mod dumb;

#[derive(Debug)]
pub struct TitleText {
    #[cfg(feature = "crossfont")]
    imp: crossfont_renderer::CrossfontTitleText,
    #[cfg(feature = "cosmic-text")]
    imp: cosmic_text_renderer::CosmicTextTitleText,
    #[cfg(all(
        not(feature = "crossfont"),
        not(feature = "cosmic-text"),
        feature = "ab_glyph"
    ))]
    imp: ab_glyph_renderer::AbGlyphTitleText,
    #[cfg(all(
        not(feature = "crossfont"),
        not(feature = "cosmic-text"),
        not(feature = "ab_glyph")
    ))]
    imp: dumb::DumbTitleText,
}

impl TitleText {
    pub fn new(color: Color) -> Option<Self> {
        #[cfg(feature = "crossfont")]
        return crossfont_renderer::CrossfontTitleText::new(color)
            .ok()
            .map(|imp| Self { imp });

        #[cfg(feature = "cosmic-text")]
        return Some(Self {
            imp: cosmic_text_renderer::CosmicTextTitleText::new(color),
        });

        #[cfg(all(
            not(feature = "crossfont"),
            not(feature = "cosmic-text"),
            feature = "ab_glyph"
        ))]
        return Some(Self {
            imp: ab_glyph_renderer::AbGlyphTitleText::new(color),
        });

        #[cfg(all(
            not(feature = "crossfont"),
            not(feature = "cosmic-text"),
            not(feature = "ab_glyph")
        ))]
        {
            let _ = color;
            return None;
        }
    }

    pub fn update_scale(&mut self, scale: u32) {
        self.imp.update_scale(scale)
    }

    pub fn update_title(&mut self, title: impl Into<String>) {
        self.imp.update_title(title)
    }

    pub fn update_color(&mut self, color: Color) {
        self.imp.update_color(color)
    }

    pub fn pixmap(&self) -> Option<&Pixmap> {
        self.imp.pixmap()
    }
}

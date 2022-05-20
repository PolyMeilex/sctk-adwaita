use tiny_skia::{Color, Pixmap};

#[cfg(feature = "crossfont")]
mod crossfont_renderer;

#[cfg(not(feature = "crossfont"))]
mod dumb;

#[derive(Debug)]
pub struct TitleText {
    #[cfg(feature = "crossfont")]
    imp: crossfont_renderer::CrossfontTitleText,
    #[cfg(not(feature = "crossfont"))]
    imp: dumb::DumbTitleText,
}

impl TitleText {
    pub fn new(color: Color) -> Option<Self> {
        #[cfg(feature = "crossfont")]
        return crossfont_renderer::CrossfontTitleText::new(color)
            .ok()
            .map(|imp| Self { imp });

        #[cfg(not(feature = "crossfont"))]
        {
            let _ = color;
            return None;
        }
    }

    pub fn update_scale(&mut self, scale: u32) {
        self.imp.update_scale(scale)
    }

    pub fn update_title<S: Into<String>>(&mut self, title: S) {
        self.imp.update_title(title)
    }

    pub fn update_color(&mut self, color: Color) {
        self.imp.update_color(color)
    }

    pub fn pixmap(&self) -> Option<&Pixmap> {
        self.imp.pixmap()
    }
}

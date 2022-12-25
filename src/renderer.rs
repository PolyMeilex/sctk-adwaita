use tiny_skia::{Color, PixmapMut};

use crate::{
    buttons::Buttons,
    theme::{ColorMap, HEADER_SIZE},
    title::TitleText,
    Location,
};

#[derive(Debug, Default)]
pub(crate) struct DecorationRenderer {
    pub x: f32,
    pub y: f32,
    pub scale: u32,
    pub window_size: (u32, u32),
    pub maximized: bool,
    pub tiled: bool,
    pub resizable: bool,
}

impl DecorationRenderer {
    pub fn render(
        &self,
        pixmap: &mut PixmapMut,
        buttons: &Buttons,
        colors: &ColorMap,
        title_text: Option<&mut TitleText>,
        mouses: &[Location],
    ) {
        pixmap.fill(Color::TRANSPARENT);

        let header_height = HEADER_SIZE * self.scale as u32;
        let header_width = self.window_size.0 * self.scale as u32;

        let scale = self.scale;

        crate::draw_decoration_background(
            pixmap,
            scale as f32,
            (self.x, self.y),
            #[allow(clippy::identity_op)]
            (
                self.window_size.0 * scale + 1 * scale,
                self.window_size.1 * scale + header_height,
            ),
            colors,
            self.maximized,
            self.tiled,
        );

        if let Some(text_pixmap) = title_text.and_then(|t| t.pixmap()) {
            crate::draw_title(
                pixmap,
                text_pixmap,
                (self.x, self.y),
                (header_width, header_height),
                buttons,
            );
        }

        if buttons.close.x() > self.x {
            buttons.draw_close(scale as f32, colors, mouses, pixmap);
        }

        if buttons.maximize.x() > self.x {
            buttons.draw_maximize(
                scale as f32,
                colors,
                mouses,
                self.resizable,
                self.maximized,
                pixmap,
            );
        }

        if buttons.minimize.x() > self.x {
            buttons.draw_minimize(scale as f32, colors, mouses, pixmap);
        }
    }
}

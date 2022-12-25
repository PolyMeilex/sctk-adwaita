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
    pub scale: f32,
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
        let header_width = self.window_size.0;

        let scale = self.scale;

        crate::draw_decoration_background(
            pixmap,
            scale,
            (self.x, self.y),
            (
                self.window_size.0 as f32 + 1.0,
                self.window_size.1 as f32 + header_height as f32,
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
            buttons.close.draw_close(scale, colors, mouses, pixmap);
        }

        if buttons.maximize.x() > self.x {
            buttons.maximize.draw_maximize(
                scale,
                colors,
                mouses,
                self.resizable,
                self.maximized,
                pixmap,
            );
        }

        if buttons.minimize.x() > self.x {
            buttons
                .minimize
                .draw_minimize(scale, colors, mouses, pixmap);
        }
    }
}

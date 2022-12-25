use std::{cell::RefCell, rc::Rc};

use smithay_client_toolkit::{
    reexports::client::{
        protocol::{
            wl_compositor::WlCompositor, wl_shm, wl_subcompositor::WlSubcompositor,
            wl_subsurface::WlSubsurface, wl_surface::WlSurface,
        },
        Attached, DispatchData,
    },
    shm::AutoMemPool,
    window::FrameRequest,
};
use tiny_skia::{Color, PixmapMut};

use crate::{
    buttons::Buttons,
    surface,
    theme::{ColorMap, BORDER_SIZE, HEADER_SIZE},
    title::TitleText,
    Inner, Location,
};

#[derive(Debug)]
pub(crate) struct DecorationSurface {
    pub surface: WlSurface,
    subsurface: WlSubsurface,
    window_size: (u32, u32),
    pub surface_size: (u32, u32),
}

impl DecorationSurface {
    pub fn new(
        parent: &WlSurface,
        compositor: &Attached<WlCompositor>,
        subcompositor: &Attached<WlSubcompositor>,
        inner: Rc<RefCell<Inner>>,
    ) -> Self {
        let surface = surface::setup_surface(
            compositor.create_surface(),
            Some(move |dpi, surface: WlSurface, ddata: DispatchData| {
                surface.set_buffer_scale(dpi);
                surface.commit();
                (inner.borrow_mut().implem)(FrameRequest::Refresh, 0, ddata);
            }),
        )
        .detach();

        let subsurface = subcompositor.get_subsurface(&surface, parent).detach();
        subsurface.set_position(
            -(BORDER_SIZE as i32),
            -(HEADER_SIZE as i32) - (BORDER_SIZE as i32),
        );
        subsurface.place_below(parent);

        Self {
            surface,
            subsurface,
            surface_size: (0, 0),
            window_size: (0, 0),
        }
    }

    pub fn hide_decoration(&self) {
        self.surface.attach(None, 0, 0);
        self.surface.commit();
    }

    pub fn hide_borders(&self) {
        todo!()
        // for p in self.iter().iter().skip(1) {
        //     p.surface.attach(None, 0, 0);
        //     p.surface.commit();
        // }
    }

    pub fn scale(&self) -> u32 {
        surface::get_surface_scale_factor(&self.surface) as u32
    }

    pub fn update_window_size(&mut self, (w, h): (u32, u32)) {
        self.window_size = (w, h);
        self.surface_size = (w + BORDER_SIZE * 2, h + HEADER_SIZE + BORDER_SIZE * 2);
    }

    pub fn render(
        &self,
        pool: &mut AutoMemPool,
        colors: &ColorMap,
        buttons: &Buttons,
        mouses: &[Location],
        maximizable: bool,
        is_maximized: bool,
        mut title_text: Option<&mut TitleText>,
    ) {
        let scale = self.scale();

        if let Some(title_text) = title_text.as_mut() {
            title_text.update_scale(scale);
            title_text.update_color(colors.font_color);
        }

        if let Ok((canvas, buffer)) = pool.buffer(
            self.surface_size.0 as i32,
            self.surface_size.1 as i32,
            4 * self.surface_size.0 as i32,
            wl_shm::Format::Argb8888,
        ) {
            if let Some(mut pixmap) =
                PixmapMut::from_bytes(canvas, self.surface_size.0, self.surface_size.1)
            {
                pixmap.fill(Color::WHITE);

                let (header_width, header_height) = buttons.scaled_size();

                let margin_h = BORDER_SIZE as f32;
                let margin_v = BORDER_SIZE as f32;
                let scale = 1.0;

                crate::draw_decoration_background(
                    &mut pixmap,
                    scale,
                    (margin_h, margin_v),
                    (
                        self.window_size.0 as f32 + 1.0,
                        self.window_size.1 as f32 + header_height as f32,
                    ),
                    colors,
                    false,
                    false,
                );

                if let Some(text_pixmap) = title_text.and_then(|t| t.pixmap()) {
                    crate::draw_title(
                        &mut pixmap,
                        text_pixmap,
                        (margin_h, margin_v),
                        (header_width, header_height),
                        buttons,
                    );
                }

                if buttons.close.x() > margin_h {
                    buttons.close.draw_close(scale, colors, mouses, &mut pixmap);
                }

                if buttons.maximize.x() > margin_h {
                    buttons.maximize.draw_maximize(
                        scale,
                        colors,
                        mouses,
                        maximizable,
                        is_maximized,
                        &mut pixmap,
                    );
                }

                if buttons.minimize.x() > margin_h {
                    buttons
                        .minimize
                        .draw_minimize(scale, colors, mouses, &mut pixmap);
                }
            }

            self.surface.attach(Some(&buffer), 0, 0);
        }

        // TODO: Damage
        self.surface.damage(0, 0, i32::MAX, i32::MAX);
        self.surface.commit();
    }
}

impl Drop for DecorationSurface {
    fn drop(&mut self) {
        self.subsurface.destroy();
        self.surface.destroy();
    }
}

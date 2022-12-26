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
use tiny_skia::PixmapMut;

use crate::{
    buttons::Buttons,
    renderer, surface,
    theme::{ColorMap, BORDER_SIZE, HEADER_SIZE},
    title::TitleText,
    utils, Inner, Location,
};

#[derive(Debug)]
pub(crate) struct DecorationSurface {
    pub surface: WlSurface,
    subsurface: WlSubsurface,
    window_size: (u32, u32),
    surface_size: (u32, u32),
    maximized_surface_size: (u32, u32),

    maximized: bool,
    tiled: bool,
    resizable: bool,
    buttons: Buttons,
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
            maximized_surface_size: (0, 0),
            window_size: (0, 0),
            maximized: false,
            tiled: false,
            resizable: true,
            buttons: Buttons::default(),
        }
    }

    pub fn surface_size(&self) -> (u32, u32) {
        if self.maximized {
            self.maximized_surface_size
        } else {
            self.surface_size
        }
    }

    pub fn set_maximized(&mut self, maximized: bool) {
        self.maximized = maximized;

        if maximized {
            self.subsurface.set_position(0, -(HEADER_SIZE as i32));
        } else {
            self.subsurface.set_position(
                -(BORDER_SIZE as i32),
                -(HEADER_SIZE as i32) - (BORDER_SIZE as i32),
            );
        }
    }

    pub fn set_title(&mut self, tiled: bool) {
        self.tiled = tiled;
    }

    pub fn set_resizable(&mut self, resizable: bool) {
        self.resizable = resizable;
    }

    pub fn hide_decoration(&self) {
        self.surface.attach(None, 0, 0);
        self.surface.commit();
    }

    pub fn scale(&self) -> u32 {
        surface::get_surface_scale_factor(&self.surface) as u32
    }

    pub fn update_window_size(&mut self, (w, h): (u32, u32)) {
        self.window_size = (w, h);
        self.surface_size = (w + BORDER_SIZE * 2, h + HEADER_SIZE + BORDER_SIZE * 2);
        self.maximized_surface_size = (w, h + HEADER_SIZE);
    }

    pub fn render(
        &mut self,
        pool: &mut AutoMemPool,
        colors: &ColorMap,
        mouses: &[Location],
        mut title_text: Option<&mut TitleText>,
    ) {
        let scale = self.scale();

        if let Some(title_text) = title_text.as_mut() {
            title_text.update_scale(scale);
            title_text.update_color(colors.font_color);
        }

        let surface_size = self.surface_size();
        let surface_size = (surface_size.0 * scale, surface_size.1 * scale);

        if let Ok((canvas, buffer)) = pool.buffer(
            surface_size.0 as i32,
            surface_size.1 as i32,
            4 * surface_size.0 as i32,
            wl_shm::Format::Argb8888,
        ) {
            if let Some(mut pixmap) =
                PixmapMut::from_bytes(canvas, surface_size.0, surface_size.1)
            {
                let (x, y) = if self.maximized {
                    (0, 0)
                } else {
                    (BORDER_SIZE * scale, BORDER_SIZE * scale)
                };

                renderer::render(
                    &mut pixmap,
                    &mut self.buttons,
                    colors,
                    title_text,
                    mouses,
                    renderer::RenderData {
                        x: x as f32,
                        y: y as f32,
                        scale,
                        window_size: self.window_size,
                        surface_size,
                        maximized: self.maximized,
                        tiled: self.tiled,
                        resizable: self.resizable,
                    },
                );
            }

            self.surface.attach(Some(&buffer), 0, 0);
        }

        // TODO: Better damage?
        self.surface.damage(0, 0, i32::MAX, i32::MAX);
        self.surface.commit();
    }

    pub fn precise_location(&self, x: f64, y: f64) -> Location {
        let (width, height) = self.surface_size();

        match self.buttons.find_button(x, y) {
            Some(button) => Location::Button(button),
            None => {
                let top_border = utils::HitBox::new(0.0, 0.0, width as f64, BORDER_SIZE as f64);
                let bottom_border = utils::HitBox::new(
                    0.0,
                    height as f64 - BORDER_SIZE as f64,
                    width as f64,
                    BORDER_SIZE as f64,
                );

                let left_border = utils::HitBox::new(0.0, 0.0, BORDER_SIZE as f64, height as f64);
                let right_border =
                    utils::HitBox::new(width as f64 - 5.0, 0.0, BORDER_SIZE as f64, height as f64);

                let is_top = top_border.contains(x, y);
                let is_bottom = bottom_border.contains(x, y);

                let is_left = left_border.contains(x, y);
                let is_right = right_border.contains(x, y);

                if is_top {
                    if is_left {
                        Location::TopLeft
                    } else if is_right {
                        Location::TopRight
                    } else {
                        Location::Top
                    }
                } else if is_bottom {
                    if is_left {
                        Location::BottomLeft
                    } else if is_right {
                        Location::BottomRight
                    } else {
                        Location::Bottom
                    }
                } else if is_left {
                    Location::Left
                } else if is_right {
                    Location::Right
                } else {
                    Location::Head
                }
            }
        }
    }
}

impl Drop for DecorationSurface {
    fn drop(&mut self) {
        self.subsurface.destroy();
        self.surface.destroy();
    }
}

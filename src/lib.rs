mod buttons;
mod config;
mod decoration_surface;
mod pointer;
mod surface;
pub mod theme;
mod title;

use crate::theme::ColorMap;
use buttons::{ButtonKind, Buttons};
use client::{
    protocol::{wl_compositor, wl_seat, wl_shm, wl_subcompositor, wl_surface},
    Attached, DispatchData,
};
use decoration_surface::DecorationSurface;
use pointer::PointerUserData;
use smithay_client_toolkit::{
    reexports::client,
    seat::pointer::{ThemeManager, ThemeSpec, ThemedPointer},
    shm::AutoMemPool,
    window::{Frame, FrameRequest, State, WindowState},
};
use std::{cell::RefCell, fmt, rc::Rc};
use theme::{ColorTheme, BORDER_SIZE, HEADER_SIZE};
use tiny_skia::{
    ClipMask, FillRule, Path, PathBuilder, Pixmap, PixmapMut, PixmapPaint, Point, Rect, Stroke,
    Transform,
};
use title::TitleText;

type SkiaResult = Option<()>;

/*
 * Utilities
 */

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum Location {
    None,
    Head,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
    TopLeft,
    Button(ButtonKind),
}

/*
 * The core frame
 */

struct Inner {
    decoration: Option<DecorationSurface>,
    size: (u32, u32),
    resizable: bool,
    theme_over_surface: bool,
    implem: Box<dyn FnMut(FrameRequest, u32, DispatchData)>,
    maximized: bool,
    fullscreened: bool,
    tiled: bool,
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inner")
            .field("size", &self.size)
            .field("resizable", &self.resizable)
            .field("theme_over_surface", &self.theme_over_surface)
            .field(
                "implem",
                &"FnMut(FrameRequest, u32, DispatchData) -> { ... }",
            )
            .field("maximized", &self.maximized)
            .field("fullscreened", &self.fullscreened)
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct FrameConfig {
    pub theme: ColorTheme,
}

impl FrameConfig {
    pub fn auto() -> Self {
        Self {
            theme: ColorTheme::auto(),
        }
    }

    pub fn light() -> Self {
        Self {
            theme: ColorTheme::light(),
        }
    }

    pub fn dark() -> Self {
        Self {
            theme: ColorTheme::dark(),
        }
    }
}

/// A simple set of decorations
#[derive(Debug)]
pub struct AdwaitaFrame {
    base_surface: wl_surface::WlSurface,
    compositor: Attached<wl_compositor::WlCompositor>,
    subcompositor: Attached<wl_subcompositor::WlSubcompositor>,
    inner: Rc<RefCell<Inner>>,
    pool: AutoMemPool,
    active: WindowState,
    hidden: bool,
    pointers: Vec<ThemedPointer>,
    themer: ThemeManager,
    surface_version: u32,

    buttons: Rc<RefCell<Buttons>>,
    colors: ColorTheme,
    title: Option<String>,
    title_text: Option<TitleText>,
}

impl Frame for AdwaitaFrame {
    type Error = ::std::io::Error;
    type Config = FrameConfig;
    fn init(
        base_surface: &wl_surface::WlSurface,
        compositor: &Attached<wl_compositor::WlCompositor>,
        subcompositor: &Attached<wl_subcompositor::WlSubcompositor>,
        shm: &Attached<wl_shm::WlShm>,
        theme_manager: Option<ThemeManager>,
        implementation: Box<dyn FnMut(FrameRequest, u32, DispatchData)>,
    ) -> Result<AdwaitaFrame, ::std::io::Error> {
        let (themer, theme_over_surface) = if let Some(theme_manager) = theme_manager {
            (theme_manager, false)
        } else {
            (
                ThemeManager::init(ThemeSpec::System, compositor.clone(), shm.clone()),
                true,
            )
        };

        let inner = Rc::new(RefCell::new(Inner {
            decoration: None,
            size: (1, 1),
            resizable: true,
            implem: implementation,
            theme_over_surface,
            maximized: false,
            fullscreened: false,
            tiled: false,
        }));

        let pool = AutoMemPool::new(shm.clone())?;

        let colors = ColorTheme::auto();

        Ok(AdwaitaFrame {
            base_surface: base_surface.clone(),
            compositor: compositor.clone(),
            subcompositor: subcompositor.clone(),
            inner,
            pool,
            active: WindowState::Inactive,
            hidden: true,
            pointers: Vec::new(),
            themer,
            surface_version: compositor.as_ref().version(),
            buttons: Default::default(),
            title: None,
            title_text: TitleText::new(colors.active.font_color),
            colors,
        })
    }

    fn new_seat(&mut self, seat: &Attached<wl_seat::WlSeat>) {
        let inner = self.inner.clone();

        let buttons = self.buttons.clone();
        let pointer = self.themer.theme_pointer_with_impl(
            seat,
            move |event, pointer: ThemedPointer, ddata: DispatchData| {
                if let Some(data) = pointer
                    .as_ref()
                    .user_data()
                    .get::<RefCell<PointerUserData>>()
                {
                    let mut data = data.borrow_mut();
                    let mut inner = inner.borrow_mut();
                    data.event(event, &mut inner, &buttons.borrow(), &pointer, ddata);
                }
            },
        );
        pointer
            .as_ref()
            .user_data()
            .set(|| RefCell::new(PointerUserData::new(seat.detach())));
        self.pointers.push(pointer);
    }

    fn remove_seat(&mut self, seat: &wl_seat::WlSeat) {
        self.pointers.retain(|pointer| {
            pointer
                .as_ref()
                .user_data()
                .get::<RefCell<PointerUserData>>()
                .map(|user_data| {
                    let guard = user_data.borrow_mut();
                    if &guard.seat == seat {
                        pointer.release();
                        false
                    } else {
                        true
                    }
                })
                .unwrap_or(false)
        });
    }

    fn set_states(&mut self, states: &[State]) -> bool {
        let mut inner = self.inner.borrow_mut();
        let mut need_redraw = false;

        // Process active.
        let new_active = if states.contains(&State::Activated) {
            WindowState::Active
        } else {
            WindowState::Inactive
        };
        need_redraw |= new_active != self.active;
        self.active = new_active;

        // Process maximized.
        let new_maximized = states.contains(&State::Maximized);
        need_redraw |= new_maximized != inner.maximized;
        inner.maximized = new_maximized;

        // Process fullscreened.
        let new_fullscreened = states.contains(&State::Fullscreen);
        need_redraw |= new_fullscreened != inner.fullscreened;
        inner.fullscreened = new_fullscreened;

        let new_tiled = states.contains(&State::TiledLeft)
            || states.contains(&State::TiledRight)
            || states.contains(&State::TiledTop)
            || states.contains(&State::TiledBottom);
        need_redraw |= new_tiled != inner.tiled;
        inner.tiled = new_tiled;

        need_redraw
    }

    fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
        let mut inner = self.inner.borrow_mut();
        if !self.hidden {
            inner.decoration = Some(DecorationSurface::new(
                &self.base_surface,
                &self.compositor,
                &self.subcompositor,
                self.inner.clone(),
            ));
        } else {
            inner.decoration = None;
        }
    }

    fn set_resizable(&mut self, resizable: bool) {
        self.inner.borrow_mut().resizable = resizable;
    }

    fn resize(&mut self, newsize: (u32, u32)) {
        self.inner.borrow_mut().size = newsize;

        if let Some(decoration) = self.inner.borrow_mut().decoration.as_mut() {
            decoration.update_window_size(newsize);
        }

        self.buttons
            .borrow_mut()
            .arrange(newsize.0 + BORDER_SIZE * 2);
    }

    fn redraw(&mut self) {
        let inner = &mut *self.inner.borrow_mut();

        // Don't draw borders if the frame explicitly hidden or fullscreened.
        if self.hidden || inner.fullscreened {
            if let Some(decor) = inner.decoration.as_mut() {
                decor.hide_decoration();
            }
            return;
        }

        let pointers = self
            .pointers
            .iter()
            .flat_map(|p| {
                if p.as_ref().is_alive() {
                    let data: &RefCell<PointerUserData> = p.as_ref().user_data().get()?;
                    Some(data.borrow().location)
                } else {
                    None
                }
            })
            .collect::<Vec<Location>>();

        if let Some(decoration) = inner.decoration.as_mut() {
            decoration.render(
                &mut self.pool,
                self.colors.for_state(self.active),
                &self.buttons.borrow(),
                &pointers,
                inner.resizable,
                inner.maximized,
                self.title_text.as_mut(),
            );
        }
    }

    fn subtract_borders(&self, width: i32, height: i32) -> (i32, i32) {
        if self.hidden || self.inner.borrow().fullscreened {
            (width, height)
        } else {
            (width, height - HEADER_SIZE as i32)
        }
    }

    fn add_borders(&self, width: i32, height: i32) -> (i32, i32) {
        if self.hidden || self.inner.borrow().fullscreened {
            (width, height)
        } else {
            (width, height + HEADER_SIZE as i32)
        }
    }

    fn location(&self) -> (i32, i32) {
        if self.hidden || self.inner.borrow().fullscreened {
            (0, 0)
        } else {
            (0, -(HEADER_SIZE as i32))
        }
    }

    fn set_config(&mut self, config: FrameConfig) {
        self.colors = config.theme;
    }

    fn set_title(&mut self, title: String) {
        if let Some(title_text) = self.title_text.as_mut() {
            title_text.update_title(&title);
        }

        self.title = Some(title);
    }
}

impl Drop for AdwaitaFrame {
    fn drop(&mut self) {
        for ptr in self.pointers.drain(..) {
            if ptr.as_ref().version() >= 3 {
                ptr.release();
            }
        }
    }
}

fn draw_title(
    pixmap: &mut PixmapMut,
    text_pixmap: &Pixmap,
    (margin_h, margin_v): (f32, f32),
    (header_w, header_h): (u32, u32),
    buttons: &Buttons,
) {
    let canvas_w = pixmap.width() as f32;
    let canvas_h = pixmap.height() as f32;

    let header_w = header_w as f32;
    let header_h = header_h as f32;

    let text_w = text_pixmap.width() as f32;
    let text_h = text_pixmap.height() as f32;

    let x = header_w / 2.0 - text_w / 2.0;
    let y = header_h / 2.0 - text_h / 2.0;

    let x = margin_h + x;
    let y = margin_v + y;

    let (x, y) = if x + text_w < buttons.minimize.x() - 10.0 {
        (x, y)
    } else {
        let y = header_h / 2.0 - text_h / 2.0;

        let x = buttons.minimize.x() - text_w - 10.0;
        let y = margin_v + y;
        (x, y)
    };

    let x = x.max(margin_h + 5.0);

    if let Some(clip) = Rect::from_xywh(0.0, 0.0, buttons.minimize.x() - 10.0, canvas_h) {
        let mut mask = ClipMask::new();
        mask.set_path(
            canvas_w as u32,
            canvas_h as u32,
            &PathBuilder::from_rect(clip),
            FillRule::Winding,
            false,
        );

        pixmap.draw_pixmap(
            x as i32,
            y as i32,
            text_pixmap.as_ref(),
            &PixmapPaint::default(),
            Transform::identity(),
            Some(&mask),
        );
    }
}

fn draw_decoration_background(
    pixmap: &mut PixmapMut,
    scale: f32,
    (margin_h, margin_v): (f32, f32),
    (width, height): (f32, f32),
    colors: &ColorMap,
    is_maximized: bool,
    tiled: bool,
) -> SkiaResult {
    let radius = if is_maximized || tiled {
        0.0
    } else {
        10.0 * scale
    };

    let margin_h = margin_h - 1.0;

    let bg = rounded_headerbar_shape(margin_h, margin_v, width, height, radius)?;
    let header = rounded_headerbar_shape(margin_h, margin_v, width, HEADER_SIZE as f32, radius)?;

    pixmap.fill_path(
        &header,
        &colors.headerbar_paint(),
        FillRule::Winding,
        Transform::identity(),
        None,
    );

    pixmap.stroke_path(
        &bg,
        &colors.border_paint(),
        &Stroke::default(),
        Transform::identity(),
        None,
    );

    Some(())
}

fn rounded_headerbar_shape(x: f32, y: f32, width: f32, height: f32, radius: f32) -> Option<Path> {
    use std::f32::consts::FRAC_1_SQRT_2;

    let mut pb = PathBuilder::new();
    let mut cursor = Point::from_xy(x, y);

    // !!!
    // This code is heavily "inspired" by https://gitlab.com/snakedye/snui/
    // So technically it should be licensed under MPL-2.0, sorry about that 🥺 👉👈
    // !!!

    // Positioning the cursor
    cursor.y += radius;
    pb.move_to(cursor.x, cursor.y);

    // Drawing the outline
    pb.cubic_to(
        cursor.x,
        cursor.y,
        cursor.x,
        cursor.y - FRAC_1_SQRT_2 * radius,
        {
            cursor.x += radius;
            cursor.x
        },
        {
            cursor.y -= radius;
            cursor.y
        },
    );
    pb.line_to(
        {
            cursor.x = x + width - radius;
            cursor.x
        },
        cursor.y,
    );
    pb.cubic_to(
        cursor.x,
        cursor.y,
        cursor.x + FRAC_1_SQRT_2 * radius,
        cursor.y,
        {
            cursor.x += radius;
            cursor.x
        },
        {
            cursor.y += radius;
            cursor.y
        },
    );
    pb.line_to(cursor.x, {
        cursor.y = y + height;
        cursor.y
    });
    pb.line_to(
        {
            cursor.x = x;
            cursor.x
        },
        cursor.y,
    );

    pb.close();

    pb.finish()
}

fn precise_location(buttons: &Buttons, width: u32, height: u32, x: f64, y: f64) -> Location {
    match buttons.find_button(x, y) {
        Some(button) => Location::Button(button),
        None => {
            let is_top = y <= BORDER_SIZE as f64;
            let is_bottom = y >= height as f64;

            let is_left = x <= BORDER_SIZE as f64;
            let is_right = x >= f64::from(BORDER_SIZE + width);

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
            } else if x < f64::from(BORDER_SIZE) {
                Location::Left
            } else if x > f64::from(width) {
                Location::Right
            } else {
                Location::Head
            }
        }
    }
}

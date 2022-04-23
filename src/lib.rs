use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use parts::{DecorationPartKind, Parts};
use smithay_client_toolkit::reexports::client::protocol::wl_seat::WlSeat;
use smithay_client_toolkit::reexports::{client, protocols};

use client::protocol::{
    wl_compositor, wl_pointer, wl_seat, wl_shm, wl_subcompositor, wl_subsurface, wl_surface,
};
use client::{Attached, DispatchData};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

use log::error;

use smithay_client_toolkit::seat::pointer::{ThemeManager, ThemeSpec, ThemedPointer};
use smithay_client_toolkit::shm::AutoMemPool;
use smithay_client_toolkit::window::{ButtonState, Frame, FrameRequest, State, WindowState};

mod theme;
use theme::{ColorTheme, BORDER_COLOR, BORDER_SIZE, HEADER_SIZE};

mod buttons;
use buttons::{ButtonKind, Buttons};

mod parts;
mod surface;

/*
 * Utilities
 */

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Location {
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

#[derive(Debug)]
pub struct Part {
    pub surface: wl_surface::WlSurface,
    pub subsurface: wl_subsurface::WlSubsurface,
}

impl Part {
    fn new(
        parent: &wl_surface::WlSurface,
        compositor: &Attached<wl_compositor::WlCompositor>,
        subcompositor: &Attached<wl_subcompositor::WlSubcompositor>,
        inner: Option<Rc<RefCell<Inner>>>,
    ) -> Part {
        let surface = if let Some(inner) = inner {
            surface::setup_surface(
                compositor.create_surface(),
                Some(
                    move |dpi, surface: wl_surface::WlSurface, ddata: DispatchData| {
                        surface.set_buffer_scale(dpi);
                        surface.commit();
                        (&mut inner.borrow_mut().implem)(FrameRequest::Refresh, 0, ddata);
                    },
                ),
            )
        } else {
            surface::setup_surface(
                compositor.create_surface(),
                Some(
                    move |dpi, surface: wl_surface::WlSurface, _ddata: DispatchData| {
                        surface.set_buffer_scale(dpi);
                        surface.commit();
                    },
                ),
            )
        };

        let surface = surface.detach();

        let subsurface = subcompositor.get_subsurface(&surface, parent);

        Part {
            surface,
            subsurface: subsurface.detach(),
        }
    }

    fn scale(&self) -> u32 {
        surface::get_surface_scale_factor(&self.surface) as u32
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        self.subsurface.destroy();
        self.surface.destroy();
    }
}

struct PointerUserData {
    location: Location,
    current_surface: DecorationPartKind,

    position: (f64, f64),
    seat: WlSeat,
}

impl PointerUserData {
    fn new(seat: WlSeat) -> Self {
        Self {
            location: Location::None,
            current_surface: DecorationPartKind::None,
            position: (0.0, 0.0),
            seat,
        }
    }

    fn event(
        &mut self,
        event: wl_pointer::Event,
        inner: &mut Inner,
        buttons: &Buttons,
        pointer: &ThemedPointer,
        ddata: DispatchData<'_>,
    ) {
        use wl_pointer::Event;
        match event {
            Event::Enter {
                serial,
                surface,
                surface_x,
                surface_y,
            } => {
                self.location = precise_location(
                    buttons,
                    inner.parts.find_surface(&surface),
                    inner.size.0,
                    surface_x,
                    surface_y,
                );
                self.current_surface = inner.parts.find_decoration_part(&surface);
                self.position = (surface_x, surface_y);
                change_pointer(&pointer, &inner, self.location, Some(serial))
            }
            Event::Leave { serial, .. } => {
                self.current_surface = DecorationPartKind::None;

                self.location = Location::None;
                change_pointer(&pointer, &inner, self.location, Some(serial));
                (&mut inner.implem)(FrameRequest::Refresh, 0, ddata);
            }
            Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                self.position = (surface_x, surface_y);
                let newpos =
                    precise_location(buttons, self.location, inner.size.0, surface_x, surface_y);
                if newpos != self.location {
                    match (newpos, self.location) {
                        (Location::Button(_), _) | (_, Location::Button(_)) => {
                            // pointer movement involves a button, request refresh
                            (&mut inner.implem)(FrameRequest::Refresh, 0, ddata);
                        }
                        _ => (),
                    }
                    // we changed of part of the decoration, pointer image
                    // may need to be changed
                    self.location = newpos;
                    change_pointer(&pointer, &inner, self.location, None)
                }
            }
            Event::Button {
                serial,
                button,
                state,
                ..
            } => {
                if state == wl_pointer::ButtonState::Pressed {
                    let request = match button {
                        // Left mouse button.
                        0x110 => {
                            request_for_location_on_lmb(&self, inner.maximized, inner.resizable)
                        }
                        // Right mouse button.
                        0x111 => request_for_location_on_rmb(&self),
                        _ => None,
                    };

                    if let Some(request) = request {
                        (&mut inner.implem)(request, serial, ddata);
                    }
                }
            }
            _ => {}
        }
    }
}

/*
 * The core frame
 */

pub struct Inner {
    parts: Parts,
    size: (u32, u32),
    resizable: bool,
    theme_over_surface: bool,
    implem: Box<dyn FnMut(FrameRequest, u32, DispatchData)>,
    maximized: bool,
    fullscreened: bool,
}

impl fmt::Debug for Inner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Inner")
            .field("parts", &self.parts)
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

fn precise_location(buttons: &Buttons, old: Location, width: u32, x: f64, y: f64) -> Location {
    match old {
        Location::Head | Location::Button(_) => buttons.find_button(x, y),

        Location::Top | Location::TopLeft | Location::TopRight => {
            if x <= f64::from(BORDER_SIZE) {
                Location::TopLeft
            } else if x >= f64::from(width + BORDER_SIZE) {
                Location::TopRight
            } else {
                Location::Top
            }
        }

        Location::Bottom | Location::BottomLeft | Location::BottomRight => {
            if x <= f64::from(BORDER_SIZE) {
                Location::BottomLeft
            } else if x >= f64::from(width + BORDER_SIZE) {
                Location::BottomRight
            } else {
                Location::Bottom
            }
        }

        other => other,
    }
}

/// A simple set of decorations that can be used as a fallback
///
/// This class drawn some simple and minimalistic decorations around
/// a window so that it remains possible to interact with the window
/// even when server-side decorations are not available.
///
/// `FallbackFrame` is hiding its `ClientSide` decorations
/// in a `Fullscreen` state and brings them back if those are
/// visible when unsetting `Fullscreen` state.
#[derive(Debug)]
pub struct FallbackFrame {
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
}

impl Frame for FallbackFrame {
    type Error = ::std::io::Error;
    type Config = ();
    fn init(
        base_surface: &wl_surface::WlSurface,
        compositor: &Attached<wl_compositor::WlCompositor>,
        subcompositor: &Attached<wl_subcompositor::WlSubcompositor>,
        shm: &Attached<wl_shm::WlShm>,
        theme_manager: Option<ThemeManager>,
        implementation: Box<dyn FnMut(FrameRequest, u32, DispatchData)>,
    ) -> Result<FallbackFrame, ::std::io::Error> {
        let (themer, theme_over_surface) = if let Some(theme_manager) = theme_manager {
            (theme_manager, false)
        } else {
            (
                ThemeManager::init(ThemeSpec::System, compositor.clone(), shm.clone()),
                true,
            )
        };

        let inner = Rc::new(RefCell::new(Inner {
            parts: Parts::default(),
            size: (1, 1),
            resizable: true,
            implem: implementation,
            theme_over_surface,
            maximized: false,
            fullscreened: false,
        }));

        let pool = AutoMemPool::new(shm.clone())?;

        Ok(FallbackFrame {
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
            colors: Default::default(),
        })
    }

    fn new_seat(&mut self, seat: &Attached<wl_seat::WlSeat>) {
        let inner = self.inner.clone();

        let buttons = self.buttons.clone();
        let pointer = self.themer.theme_pointer_with_impl(
            seat,
            move |event, pointer: ThemedPointer, ddata: DispatchData| {
                let data: &RefCell<PointerUserData> = pointer.as_ref().user_data().get().unwrap();
                let mut data = data.borrow_mut();
                let mut inner = inner.borrow_mut();
                data.event(event, &mut inner, &buttons.borrow(), &pointer, ddata);
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
            let user_data = pointer
                .as_ref()
                .user_data()
                .get::<RefCell<PointerUserData>>()
                .unwrap();
            let guard = user_data.borrow_mut();
            if &guard.seat == seat {
                pointer.release();
                false
            } else {
                true
            }
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

        need_redraw
    }

    fn set_hidden(&mut self, hidden: bool) {
        self.hidden = hidden;
        let mut inner = self.inner.borrow_mut();
        if !self.hidden {
            inner.parts.add_decorations(
                &self.base_surface,
                &self.compositor,
                &self.subcompositor,
                self.inner.clone(),
            );
        } else {
            inner.parts.remove_decorations();
        }
    }

    fn set_resizable(&mut self, resizable: bool) {
        self.inner.borrow_mut().resizable = resizable;
    }

    fn resize(&mut self, newsize: (u32, u32)) {
        self.inner.borrow_mut().size = newsize;
        self.buttons.borrow_mut().arrange(newsize.0);
    }

    fn redraw(&mut self) {
        let inner = self.inner.borrow_mut();

        // Don't draw borders if the frame explicitly hidden or fullscreened.
        if self.hidden || inner.fullscreened {
            inner.parts.hide_decorations();
            return;
        }

        // `parts` can't be empty here, since the initial state for `self.hidden` is true, and
        // they will be created once `self.hidden` will become `false`.
        let parts = &inner.parts;

        let (width, height) = inner.size;

        if let Some(decoration) = parts.decoration() {
            // Use header scale for all the thing.
            let header_scale = decoration.header.scale();
            self.buttons.borrow_mut().update_scale(header_scale);

            let top_scale = decoration.header.scale();
            let left_scale = decoration.left.scale();
            let right_scale = decoration.right.scale();
            let bottom_scale = decoration.bottom.scale();

            let (header_width, header_height) = self.buttons.borrow().scaled_size();

            {
                // Create the buffers and draw

                // -> head-subsurface
                if let Ok((canvas, buffer)) = self.pool.buffer(
                    header_width as i32,
                    header_height as i32,
                    4 * header_width as i32,
                    wl_shm::Format::Argb8888,
                ) {
                    draw_buttons(
                        canvas,
                        width,
                        header_scale,
                        inner.resizable,
                        self.active,
                        &self.colors,
                        &mut self.buttons.borrow_mut(),
                        &self
                            .pointers
                            .iter()
                            .flat_map(|p| {
                                if p.as_ref().is_alive() {
                                    let data: &RefCell<PointerUserData> =
                                        p.as_ref().user_data().get().unwrap();
                                    Some(data.borrow().location)
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<Location>>(),
                    );

                    decoration
                        .header
                        .subsurface
                        .set_position(0, -(HEADER_SIZE as i32));
                    decoration.header.surface.attach(Some(&buffer), 0, 0);
                    if self.surface_version >= 4 {
                        decoration.header.surface.damage_buffer(
                            0,
                            0,
                            header_width as i32,
                            header_height as i32,
                        );
                    } else {
                        // surface is old and does not support damage_buffer, so we damage
                        // in surface coordinates and hope it is not rescaled
                        decoration
                            .header
                            .surface
                            .damage(0, 0, width as i32, HEADER_SIZE as i32);
                    }
                    decoration.header.surface.commit();
                }

                if inner.maximized {
                    // Don't draw the borders.
                    decoration.hide_borders();
                    return;
                }

                // -> top-subsurface
                if let Ok((canvas, buffer)) = self.pool.buffer(
                    ((width + 2 * BORDER_SIZE) * top_scale) as i32,
                    (BORDER_SIZE * top_scale) as i32,
                    (4 * top_scale * (width + 2 * BORDER_SIZE)) as i32,
                    wl_shm::Format::Argb8888,
                ) {
                    for pixel in canvas.chunks_exact_mut(4) {
                        pixel[0] = 0;
                        pixel[1] = 0;
                        pixel[2] = 0;
                        pixel[3] = 0;
                    }

                    decoration.top.subsurface.set_position(
                        -(BORDER_SIZE as i32),
                        -(HEADER_SIZE as i32 + BORDER_SIZE as i32),
                    );
                    decoration.top.surface.attach(Some(&buffer), 0, 0);
                    if self.surface_version >= 4 {
                        decoration.top.surface.damage_buffer(
                            0,
                            0,
                            ((width + 2 * BORDER_SIZE) * top_scale) as i32,
                            (BORDER_SIZE * top_scale) as i32,
                        );
                    } else {
                        // surface is old and does not support damage_buffer, so we damage
                        // in surface coordinates and hope it is not rescaled
                        decoration.top.surface.damage(
                            0,
                            0,
                            (width + 2 * BORDER_SIZE) as i32,
                            BORDER_SIZE as i32,
                        );
                    }
                    decoration.top.surface.commit();
                }

                let w = ((width + 2 * BORDER_SIZE) * bottom_scale) as i32;
                // -> bottom-subsurface
                if let Ok((canvas, buffer)) = self.pool.buffer(
                    w,
                    (BORDER_SIZE * bottom_scale) as i32,
                    (4 * bottom_scale * (width + 2 * BORDER_SIZE)) as i32,
                    wl_shm::Format::Argb8888,
                ) {
                    for (id, pixel) in canvas.chunks_exact_mut(4).enumerate() {
                        let vid = id as i32 % w;
                        let hid = id as i32 / w;
                        let color = if vid > BORDER_SIZE as i32 - 2
                            && vid < w - (BORDER_SIZE as i32 - 1)
                            && hid < 1
                        {
                            BORDER_COLOR
                        } else {
                            [0, 0, 0, 0]
                        };

                        pixel[0] = color[0];
                        pixel[1] = color[1];
                        pixel[2] = color[2];
                        pixel[3] = color[3];
                    }

                    decoration
                        .bottom
                        .subsurface
                        .set_position(-(BORDER_SIZE as i32), height as i32);
                    decoration.bottom.surface.attach(Some(&buffer), 0, 0);
                    if self.surface_version >= 4 {
                        decoration.bottom.surface.damage_buffer(
                            0,
                            0,
                            ((width + 2 * BORDER_SIZE) * bottom_scale) as i32,
                            (BORDER_SIZE * bottom_scale) as i32,
                        );
                    } else {
                        // surface is old and does not support damage_buffer, so we damage
                        // in surface coordinates and hope it is not rescaled
                        decoration.bottom.surface.damage(
                            0,
                            0,
                            (width + 2 * BORDER_SIZE) as i32,
                            BORDER_SIZE as i32,
                        );
                    }
                    decoration.bottom.surface.commit();
                }

                let w = (BORDER_SIZE * left_scale) as i32;
                let h = ((height + HEADER_SIZE) * left_scale) as i32;
                // -> left-subsurface
                if let Ok((canvas, buffer)) = self.pool.buffer(
                    w,
                    h,
                    4 * (BORDER_SIZE * left_scale) as i32,
                    wl_shm::Format::Argb8888,
                ) {
                    for (id, pixel) in canvas.chunks_exact_mut(4).enumerate() {
                        let vid = id as i32 % w;
                        let hid = id as i32 / w;
                        let color = if vid > w - 2 && hid > BORDER_SIZE as i32 {
                            BORDER_COLOR
                        } else {
                            [0, 0, 0, 0]
                        };
                        pixel[0] = color[0];
                        pixel[1] = color[1];
                        pixel[2] = color[2];
                        pixel[3] = color[3];
                    }

                    decoration
                        .left
                        .subsurface
                        .set_position(-(BORDER_SIZE as i32), -(HEADER_SIZE as i32));
                    decoration.left.surface.attach(Some(&buffer), 0, 0);
                    if self.surface_version >= 4 {
                        decoration.left.surface.damage_buffer(
                            0,
                            0,
                            (BORDER_SIZE * left_scale) as i32,
                            ((height + HEADER_SIZE) * left_scale) as i32,
                        );
                    } else {
                        // surface is old and does not support damage_buffer, so we damage
                        // in surface coordinates and hope it is not rescaled
                        decoration.left.surface.damage(
                            0,
                            0,
                            BORDER_SIZE as i32,
                            (height + HEADER_SIZE) as i32,
                        );
                    }
                    decoration.left.surface.commit();
                }

                let w = (BORDER_SIZE * right_scale) as i32;
                // -> right-subsurface
                if let Ok((canvas, buffer)) = self.pool.buffer(
                    w,
                    ((height + HEADER_SIZE) * right_scale) as i32,
                    4 * (BORDER_SIZE * right_scale) as i32,
                    wl_shm::Format::Argb8888,
                ) {
                    for (id, pixel) in canvas.chunks_exact_mut(4).enumerate() {
                        let wid = id as i32 % w;
                        let hid = id as i32 / w;
                        let color = if wid < 1 && hid > BORDER_SIZE as i32 {
                            BORDER_COLOR
                        } else {
                            [0, 0, 0, 0]
                        };
                        pixel[0] = color[0];
                        pixel[1] = color[1];
                        pixel[2] = color[2];
                        pixel[3] = color[3];
                    }

                    decoration
                        .right
                        .subsurface
                        .set_position(width as i32, -(HEADER_SIZE as i32));
                    decoration.right.surface.attach(Some(&buffer), 0, 0);
                    if self.surface_version >= 4 {
                        decoration.right.surface.damage_buffer(
                            0,
                            0,
                            (BORDER_SIZE * right_scale) as i32,
                            ((height + HEADER_SIZE) * right_scale) as i32,
                        );
                    } else {
                        // surface is old and does not support damage_buffer, so we damage
                        // in surface coordinates and hope it is not rescaled
                        decoration.right.surface.damage(
                            0,
                            0,
                            BORDER_SIZE as i32,
                            (height + HEADER_SIZE) as i32,
                        );
                    }
                    decoration.right.surface.commit();
                }
            }
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

    fn set_config(&mut self, _config: ()) {}

    fn set_title(&mut self, _title: String) {}
}

impl Drop for FallbackFrame {
    fn drop(&mut self) {
        for ptr in self.pointers.drain(..) {
            if ptr.as_ref().version() >= 3 {
                ptr.release();
            }
        }
    }
}

fn change_pointer(pointer: &ThemedPointer, inner: &Inner, location: Location, serial: Option<u32>) {
    // Prevent theming of the surface if it was requested.
    if !inner.theme_over_surface && location == Location::None {
        return;
    }

    let name = match location {
        // If we can't resize a frame we shouldn't show resize cursors.
        _ if !inner.resizable => "left_ptr",
        Location::Top => "top_side",
        Location::TopRight => "top_right_corner",
        Location::Right => "right_side",
        Location::BottomRight => "bottom_right_corner",
        Location::Bottom => "bottom_side",
        Location::BottomLeft => "bottom_left_corner",
        Location::Left => "left_side",
        Location::TopLeft => "top_left_corner",
        _ => "left_ptr",
    };

    if pointer.set_cursor(name, serial).is_err() {
        error!("Failed to set cursor");
    }
}

fn request_for_location_on_lmb(
    pointer_data: &PointerUserData,
    maximized: bool,
    resizable: bool,
) -> Option<FrameRequest> {
    use protocols::xdg_shell::client::xdg_toplevel::ResizeEdge;
    match pointer_data.location {
        Location::Top if resizable => Some(FrameRequest::Resize(
            pointer_data.seat.clone(),
            ResizeEdge::Top,
        )),
        Location::TopLeft if resizable => Some(FrameRequest::Resize(
            pointer_data.seat.clone(),
            ResizeEdge::TopLeft,
        )),
        Location::Left if resizable => Some(FrameRequest::Resize(
            pointer_data.seat.clone(),
            ResizeEdge::Left,
        )),
        Location::BottomLeft if resizable => Some(FrameRequest::Resize(
            pointer_data.seat.clone(),
            ResizeEdge::BottomLeft,
        )),
        Location::Bottom if resizable => Some(FrameRequest::Resize(
            pointer_data.seat.clone(),
            ResizeEdge::Bottom,
        )),
        Location::BottomRight if resizable => Some(FrameRequest::Resize(
            pointer_data.seat.clone(),
            ResizeEdge::BottomRight,
        )),
        Location::Right if resizable => Some(FrameRequest::Resize(
            pointer_data.seat.clone(),
            ResizeEdge::Right,
        )),
        Location::TopRight if resizable => Some(FrameRequest::Resize(
            pointer_data.seat.clone(),
            ResizeEdge::TopRight,
        )),
        Location::Head => Some(FrameRequest::Move(pointer_data.seat.clone())),
        Location::Button(ButtonKind::Close) => Some(FrameRequest::Close),
        Location::Button(ButtonKind::Maximize) => {
            if maximized {
                Some(FrameRequest::UnMaximize)
            } else {
                Some(FrameRequest::Maximize)
            }
        }
        Location::Button(ButtonKind::Minimize) => Some(FrameRequest::Minimize),
        _ => None,
    }
}

fn request_for_location_on_rmb(pointer_data: &PointerUserData) -> Option<FrameRequest> {
    match pointer_data.location {
        Location::Head | Location::Button(_) => Some(FrameRequest::ShowMenu(
            pointer_data.seat.clone(),
            pointer_data.position.0 as i32,
            // We must offset it by header size for precise position.
            pointer_data.position.1 as i32 - HEADER_SIZE as i32,
        )),
        _ => None,
    }
}

fn draw_buttons(
    canvas: &mut [u8],
    width: u32,
    scale: u32,
    maximizable: bool,
    state: WindowState,
    colors: &ColorTheme,
    buttons: &mut Buttons,
    mouses: &[Location],
) {
    let w = width;
    let h = HEADER_SIZE;
    let scale = scale as usize;

    let colors = if state == WindowState::Active {
        &colors.active
    } else {
        &colors.inactive
    };

    let headerbar_paint = colors.headerbar_paint();

    let mut button_icon_paint = colors.button_icon_paint();
    let button_idle_paint = colors.button_idle_paint();
    let button_hover_paint = colors.button_hover_paint();

    let mut pixmap = Pixmap::new(w as u32 * scale as u32, h as u32 * scale as u32).unwrap();

    {
        let h = h as f32 * scale as f32;
        let w = w as f32 * scale as f32;

        let r = 10.0 * scale as f32;
        let x = r;
        let y = r;

        let corner_l = {
            let mut pb = PathBuilder::new();
            pb.push_circle(x, y, r);
            pb.finish().unwrap()
        };

        pixmap.fill_path(
            &corner_l,
            &headerbar_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        pixmap.fill_rect(
            Rect::from_xywh(0.0, y, r, h - r).unwrap(),
            &headerbar_paint,
            Transform::identity(),
            None,
        );

        if let Some(rect) = Rect::from_xywh(x, 0.0, w - r * 2.0, h) {
            pixmap.fill_rect(rect, &headerbar_paint, Transform::identity(), None);
        }

        let x = w - r;

        let corner_r = {
            let mut pb = PathBuilder::new();
            pb.push_circle(x, y, r);
            pb.finish().unwrap()
        };

        pixmap.fill_path(
            &corner_r,
            &headerbar_paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        pixmap.fill_rect(
            Rect::from_xywh(x, y, r, h - r).unwrap(),
            &headerbar_paint,
            Transform::identity(),
            None,
        );

        // Line

        let mut line = Paint::default();
        line.set_color_rgba8(220, 220, 220, 255);
        line.anti_alias = false;

        pixmap.fill_rect(
            Rect::from_xywh(0.0, h - 1.0, w, h).unwrap(),
            &line,
            Transform::identity(),
            None,
        );
    }

    {
        // Draw the close button
        let btn_state = if mouses
            .iter()
            .any(|&l| l == Location::Button(ButtonKind::Close))
        {
            ButtonState::Hovered
        } else {
            ButtonState::Idle
        };

        let radius = buttons.close.radius();

        let x = buttons.close.center_x();
        let y = buttons.close.center_y();

        let path1 = {
            let mut pb = PathBuilder::new();
            pb.push_circle(x, y, radius);
            pb.finish().unwrap()
        };

        if state == WindowState::Active && btn_state == ButtonState::Hovered {
            pixmap.fill_path(
                &path1,
                &button_hover_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        } else {
            pixmap.fill_path(
                &path1,
                &button_idle_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        let x_icon = {
            let size = 3.5;
            let mut pb = PathBuilder::new();

            {
                let sx = x - size;
                let sy = y - size;
                let ex = x + size;
                let ey = y + size;

                pb.move_to(sx, sy);
                pb.line_to(ex, ey);
                pb.close();
            }

            {
                let sx = x - size;
                let sy = y + size;
                let ex = x + size;
                let ey = y - size;

                pb.move_to(sx, sy);
                pb.line_to(ex, ey);
                pb.close();
            }

            pb.finish().unwrap()
        };

        button_icon_paint.anti_alias = true;
        pixmap.stroke_path(
            &x_icon,
            &button_icon_paint,
            &Stroke {
                width: 1.1,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }

    {
        let btn_state = if !maximizable {
            ButtonState::Disabled
        } else if mouses
            .iter()
            .any(|&l| l == Location::Button(ButtonKind::Maximize))
        {
            ButtonState::Hovered
        } else {
            ButtonState::Idle
        };

        let radius = buttons.maximize.radius();

        let x = buttons.maximize.center_x();
        let y = buttons.maximize.center_y();

        let path1 = {
            let mut pb = PathBuilder::new();
            pb.push_circle(x, y, radius);
            pb.finish().unwrap()
        };

        if state == WindowState::Active && btn_state == ButtonState::Hovered {
            pixmap.fill_path(
                &path1,
                &button_hover_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        } else {
            pixmap.fill_path(
                &path1,
                &button_idle_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        let path2 = {
            let size = 8.0;
            let hsize = size / 2.0;
            let mut pb = PathBuilder::new();
            pb.push_rect(x - hsize, y - hsize, size, size);
            pb.finish().unwrap()
        };

        button_icon_paint.anti_alias = false;
        pixmap.stroke_path(
            &path2,
            &button_icon_paint,
            &Stroke {
                width: 1.0,
                ..Default::default()
            },
            Transform::identity(),
            None,
        );
    }

    {
        let btn_state = if mouses
            .iter()
            .any(|&l| l == Location::Button(ButtonKind::Minimize))
        {
            ButtonState::Hovered
        } else {
            ButtonState::Idle
        };

        let radius = buttons.minimize.radius();

        let x = buttons.minimize.center_x();
        let y = buttons.minimize.center_y();

        let path1 = {
            let mut pb = PathBuilder::new();
            pb.push_circle(x, y, radius);
            pb.finish().unwrap()
        };

        if state == WindowState::Active && btn_state == ButtonState::Hovered {
            pixmap.fill_path(
                &path1,
                &button_hover_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        } else {
            pixmap.fill_path(
                &path1,
                &button_idle_paint,
                FillRule::Winding,
                Transform::identity(),
                None,
            );
        }

        button_icon_paint.anti_alias = false;
        pixmap.fill_rect(
            Rect::from_xywh(x - 4.0, y + 4.0, 8.0, 1.0).unwrap(),
            &button_icon_paint,
            Transform::identity(),
            None,
        );
    }

    let buff = pixmap.data();

    for (id, pixel) in canvas.iter_mut().enumerate() {
        *pixel = buff[id];
    }
}

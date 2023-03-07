use tiny_skia::{FillRule, PathBuilder, PixmapMut, Rect, Stroke, Transform};

use smithay_client_toolkit::shell::xdg::window::{WindowManagerCapabilities, WindowState};

use crate::{theme::ColorMap, Location, SkiaResult};

/// The size of the button on the header bar in logical points.
const BUTTON_SIZE: f32 = 24.;
const BUTTON_MARGIN: f32 = 5.;
const BUTTON_SPACING: f32 = 13.;

#[derive(Debug)]
pub(crate) struct Buttons {
    pub close: Button,
    pub maximize: Option<Button>,
    pub minimize: Option<Button>,
}

impl Default for Buttons {
    fn default() -> Self {
        Self {
            close: Button::new(ButtonKind::Close),
            maximize: Some(Button::new(ButtonKind::Maximize)),
            minimize: Some(Button::new(ButtonKind::Minimize)),
        }
    }
}

impl Buttons {
    /// Rearrange the buttons with the new width.
    pub fn arrange(&mut self, width: u32) {
        let mut x = width as f32 - BUTTON_MARGIN;

        for button in [
            Some(&mut self.close),
            self.maximize.as_mut(),
            self.minimize.as_mut(),
        ]
        .into_iter()
        .flatten()
        {
            // Subtract the button size.
            x -= BUTTON_SIZE;

            // Update it's
            button.offset = x;

            // Subtract spacing for the next button.
            x -= BUTTON_SPACING;
        }
    }

    /// Find the coordinate of the button.
    pub fn find_button(&self, x: f64, y: f64) -> Location {
        let x = x as f32;
        let y = y as f32;
        if self.close.contains(x, y) {
            Location::Button(ButtonKind::Close)
        } else if self.maximize.as_ref().map(|b| b.contains(x, y)) == Some(true) {
            Location::Button(ButtonKind::Maximize)
        } else if self.minimize.as_ref().map(|b| b.contains(x, y)) == Some(true) {
            Location::Button(ButtonKind::Minimize)
        } else {
            Location::Head
        }
    }

    pub fn update(&mut self, wm_capabilites: WindowManagerCapabilities) {
        self.maximize = wm_capabilites
            .contains(WindowManagerCapabilities::MAXIMIZE)
            .then_some(Button::new(ButtonKind::Maximize));
        self.minimize = wm_capabilites
            .contains(WindowManagerCapabilities::MINIMIZE)
            .then_some(Button::new(ButtonKind::Minimize));
    }

    pub fn left_most(&self) -> &Button {
        if let Some(minimize) = self.minimize.as_ref() {
            minimize
        } else if let Some(maximize) = self.maximize.as_ref() {
            maximize
        } else {
            &self.close
        }
    }

    pub fn iter(&self) -> std::array::IntoIter<Option<Button>, 3> {
        [
            Some(self.close.clone()),
            self.maximize.clone(),
            self.minimize.clone(),
        ]
        .into_iter()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct Button {
    /// The button offset into the header bar canvas.
    offset: f32,
    /// The kind of the button.
    kind: ButtonKind,
}

impl Button {
    pub fn new(kind: ButtonKind) -> Self {
        Self { offset: 0., kind }
    }

    pub fn radius(&self) -> f32 {
        BUTTON_SIZE / 2.0
    }

    pub fn x(&self) -> f32 {
        self.offset
    }

    pub fn center_x(&self) -> f32 {
        self.offset + self.radius()
    }

    pub fn center_y(&self) -> f32 {
        BUTTON_MARGIN + self.radius()
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        x > self.offset
            && x < self.offset + BUTTON_SIZE
            && y > BUTTON_MARGIN
            && y < BUTTON_MARGIN + BUTTON_SIZE
    }

    pub fn draw(
        &self,
        scale: f32,
        colors: &ColorMap,
        mouse_location: Location,
        pixmap: &mut PixmapMut,
        resizable: bool,
        state: &WindowState,
    ) -> SkiaResult {
        println!("Draw button: {:?}", self);
        let button_bg = if mouse_location == Location::Button(self.kind)
            && (resizable || self.kind != ButtonKind::Maximize)
        {
            colors.button_hover_paint()
        } else {
            colors.button_idle_paint()
        };

        // Convert to pixels.
        let x = self.center_x() * scale;
        let y = self.center_y() * scale;
        let radius = self.radius() * scale;

        // Draw the button background.
        let circle = PathBuilder::from_circle(x, y, radius)?;
        pixmap.fill_path(
            &circle,
            &button_bg,
            FillRule::Winding,
            Transform::identity(),
            None,
        );

        let mut button_icon_paint = colors.button_icon_paint();
        // Do AA only for diagonal lines.
        button_icon_paint.anti_alias = self.kind == ButtonKind::Close;

        // Draw the icon.
        match self.kind {
            ButtonKind::Close => {
                let x_icon = {
                    let size = 3.5 * scale;
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

                    pb.finish()?
                };

                pixmap.stroke_path(
                    &x_icon,
                    &button_icon_paint,
                    &Stroke {
                        width: 1.1 * scale,
                        ..Default::default()
                    },
                    Transform::identity(),
                    None,
                );
            }
            ButtonKind::Maximize => {
                let path2 = {
                    let size = 8.0 * scale;
                    let hsize = size / 2.0;
                    let mut pb = PathBuilder::new();

                    let x = x - hsize;
                    let y = y - hsize;
                    pb.push_rect(x, y, size, size);

                    if state.contains(WindowState::MAXIMIZED) {
                        if let Some(rect) =
                            Rect::from_xywh(x + 2.0 * scale, y - 2.0 * scale, size, size)
                        {
                            pb.move_to(rect.left(), rect.top());
                            pb.line_to(rect.right(), rect.top());
                            pb.line_to(rect.right(), rect.bottom());
                        }
                    }

                    pb.finish()?
                };

                pixmap.stroke_path(
                    &path2,
                    &button_icon_paint,
                    &Stroke {
                        width: 1.0 * scale,
                        ..Default::default()
                    },
                    Transform::identity(),
                    None,
                );
            }
            ButtonKind::Minimize => {
                let len = 8.0 * scale;
                let hlen = len / 2.0;
                pixmap.fill_rect(
                    Rect::from_xywh(x - hlen, y + hlen, len, scale)?,
                    &button_icon_paint,
                    Transform::identity(),
                    None,
                );
            }
        }

        Some(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ButtonKind {
    Close,
    Maximize,
    Minimize,
}

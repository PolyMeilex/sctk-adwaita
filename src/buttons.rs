use crate::Location;

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ButtonKind {
    Close,
    Maximize,
    Minimize,
}

#[derive(Default, Debug)]
pub struct Button {
    x: f32,
    y: f32,
    size: f32,
}

impl Button {
    pub fn radius(&self) -> f32 {
        self.size / 2.0
    }

    pub fn center_x(&self) -> f32 {
        self.x + self.radius()
    }

    pub fn center_y(&self) -> f32 {
        self.y + self.radius()
    }

    fn contains(&self, x: f32, y: f32) -> bool {
        x > self.x && x < self.x + self.size && y > self.y && y < self.y + self.size
    }
}

#[derive(Debug)]
pub struct Buttons {
    pub close: Button,
    pub maximize: Button,
    pub minimize: Button,

    w: u32,
    h: u32,
    scale: u32,
}

impl Default for Buttons {
    fn default() -> Self {
        Self {
            close: Default::default(),
            maximize: Default::default(),
            minimize: Default::default(),
            scale: 1,

            w: 0,
            h: super::theme::HEADER_SIZE,
        }
    }
}

impl Buttons {
    pub fn arrange(&mut self, w: u32) {
        self.w = w;

        let scale = self.scale as f32;
        let margin = 5.0 * scale;
        let spacing = 13.0 * scale;
        let size = 12.0 * 2.0 * scale;

        let mut x = w as f32 * scale - margin;
        let y = margin;

        x -= size;
        self.close.x = x;
        self.close.y = y;
        self.close.size = size;

        x -= size;
        x -= spacing;
        self.maximize.x = x;
        self.maximize.y = y;
        self.maximize.size = size;

        x -= size;
        x -= spacing;
        self.minimize.x = x;
        self.minimize.y = y;
        self.minimize.size = size;
    }

    pub fn update_scale(&mut self, scale: u32) {
        if self.scale != scale {
            self.scale = scale;
            self.arrange(self.w);
        }
    }

    pub fn find_button(&self, x: f64, y: f64) -> Location {
        let x = x as f32 * self.scale as f32;
        let y = y as f32 * self.scale as f32;
        if self.close.contains(x, y) {
            Location::Button(ButtonKind::Close)
        } else if self.maximize.contains(x, y) {
            Location::Button(ButtonKind::Maximize)
        } else if self.minimize.contains(x, y) {
            Location::Button(ButtonKind::Minimize)
        } else {
            Location::Head
        }
    }

    pub fn scaled_size(&self) -> (u32, u32) {
        (self.w * self.scale as u32, self.h * self.scale as u32)
    }
}

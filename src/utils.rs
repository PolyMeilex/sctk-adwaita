pub struct HitBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl HitBox {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }

    pub fn new_f32(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x: x as f64,
            y: y as f64,
            w: w as f64,
            h: h as f64,
        }
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

use std::ops;

/// A point in 2-dimensional space, with each dimension of type `N`.
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Point<N> {
    pub x: N,
    pub y: N,
}

/// A vector in 2-dimensional space, with each dimension of type `N`.
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Vector<N> {
    pub x: N,
    pub y: N,
}

/// A convenience function for generating `Point`s.
#[inline]
pub fn point<N>(x: N, y: N) -> Point<N> {
    Point { x, y }
}
/// A convenience function for generating `Vector`s.
#[inline]
pub fn vector<N>(x: N, y: N) -> Vector<N> {
    Vector { x, y }
}

impl<N: ops::Sub<Output = N>> ops::Sub<Vector<N>> for Point<N> {
    type Output = Point<N>;
    fn sub(self, rhs: Vector<N>) -> Point<N> {
        point(self.x - rhs.x, self.y - rhs.y)
    }
}

/// A rectangle, with top-left corner at `min`, and bottom-right corner at
/// `max`.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rect<N> {
    pub min: Point<N>,
    pub max: Point<N>,
}

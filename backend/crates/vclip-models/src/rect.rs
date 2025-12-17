use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A normalized rectangle (0.0 to 1.0) representing a relative region of a frame.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NormalizedRect {
    /// X coordinate of the top-left corner (0.0 = left, 1.0 = right)
    pub x: f64,
    /// Y coordinate of the top-left corner (0.0 = top, 1.0 = bottom)
    pub y: f64,
    /// Width of the rectangle (0.0 to 1.0)
    pub width: f64,
    /// Height of the rectangle (0.0 to 1.0)
    pub height: f64,
}

impl NormalizedRect {
    /// Create a new normalized rectangle.
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Self {
        Self { x, y, width, height }
    }

    /// Check if the rectangle is valid (within 0.0-1.0 range).
    pub fn is_valid(&self) -> bool {
        self.x >= 0.0
            && self.y >= 0.0
            && self.width > 0.0
            && self.height > 0.0
            && self.x + self.width <= 1.001 // Allow small epsilon for float precision
            && self.y + self.height <= 1.001
    }
}

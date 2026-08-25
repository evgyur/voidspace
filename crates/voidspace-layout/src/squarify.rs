use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub min_x: f32,
    pub min_y: f32,
    pub max_x: f32,
    pub max_y: f32,
}

impl Rect {
    pub const fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min_x,
            min_y,
            max_x,
            max_y,
        }
    }

    pub fn width(self) -> f32 {
        (self.max_x - self.min_x).max(0.0)
    }

    pub fn height(self) -> f32 {
        (self.max_y - self.min_y).max(0.0)
    }

    pub fn area(self) -> f32 {
        self.width() * self.height()
    }

    pub fn contains(self, other: Self) -> bool {
        other.min_x >= self.min_x - 0.01
            && other.min_y >= self.min_y - 0.01
            && other.max_x <= self.max_x + 0.01
            && other.max_y <= self.max_y + 0.01
    }

    pub fn contains_point(self, x: f32, y: f32) -> bool {
        x >= self.min_x && x <= self.max_x && y >= self.min_y && y <= self.max_y
    }
}

/// Stable squarified treemap for positive weights. Results preserve input order.
pub fn layout_weights(weights: &[u64], bounds: Rect) -> Vec<Rect> {
    if weights.is_empty() || bounds.area() <= 0.0 {
        return vec![Rect::default(); weights.len()];
    }
    let total: f64 = weights.iter().map(|weight| *weight as f64).sum();
    if total <= 0.0 {
        return vec![Rect::default(); weights.len()];
    }
    let scale = f64::from(bounds.area()) / total;
    let mut remaining_items: Vec<(usize, f64)> = weights
        .iter()
        .enumerate()
        .map(|(index, weight)| (index, *weight as f64 * scale))
        .collect();
    remaining_items.sort_by(|(left_index, left), (right_index, right)| {
        right
            .total_cmp(left)
            .then_with(|| left_index.cmp(right_index))
    });

    let mut result = vec![Rect::default(); weights.len()];
    let mut remaining = bounds;
    let mut row: Vec<(usize, f64)> = Vec::new();

    while let Some(next) = remaining_items.first().copied() {
        let side = f64::from(remaining.width().min(remaining.height())).max(f64::EPSILON);
        if row.is_empty() || worst(&row, side) >= worst_with(&row, next, side) {
            row.push(next);
            remaining_items.remove(0);
        } else {
            remaining = place_row(&row, remaining, &mut result);
            row.clear();
        }
    }
    if !row.is_empty() {
        place_row(&row, remaining, &mut result);
    }
    result
}

fn worst(row: &[(usize, f64)], side: f64) -> f64 {
    if row.is_empty() {
        return f64::INFINITY;
    }
    let sum: f64 = row.iter().map(|(_, area)| *area).sum();
    let min = row
        .iter()
        .map(|(_, area)| *area)
        .fold(f64::INFINITY, f64::min);
    let max = row.iter().map(|(_, area)| *area).fold(0.0, f64::max);
    ((side * side * max) / (sum * sum)).max((sum * sum) / (side * side * min))
}

fn worst_with(row: &[(usize, f64)], next: (usize, f64), side: f64) -> f64 {
    let mut candidate = row.to_vec();
    candidate.push(next);
    worst(&candidate, side)
}

fn place_row(row: &[(usize, f64)], bounds: Rect, output: &mut [Rect]) -> Rect {
    let sum: f64 = row.iter().map(|(_, area)| *area).sum();
    if bounds.width() >= bounds.height() {
        let row_height = (sum / f64::from(bounds.width())).max(0.0) as f32;
        let mut x = bounds.min_x;
        for (position, (index, area)) in row.iter().enumerate() {
            let width = if position + 1 == row.len() {
                bounds.max_x - x
            } else {
                (*area / f64::from(row_height.max(f32::EPSILON))) as f32
            };
            output[*index] = Rect::new(x, bounds.min_y, x + width, bounds.min_y + row_height);
            x += width;
        }
        Rect::new(
            bounds.min_x,
            (bounds.min_y + row_height).min(bounds.max_y),
            bounds.max_x,
            bounds.max_y,
        )
    } else {
        let column_width = (sum / f64::from(bounds.height())).max(0.0) as f32;
        let mut y = bounds.min_y;
        for (position, (index, area)) in row.iter().enumerate() {
            let height = if position + 1 == row.len() {
                bounds.max_y - y
            } else {
                (*area / f64::from(column_width.max(f32::EPSILON))) as f32
            };
            output[*index] = Rect::new(bounds.min_x, y, bounds.min_x + column_width, y + height);
            y += height;
        }
        Rect::new(
            (bounds.min_x + column_width).min(bounds.max_x),
            bounds.min_y,
            bounds.max_x,
            bounds.max_y,
        )
    }
}

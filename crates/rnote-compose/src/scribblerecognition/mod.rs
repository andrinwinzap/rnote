// Imports
use crate::penpath::PenPath;
use p2d::math::Vector2;

/// The minimum number of flattened input points needed to attempt a recognition.
const MIN_INPUT_POINTS: usize = 12;
/// The minimum extent (in surface coordinates) of the stroke bounds on its larger axis for the
/// stroke to be a scribble. Small marks stay strokes, whatever their shape.
const MIN_EXTENT_SURFACE: f64 = 30.0;
/// The minimum length of the stroke relative to the diagonal of its bounds.
///
/// A scribble goes over the same place again and again, so it is much longer than the area it
/// covers. Together with the reversals below this keeps ordinary strokes from being scribbles.
const MIN_LENGTH_DIAG_RATIO: f64 = 2.0;
/// The number of times the stroke has to reverse its direction along its dominant axis.
///
/// This is what separates a scribble from handwriting: writing advances along its own direction and
/// rarely turns back, while a scribble runs back and forth over the same spot.
const MIN_REVERSALS: usize = 3;
/// How far the stroke has to turn back, relative to its extent along the dominant axis, to count as
/// a reversal. Keeps the wobble of a shaky hand from counting.
const REVERSAL_PROMINENCE_RATIO: f64 = 0.2;
/// The smallest turn back (in surface coordinates) that counts as a reversal, whatever the extent.
const REVERSAL_MIN_PROMINENCE_SURFACE: f64 = 8.0;

/// Whether the drawn pen path is a scribble, i.e. the gesture of scratching something out.
///
/// `zoom` is the zoom of the view the path was drawn in, used for the thresholds that are defined
/// in surface coordinates so they mean the same thing on screen at any zoom.
pub fn is_scribble(path: &PenPath, zoom: f64) -> bool {
    let points = flattened_points(path);
    if points.len() < MIN_INPUT_POINTS {
        return false;
    }

    let (min, max) = points_bounds(&points);
    let extents = max - min;
    if extents.x.max(extents.y) < MIN_EXTENT_SURFACE / zoom {
        return false;
    }
    let diagonal = extents.length();
    if diagonal < f64::EPSILON {
        return false;
    }

    if polyline_len(&points) < MIN_LENGTH_DIAG_RATIO * diagonal {
        return false;
    }

    // Project onto the dominant axis and count how often the stroke turns back along it.
    let axis = dominant_axis(&points);
    let projected = points.iter().map(|p| p.dot(axis)).collect::<Vec<f64>>();
    let span = projected
        .iter()
        .fold(f64::NEG_INFINITY, |acc, v| acc.max(*v))
        - projected.iter().fold(f64::INFINITY, |acc, v| acc.min(*v));
    let prominence = (REVERSAL_PROMINENCE_RATIO * span).max(REVERSAL_MIN_PROMINENCE_SURFACE / zoom);

    count_reversals(&projected, prominence) >= MIN_REVERSALS
}

/// Extract the flattened points of the pen path.
fn flattened_points(path: &PenPath) -> Vec<Vector2> {
    let mut points = Vec::new();

    for el in path.to_kurbo_flattened(0.25).elements() {
        match el {
            kurbo::PathEl::MoveTo(p) | kurbo::PathEl::LineTo(p) => {
                points.push(Vector2::new(p.x, p.y));
            }
            _ => {}
        }
    }

    points.dedup_by(|a, b| (*a - *b).length() < f64::EPSILON);
    points
}

/// The min/max corners of the bounds of the given points.
fn points_bounds(points: &[Vector2]) -> (Vector2, Vector2) {
    let mut min = points[0];
    let mut max = points[0];

    for p in points {
        min = min.min(*p);
        max = max.max(*p);
    }

    (min, max)
}

/// The total length of the polyline through the given points.
fn polyline_len(points: &[Vector2]) -> f64 {
    points.windows(2).map(|w| (w[1] - w[0]).length()).sum()
}

/// The direction the points spread out in the most, as the principal axis of their covariance.
fn dominant_axis(points: &[Vector2]) -> Vector2 {
    let n = points.len() as f64;
    let mean = points.iter().fold(Vector2::ZERO, |acc, p| acc + *p) / n;
    let (mut xx, mut xy, mut yy) = (0.0, 0.0, 0.0);

    for p in points {
        let d = *p - mean;
        xx += d.x * d.x;
        xy += d.x * d.y;
        yy += d.y * d.y;
    }

    // Angle of the eigenvector of the larger eigenvalue of [[xx, xy], [xy, yy]].
    let angle = 0.5 * (2.0 * xy).atan2(xx - yy);

    Vector2::new(angle.cos(), angle.sin())
}

/// How often the values turn back on themselves by at least `prominence`.
///
/// Smaller wiggles are ignored, so that the shaky hand of a slowly drawn stroke does not add up to
/// a scribble.
fn count_reversals(values: &[f64], prominence: f64) -> usize {
    #[derive(PartialEq)]
    enum Direction {
        Unknown,
        Rising,
        Falling,
    }

    let mut reversals = 0;
    let mut direction = Direction::Unknown;
    let mut extreme = values[0];

    for &v in values {
        match direction {
            Direction::Unknown => {
                if (v - extreme).abs() >= prominence {
                    direction = if v > extreme {
                        Direction::Rising
                    } else {
                        Direction::Falling
                    };
                    extreme = v;
                }
            }
            Direction::Rising => {
                if v > extreme {
                    extreme = v;
                } else if extreme - v >= prominence {
                    reversals += 1;
                    direction = Direction::Falling;
                    extreme = v;
                }
            }
            Direction::Falling => {
                if v < extreme {
                    extreme = v;
                } else if v - extreme >= prominence {
                    reversals += 1;
                    direction = Direction::Rising;
                    extreme = v;
                }
            }
        }
    }

    reversals
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::penpath::Element;

    fn pen_path_from_points(points: impl IntoIterator<Item = Vector2>) -> PenPath {
        PenPath::try_from_elements(points.into_iter().map(|p| Element::new(p, 0.5))).unwrap()
    }

    /// Deterministic pseudo-random jitter to simulate hand wobble.
    fn jitter(i: usize, magnitude: f64) -> Vector2 {
        let a = ((i as f64) * 12.9898).sin() * 43758.5453;
        let b = ((i as f64) * 78.233).sin() * 24634.6345;
        Vector2::new(a.fract(), b.fract()) * magnitude
    }

    /// A scratch-out: `passes` strokes back and forth over the same band.
    fn scribble_points(passes: usize, width: f64, height: f64) -> Vec<Vector2> {
        let mut points = Vec::new();

        for pass in 0..passes {
            let forward = pass % 2 == 0;
            for step in 0..=20 {
                let t = step as f64 / 20.0;
                let x = if forward { t } else { 1.0 - t };
                points.push(Vector2::new(
                    100.0 + x * width,
                    100.0 + (pass as f64 / passes as f64) * height,
                ));
            }
        }

        points
    }

    #[test]
    fn recognizes_a_scratch_out() {
        let points = scribble_points(6, 160.0, 40.0)
            .into_iter()
            .enumerate()
            .map(|(i, p)| p + jitter(i, 1.5));

        assert!(is_scribble(&pen_path_from_points(points), 1.0));
    }

    #[test]
    fn a_straight_line_is_no_scribble() {
        let points = (0..=40).map(|i| Vector2::new(100.0 + 5.0 * i as f64, 100.0) + jitter(i, 1.0));

        assert!(!is_scribble(&pen_path_from_points(points), 1.0));
    }

    #[test]
    fn handwriting_is_no_scribble() {
        // A cursive "mmmm": it oscillates across the writing direction, but keeps advancing
        // along it instead of running back over itself.
        let points = (0..=160).map(|i| {
            let t = i as f64 / 160.0;
            Vector2::new(
                100.0 + t * 200.0,
                100.0 + 20.0 * (t * 8.0 * std::f64::consts::PI).sin(),
            ) + jitter(i, 0.8)
        });

        assert!(!is_scribble(&pen_path_from_points(points), 1.0));
    }

    #[test]
    fn a_circle_is_no_scribble() {
        let points = (0..=80).map(|i| {
            let angle = (i as f64 / 80.0) * std::f64::consts::TAU;
            Vector2::new(100.0 + 50.0 * angle.cos(), 100.0 + 50.0 * angle.sin()) + jitter(i, 1.0)
        });

        assert!(!is_scribble(&pen_path_from_points(points), 1.0));
    }

    #[test]
    fn a_tiny_scratch_is_no_scribble() {
        // The same gesture, but only a few millimeters wide: too small to be meant as an erase.
        let points = scribble_points(6, 20.0, 6.0);

        assert!(!is_scribble(&pen_path_from_points(points), 1.0));
    }

    #[test]
    fn a_small_scratch_counts_when_zoomed_in() {
        // Zoomed in, the same document-space gesture covers enough of the screen.
        let points = scribble_points(6, 20.0, 6.0);

        assert!(is_scribble(&pen_path_from_points(points), 4.0));
    }

    #[test]
    fn two_passes_are_not_enough() {
        // Going over something once and back is a correction stroke, not a scratch-out.
        let points = scribble_points(2, 160.0, 15.0);

        assert!(!is_scribble(&pen_path_from_points(points), 1.0));
    }
}

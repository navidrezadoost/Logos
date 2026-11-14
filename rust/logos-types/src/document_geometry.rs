//! Pure coordinate-space conversions for the candidate document contract.
//!
//! Matrices use `logos-layout::Matrix`'s column-vector convention. A node's
//! `local_to_parent` maps its local coordinates into its immediate parent's
//! coordinates. A chain is provided in root-to-leaf order. The module does
//! not decide how an editor derives those matrices from shape records.

use logos_layout::{Matrix, Point, Rect};

/// A matrix cannot safely participate in coordinate conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateError {
    NonFiniteMatrix,
    SingularMatrix,
    NonFinitePoint,
    NonFiniteRect,
    NonFiniteResult,
}

/// Compose local-to-parent matrices supplied in root-to-leaf order.
pub fn local_to_world(chain: &[Matrix]) -> Result<Matrix, CoordinateError> {
    let mut world = Matrix::identity();
    for local_to_parent in chain {
        ensure_finite_matrix(local_to_parent)?;
        world = world * *local_to_parent;
    }
    ensure_finite_matrix(&world)?;
    Ok(world)
}

/// Map a point from an object's local coordinates into page/world coordinates.
pub fn point_to_world(point: Point, local_to_world: Matrix) -> Result<Point, CoordinateError> {
    ensure_finite_point(point)?;
    ensure_finite_matrix(&local_to_world)?;
    let mapped = local_to_world.transform_point(point);
    ensure_finite_point(mapped)?;
    Ok(mapped)
}

/// Map a point from page/world coordinates into a parent's local coordinates.
pub fn point_to_local(point: Point, parent_to_world: Matrix) -> Result<Point, CoordinateError> {
    ensure_finite_point(point)?;
    ensure_finite_matrix(&parent_to_world)?;
    let inverse = parent_to_world
        .inverse()
        .ok_or(CoordinateError::SingularMatrix)?;
    point_to_world(point, inverse)
}

/// Transform an axis-aligned rectangle and return the enclosing axis-aligned
/// rectangle in the destination space. Rotation and skew therefore preserve
/// the four transformed corners, not an assumed width/height orientation.
pub fn rect_to_space(rect: Rect, transform: Matrix) -> Result<Rect, CoordinateError> {
    ensure_finite_rect(rect)?;
    ensure_finite_matrix(&transform)?;
    let corners = rect.to_points();
    let mut points = [Point::new(0.0, 0.0); 4];
    for (index, corner) in corners.into_iter().enumerate() {
        points[index] = point_to_world(corner, transform)?;
    }
    let min_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::INFINITY, f64::min);
    let min_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::INFINITY, f64::min);
    let max_x = points
        .iter()
        .map(|point| point.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let max_y = points
        .iter()
        .map(|point| point.y)
        .fold(f64::NEG_INFINITY, f64::max);
    let result = Rect::new(min_x, min_y, max_x - min_x, max_y - min_y);
    ensure_finite_rect(result)?;
    Ok(result)
}

/// Convert an object's local-to-parent matrix to local-to-world.
pub fn object_to_world(
    parent_to_world: Matrix,
    object_to_parent: Matrix,
) -> Result<Matrix, CoordinateError> {
    ensure_finite_matrix(&parent_to_world)?;
    ensure_finite_matrix(&object_to_parent)?;
    let world = parent_to_world * object_to_parent;
    ensure_finite_matrix(&world)?;
    Ok(world)
}

/// Convert an object's local-to-world matrix into a new parent's local space.
pub fn object_to_new_parent(
    object_to_world: Matrix,
    new_parent_to_world: Matrix,
) -> Result<Matrix, CoordinateError> {
    ensure_finite_matrix(&object_to_world)?;
    ensure_finite_matrix(&new_parent_to_world)?;
    let inverse = new_parent_to_world
        .inverse()
        .ok_or(CoordinateError::SingularMatrix)?;
    let converted = inverse * object_to_world;
    ensure_finite_matrix(&converted)?;
    Ok(converted)
}

/// Reparent an object while preserving its world-space affine transform.
pub fn reparent_preserving_world(
    old_parent_to_world: Matrix,
    object_to_old_parent: Matrix,
    new_parent_to_world: Matrix,
) -> Result<Matrix, CoordinateError> {
    let object_world = object_to_world(old_parent_to_world, object_to_old_parent)?;
    object_to_new_parent(object_world, new_parent_to_world)
}

fn ensure_finite_matrix(matrix: &Matrix) -> Result<(), CoordinateError> {
    if [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f]
        .iter()
        .all(|value| value.is_finite())
    {
        Ok(())
    } else {
        Err(CoordinateError::NonFiniteMatrix)
    }
}

fn ensure_finite_point(point: Point) -> Result<(), CoordinateError> {
    if point.x.is_finite() && point.y.is_finite() {
        Ok(())
    } else {
        Err(CoordinateError::NonFinitePoint)
    }
}

fn ensure_finite_rect(rect: Rect) -> Result<(), CoordinateError> {
    if [rect.x, rect.y, rect.width, rect.height]
        .iter()
        .all(|value| value.is_finite())
        && rect.width >= 0.0
        && rect.height >= 0.0
    {
        Ok(())
    } else {
        Err(CoordinateError::NonFiniteRect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: f64 = 1.0e-6;

    fn close(left: f64, right: f64) {
        assert!((left - right).abs() <= TOLERANCE, "{left} != {right}");
    }

    fn matrix(values: [f64; 6]) -> Matrix {
        Matrix::new(
            values[0], values[1], values[2], values[3], values[4], values[5],
        )
    }

    #[test]
    fn golden_root_and_translated_nested_coordinate_cases() {
        let identity = Matrix::identity();
        let translated = Matrix::translate_dist(100.0, 50.0);
        let nested = Matrix::translate_dist(20.0, 10.0);
        let combined = local_to_world(&[translated, nested]).unwrap();
        assert_eq!(local_to_world(&[]).unwrap(), identity);
        assert_eq!(combined, Matrix::translate_dist(120.0, 60.0));
        let world = point_to_world(Point::new(3.0, 4.0), combined).unwrap();
        close(world.x, 123.0);
        close(world.y, 64.0);
        let local = point_to_local(world, combined).unwrap();
        close(local.x, 3.0);
        close(local.y, 4.0);
    }

    #[test]
    fn golden_rotated_scaled_and_composed_matrices() {
        let rotate = Matrix::rotate_matrix(90.0);
        let scaled = Matrix::scale_matrix(Point::new(2.0, 3.0));
        let result = local_to_world(&[Matrix::translate_dist(10.0, 5.0), rotate, scaled]).unwrap();
        let point = point_to_world(Point::new(2.0, 1.0), result).unwrap();
        close(point.x, 7.0);
        close(point.y, 9.0);

        let rect = rect_to_space(Rect::new(0.0, 0.0, 10.0, 20.0), rotate).unwrap();
        close(rect.x, -20.0);
        close(rect.y, 0.0);
        close(rect.width, 20.0);
        close(rect.height, 10.0);

        let arbitrary = matrix([0.0, 1.0, -1.0, 0.0, 10.0, 5.0]);
        assert!(arbitrary.close_to(&(Matrix::translate_dist(10.0, 5.0) * rotate), TOLERANCE));
    }

    #[test]
    fn golden_negative_coordinates_and_combined_rotation_translation() {
        let transform = Matrix::translate_dist(-15.0, 8.0) * Matrix::rotate_matrix(90.0);
        let point = point_to_world(Point::new(-2.0, 3.0), transform).unwrap();
        close(point.x, -18.0);
        close(point.y, 6.0);
    }

    #[test]
    fn golden_reparenting_preserves_world_transform() {
        let old_parent = Matrix::translate_dist(100.0, -20.0);
        let object = Matrix::translate_dist(12.0, 7.0) * Matrix::rotate_matrix(30.0);
        let new_parent =
            Matrix::translate_dist(-40.0, 60.0) * Matrix::scale_matrix(Point::new(2.0, 2.0));
        let converted = reparent_preserving_world(old_parent, object, new_parent).unwrap();
        let old_world = object_to_world(old_parent, object).unwrap();
        let new_world = object_to_world(new_parent, converted).unwrap();
        assert!(old_world.close_to(&new_world, TOLERANCE));
    }

    #[test]
    fn singular_non_finite_and_negative_size_inputs_fail_closed() {
        let singular = Matrix::scale_matrix(Point::new(0.0, 1.0));
        assert_eq!(
            point_to_local(Point::new(1.0, 2.0), singular),
            Err(CoordinateError::SingularMatrix)
        );
        assert_eq!(
            point_to_world(
                Point::new(1.0, 2.0),
                Matrix::new(f64::NAN, 0.0, 0.0, 1.0, 0.0, 0.0)
            ),
            Err(CoordinateError::NonFiniteMatrix),
        );
        assert_eq!(
            rect_to_space(Rect::new(0.0, 0.0, -1.0, 2.0), Matrix::identity()),
            Err(CoordinateError::NonFiniteRect),
        );
        assert_eq!(
            reparent_preserving_world(Matrix::identity(), Matrix::identity(), singular),
            Err(CoordinateError::SingularMatrix),
        );
    }
}

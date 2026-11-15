use glam::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Ray {
    pub origin: Vec3,
    pub direction: Vec3, // Normalized
}

#[derive(Clone, Copy, Debug)]
pub struct RayHit {
    pub target_id: u64,
    pub distance: f32,
    pub hit_point: Vec3,
    pub offset_from_center: f32, // Distance from crosshair ray to target sphere center
}

/// Ray-Sphere intersection test
/// Returns Some((distance, hit_point, offset_from_center)) if ray intersects sphere
pub fn intersect_ray_sphere(ray: Ray, center: Vec3, radius: f32) -> Option<(f32, Vec3, f32)> {
    let oc = ray.origin - center;
    let b = oc.dot(ray.direction);
    let c = oc.dot(oc) - radius * radius;

    // Ray origin is outside sphere and pointing away
    if c > 0.0 && b > 0.0 {
        return None;
    }

    let discriminant = b * b - c;
    if discriminant < 0.0 {
        return None;
    }

    let sqrt_disc = discriminant.sqrt();
    let mut t = -b - sqrt_disc;

    if t < 0.0 {
        t = -b + sqrt_disc;
    }

    if t >= 0.0 {
        let hit_point = ray.origin + ray.direction * t;
        // Orthogonal distance from center of sphere to ray
        let projection = center - ray.origin;
        let proj_on_ray = projection.dot(ray.direction);
        let closest_point_on_ray = ray.origin + ray.direction * proj_on_ray;
        let offset = (center - closest_point_on_ray).length();

        Some((t, hit_point, offset))
    } else {
        None
    }
}

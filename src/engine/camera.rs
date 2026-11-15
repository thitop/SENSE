use glam::{Mat4, Vec3};

pub struct Camera {
    pub position: Vec3,
    pub yaw: f32,   // in radians (horizontal rotation)
    pub pitch: f32, // in radians (vertical rotation)
    pub fov_y_rad: f32,
    pub aspect_ratio: f32,
    pub z_near: f32,
    pub z_far: f32,
}

impl Camera {
    pub fn new(aspect_ratio: f32) -> Self {
        Self {
            position: Vec3::new(0.0, 1.5, 0.0), // ~eye height 1.5m
            yaw: -std::f32::consts::FRAC_PI_2,  // facing down -Z axis
            pitch: 0.0,
            fov_y_rad: 70.0_f32.to_radians(), // 70 deg vertical FOV (approx 103 horizontal 16:9 like Valorant/Overwatch)
            aspect_ratio,
            z_near: 0.01,
            z_far: 200.0,
        }
    }

    pub fn update_aspect_ratio(&mut self, width: u32, height: u32) {
        if height > 0 {
            self.aspect_ratio = width as f32 / height as f32;
        }
    }

    /// Rotate camera based on mouse delta and angular sensitivity (radians per count)
    pub fn rotate(&mut self, delta_x: f32, delta_y: f32, rad_per_count: f32) {
        self.yaw += delta_x * rad_per_count;
        // Invert delta_y because screen coordinates are top-to-bottom
        self.pitch -= delta_y * rad_per_count;

        // Clamp pitch to ~[-89.5, 89.5] degrees to avoid gimbal flip
        let max_pitch = 89.5_f32.to_radians();
        self.pitch = self.pitch.clamp(-max_pitch, max_pitch);

        // Keep yaw within [-PI, PI] or [0, 2PI]
        let two_pi = std::f32::consts::TAU;
        self.yaw = (self.yaw + std::f32::consts::PI).rem_euclid(two_pi) - std::f32::consts::PI;
    }

    pub fn forward(&self) -> Vec3 {
        let (sin_y, cos_y) = self.yaw.sin_cos();
        let (sin_p, cos_p) = self.pitch.sin_cos();
        Vec3::new(cos_p * cos_y, sin_p, cos_p * sin_y).normalize()
    }

    pub fn right(&self) -> Vec3 {
        self.forward().cross(Vec3::Y).normalize()
    }

    pub fn up(&self) -> Vec3 {
        self.right().cross(self.forward()).normalize()
    }

    pub fn view_matrix(&self) -> Mat4 {
        Mat4::look_to_rh(self.position, self.forward(), Vec3::Y)
    }

    pub fn proj_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.fov_y_rad, self.aspect_ratio, self.z_near, self.z_far)
    }

    pub fn view_proj_matrix(&self) -> Mat4 {
        self.proj_matrix() * self.view_matrix()
    }
}


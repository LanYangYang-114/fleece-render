use crate::math::Vec3;

#[derive(Clone, Copy, Debug)]
pub struct Projected {
    pub x: f32,
    pub y: f32,
    pub depth: f32,
}

#[derive(Clone, Debug)]
pub struct Camera {
    pub position: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub fov_y_degrees: f32,
    pub aspect: f32,
    pub width: u32,
    pub height: u32,
    right: Vec3,
    true_up: Vec3,
    forward: Vec3,
    tan_half_fov: f32,
}

impl Camera {
    pub fn new(position: Vec3, target: Vec3, fov_y_degrees: f32, width: u32, height: u32) -> Self {
        let forward = (target - position).normalized();
        let right = forward.cross(Vec3::new(0.0, 1.0, 0.0)).normalized();
        let true_up = right.cross(forward).normalized();
        let tan_half_fov = (fov_y_degrees.to_radians() * 0.5).tan();
        Self {
            position,
            target,
            up: Vec3::new(0.0, 1.0, 0.0),
            fov_y_degrees,
            aspect: width as f32 / height as f32,
            width,
            height,
            right,
            true_up,
            forward,
            tan_half_fov,
        }
    }

    pub fn forward(&self) -> Vec3 {
        self.forward
    }

    pub fn focal_y(&self) -> f32 {
        self.height as f32 * 0.5 / self.tan_half_fov
    }

    pub fn project(&self, point: Vec3) -> Option<Projected> {
        let delta = point - self.position;
        let depth = delta.dot(self.forward);
        if depth <= 0.05 {
            return None;
        }
        let x_view = delta.dot(self.right);
        let y_view = delta.dot(self.true_up);
        let ndc_x = x_view / (depth * self.tan_half_fov * self.aspect);
        let ndc_y = y_view / (depth * self.tan_half_fov);
        Some(Projected {
            x: (ndc_x * 0.5 + 0.5) * self.width as f32,
            y: (0.5 - ndc_y * 0.5) * self.height as f32,
            depth,
        })
    }

    pub fn projected_size(&self, world_height: f32, depth: f32) -> f32 {
        if depth <= f32::EPSILON {
            0.0
        } else {
            world_height * self.focal_y() / depth
        }
    }
}

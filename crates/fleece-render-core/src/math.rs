use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Vec3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vec3 {
    pub const ZERO: Self = Self::new(0.0, 0.0, 0.0);
    pub const ONE: Self = Self::new(1.0, 1.0, 1.0);

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    pub fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    pub fn cross(self, rhs: Self) -> Self {
        Self::new(
            self.y * rhs.z - self.z * rhs.y,
            self.z * rhs.x - self.x * rhs.z,
            self.x * rhs.y - self.y * rhs.x,
        )
    }

    pub fn length_squared(self) -> f32 {
        self.dot(self)
    }

    pub fn length(self) -> f32 {
        self.length_squared().sqrt()
    }

    pub fn normalized(self) -> Self {
        let len = self.length();
        if len <= f32::EPSILON {
            Self::ZERO
        } else {
            self / len
        }
    }

    pub fn lerp(self, rhs: Self, t: f32) -> Self {
        self + (rhs - self) * t
    }
}

impl Add for Vec3 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for Vec3 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Mul<f32> for Vec3 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

impl Mul<Vec3> for f32 {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Self::Output {
        rhs * self
    }
}

impl Div<f32> for Vec3 {
    type Output = Self;
    fn div(self, rhs: f32) -> Self::Output {
        Self::new(self.x / rhs, self.y / rhs, self.z / rhs)
    }
}

impl Neg for Vec3 {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y, -self.z)
    }
}

/// Row-major 4x4 matrix used for the small model hierarchy.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mat4(pub [f32; 16]);

impl Default for Mat4 {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Mat4 {
    pub const IDENTITY: Self = Self([
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]);

    pub fn translation(v: Vec3) -> Self {
        let mut m = Self::IDENTITY;
        m.0[3] = v.x;
        m.0[7] = v.y;
        m.0[11] = v.z;
        m
    }

    pub fn scale(v: Vec3) -> Self {
        let mut m = Self::IDENTITY;
        m.0[0] = v.x;
        m.0[5] = v.y;
        m.0[10] = v.z;
        m
    }

    pub fn rotation_x(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        let mut m = Self::IDENTITY;
        m.0[5] = c;
        m.0[6] = -s;
        m.0[9] = s;
        m.0[10] = c;
        m
    }

    pub fn rotation_y(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        let mut m = Self::IDENTITY;
        m.0[0] = c;
        m.0[2] = s;
        m.0[8] = -s;
        m.0[10] = c;
        m
    }

    pub fn rotation_z(angle: f32) -> Self {
        let (s, c) = angle.sin_cos();
        let mut m = Self::IDENTITY;
        m.0[0] = c;
        m.0[1] = -s;
        m.0[4] = s;
        m.0[5] = c;
        m
    }

    /// Three.js Euler order `XYZ`.
    pub fn rotation_xyz(rotation: Vec3) -> Self {
        Self::rotation_x(rotation.x) * Self::rotation_y(rotation.y) * Self::rotation_z(rotation.z)
    }

    pub fn transform_point(self, p: Vec3) -> Vec3 {
        let m = self.0;
        let x = m[0] * p.x + m[1] * p.y + m[2] * p.z + m[3];
        let y = m[4] * p.x + m[5] * p.y + m[6] * p.z + m[7];
        let z = m[8] * p.x + m[9] * p.y + m[10] * p.z + m[11];
        let w = m[12] * p.x + m[13] * p.y + m[14] * p.z + m[15];
        if w.abs() > f32::EPSILON {
            Vec3::new(x / w, y / w, z / w)
        } else {
            Vec3::new(x, y, z)
        }
    }

    pub fn transform_vector(self, p: Vec3) -> Vec3 {
        let m = self.0;
        Vec3::new(
            m[0] * p.x + m[1] * p.y + m[2] * p.z,
            m[4] * p.x + m[5] * p.y + m[6] * p.z,
            m[8] * p.x + m[9] * p.y + m[10] * p.z,
        )
    }
}

impl Mul for Mat4 {
    type Output = Self;
    fn mul(self, rhs: Self) -> Self::Output {
        let a = self.0;
        let b = rhs.0;
        let mut out = [0.0f32; 16];
        for row in 0..4 {
            for col in 0..4 {
                out[row * 4 + col] = (0..4).map(|k| a[row * 4 + k] * b[k * 4 + col]).sum();
            }
        }
        Self(out)
    }
}

impl Mul<Vec3> for Mat4 {
    type Output = Vec3;
    fn mul(self, rhs: Vec3) -> Self::Output {
        self.transform_point(rhs)
    }
}

use std::f32::consts::PI;
use std::fmt;

use crate::error::{CoreError, Result};
use crate::geometry::{BoxStyle, Scene, Sprite, cape_uvs, push_box, skin_uvs};
use crate::math::{Mat4, Vec3};
use crate::types::{BackEquipment, ModelOverride, RenderOptions, SkinAssets, View};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelKind {
    Classic,
    Slim,
}

impl fmt::Display for ModelKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Classic => "classic",
            Self::Slim => "slim",
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SkinModel {
    pub kind: ModelKind,
    pub head_rotation_x: f32,
    pub left_arm_rotation: Vec3,
    pub right_arm_rotation: Vec3,
    pub left_leg_rotation: Vec3,
    pub right_leg_rotation: Vec3,
    pub cape_rotation_x: f32,
}

impl SkinModel {
    pub fn for_view(kind: ModelKind, view: View) -> Self {
        let phase = match view {
            View::Front => 10.0,
            View::Back => 12.0,
        };
        let basic_z = PI * 0.02;
        let left_arm_x = (phase + PI).sin() * 0.5;
        let right_arm_x = phase.sin() * 0.5;
        Self {
            kind,
            head_rotation_x: match view {
                View::Front => -0.2,
                View::Back => 0.0,
            },
            left_arm_rotation: Vec3::new(left_arm_x, 0.0, phase.cos() * 0.03 + basic_z),
            right_arm_rotation: Vec3::new(right_arm_x, 0.0, (phase + PI).cos() * 0.03 - basic_z),
            left_leg_rotation: Vec3::new((phase + PI).sin() * 0.5, 0.0, 0.0),
            right_leg_rotation: Vec3::new(phase.sin() * 0.5, 0.0, 0.0),
            cape_rotation_x: (phase / 1.5).sin() * 0.06 + PI * 0.06,
        }
    }
}

pub fn build_scene(
    assets: &SkinAssets,
    options: &RenderOptions,
    view: View,
) -> Result<(Scene, ModelKind)> {
    let model_kind = match options.model {
        ModelOverride::AutoDetect => assets.skin.infer_model(),
        ModelOverride::Classic => ModelKind::Classic,
        ModelOverride::Slim => ModelKind::Slim,
    };
    let model = SkinModel::for_view(model_kind, view);

    let mut scene = Scene::default();
    let skin_texture = scene.add_texture(assets.skin.clone());
    let mut back_texture = None;
    if let Some(back) = &assets.back {
        back_texture = Some(scene.add_texture(back.clone()));
    }

    let root = Mat4::translation(Vec3::new(0.0, 8.0, 0.0));
    let inner_style = |uvs, double_sided, alpha_blend| BoxStyle {
        uvs,
        double_sided,
        alpha_blend,
    };

    // Head and hat layer.
    let head_group = root * Mat4::rotation_x(model.head_rotation_x);
    let head_inner = head_group * Mat4::translation(Vec3::new(0.0, 4.0, 0.0));
    push_box(
        &mut scene,
        skin_texture,
        head_inner,
        Vec3::new(8.0, 8.0, 8.0),
        &inner_style(skin_uvs(0.0, 0.0, 8.0, 8.0, 8.0), false, false),
    );
    if options.show_outer_layer {
        push_box(
            &mut scene,
            skin_texture,
            head_inner,
            Vec3::new(9.0, 9.0, 9.0),
            &inner_style(skin_uvs(32.0, 0.0, 8.0, 8.0, 8.0), true, true),
        );
    }

    // Torso and jacket layer.
    let body_group = root * Mat4::translation(Vec3::new(0.0, -6.0, 0.0));
    push_box(
        &mut scene,
        skin_texture,
        body_group,
        Vec3::new(8.0, 12.0, 4.0),
        &inner_style(skin_uvs(16.0, 16.0, 8.0, 12.0, 4.0), false, false),
    );
    if options.show_outer_layer {
        push_box(
            &mut scene,
            skin_texture,
            body_group,
            Vec3::new(8.5, 12.5, 4.5),
            &inner_style(skin_uvs(16.0, 32.0, 8.0, 12.0, 4.0), true, true),
        );
    }

    let slim = model_kind == ModelKind::Slim;
    let arm_width = if slim { 3.0 } else { 4.0 };
    let outer_arm_width = if slim { 3.5 } else { 4.5 };
    let arm_x = if slim { 0.5 } else { 1.0 };

    // Right arm (player's right, negative X).
    let right_arm_group = root
        * Mat4::translation(Vec3::new(-5.0, -2.0, 0.0))
        * Mat4::rotation_xyz(model.right_arm_rotation);
    let right_arm_pivot = right_arm_group * Mat4::translation(Vec3::new(-arm_x, -4.0, 0.0));
    push_box(
        &mut scene,
        skin_texture,
        right_arm_pivot,
        Vec3::new(arm_width, 12.0, 4.0),
        &inner_style(skin_uvs(40.0, 16.0, arm_width, 12.0, 4.0), false, false),
    );
    if options.show_outer_layer {
        push_box(
            &mut scene,
            skin_texture,
            right_arm_pivot,
            Vec3::new(outer_arm_width, 12.5, 4.5),
            &inner_style(skin_uvs(40.0, 32.0, arm_width, 12.0, 4.0), true, true),
        );
    }

    // Left arm (player's left, positive X).
    let left_arm_group = root
        * Mat4::translation(Vec3::new(5.0, -2.0, 0.0))
        * Mat4::rotation_xyz(model.left_arm_rotation);
    let left_arm_pivot = left_arm_group * Mat4::translation(Vec3::new(arm_x, -4.0, 0.0));
    push_box(
        &mut scene,
        skin_texture,
        left_arm_pivot,
        Vec3::new(arm_width, 12.0, 4.0),
        &inner_style(skin_uvs(32.0, 48.0, arm_width, 12.0, 4.0), false, false),
    );
    if options.show_outer_layer {
        push_box(
            &mut scene,
            skin_texture,
            left_arm_pivot,
            Vec3::new(outer_arm_width, 12.5, 4.5),
            &inner_style(skin_uvs(48.0, 48.0, arm_width, 12.0, 4.0), true, true),
        );
    }

    // Right leg.
    let right_leg_group = root
        * Mat4::translation(Vec3::new(-1.9, -12.0, -0.1))
        * Mat4::rotation_xyz(model.right_leg_rotation);
    let right_leg_pivot = right_leg_group * Mat4::translation(Vec3::new(0.0, -6.0, 0.0));
    push_box(
        &mut scene,
        skin_texture,
        right_leg_pivot,
        Vec3::new(4.0, 12.0, 4.0),
        &inner_style(skin_uvs(0.0, 16.0, 4.0, 12.0, 4.0), false, false),
    );
    if options.show_outer_layer {
        push_box(
            &mut scene,
            skin_texture,
            right_leg_pivot,
            Vec3::new(4.5, 12.5, 4.5),
            &inner_style(skin_uvs(0.0, 32.0, 4.0, 12.0, 4.0), true, true),
        );
    }

    // Left leg.
    let left_leg_group = root
        * Mat4::translation(Vec3::new(1.9, -12.0, -0.1))
        * Mat4::rotation_xyz(model.left_leg_rotation);
    let left_leg_pivot = left_leg_group * Mat4::translation(Vec3::new(0.0, -6.0, 0.0));
    push_box(
        &mut scene,
        skin_texture,
        left_leg_pivot,
        Vec3::new(4.0, 12.0, 4.0),
        &inner_style(skin_uvs(16.0, 48.0, 4.0, 12.0, 4.0), false, false),
    );
    if options.show_outer_layer {
        push_box(
            &mut scene,
            skin_texture,
            left_leg_pivot,
            Vec3::new(4.5, 12.5, 4.5),
            &inner_style(skin_uvs(0.0, 48.0, 4.0, 12.0, 4.0), true, true),
        );
    }

    if let Some(texture) = back_texture {
        match options.back_equipment {
            BackEquipment::Cape => {
                let cape_group = root
                    * Mat4::translation(Vec3::new(0.0, 0.0, -2.0))
                    * Mat4::rotation_x(model.cape_rotation_x)
                    * Mat4::rotation_y(PI);
                let cape_mesh = cape_group * Mat4::translation(Vec3::new(0.0, -8.0, 0.5));
                push_box(
                    &mut scene,
                    texture,
                    cape_mesh,
                    Vec3::new(10.0, 16.0, 1.0),
                    &inner_style(cape_uvs(0.0, 0.0, 10.0, 16.0, 1.0), true, true),
                );
            }
            BackEquipment::Elytra => {
                let elytra_group = root
                    * Mat4::translation(Vec3::new(0.0, 0.0, -2.0))
                    * Mat4::rotation_x(model.cape_rotation_x);
                let wing_angle_x = 0.261_799_4f32;
                let left_wing = elytra_group
                    * Mat4::translation(Vec3::new(5.0, 0.0, 0.0))
                    * Mat4::rotation_x(wing_angle_x)
                    * Mat4::rotation_y(0.01)
                    * Mat4::rotation_z(wing_angle_x)
                    * Mat4::translation(Vec3::new(-5.0, -10.0, -1.0));
                push_box(
                    &mut scene,
                    texture,
                    left_wing,
                    Vec3::new(12.0, 22.0, 4.0),
                    &inner_style(cape_uvs(22.0, 0.0, 10.0, 20.0, 2.0), true, true),
                );
                let right_wing = elytra_group
                    * Mat4::translation(Vec3::new(-5.0, 0.0, 0.0))
                    * Mat4::rotation_x(wing_angle_x)
                    * Mat4::rotation_y(-0.01)
                    * Mat4::rotation_z(-wing_angle_x)
                    * Mat4::translation(Vec3::new(5.0, -10.0, -1.0))
                    * Mat4::scale(Vec3::new(-1.0, 1.0, 1.0));
                push_box(
                    &mut scene,
                    texture,
                    right_wing,
                    Vec3::new(12.0, 22.0, 4.0),
                    &inner_style(cape_uvs(22.0, 0.0, 10.0, 20.0, 2.0), true, true),
                );
            }
        }
    }

    if let Some(text) = options.tag_text(assets) {
        let tag_texture = crate::name_tag::render_name_tag(text)?;
        let tag_id = scene.add_texture(tag_texture);
        scene.sprites.push(Sprite {
            texture: tag_id,
            center: Vec3::new(0.0, options.name_tag_y, 0.0),
            world_height: options.name_tag_height,
            max_width_ratio: options.name_tag_max_width_ratio,
        });
    }

    if scene.triangles.is_empty() {
        return Err(CoreError::Render("scene has no geometry".to_owned()));
    }
    Ok((scene, model_kind))
}

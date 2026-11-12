use std::fmt;
use std::str::FromStr;

use crate::avatar::AvatarStyle;
use crate::error::{CoreError, Result};
use crate::texture::Texture;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Parts {
    Front,
    Back,
    Both,
    All,
}

impl Parts {
    pub fn wants_front(self) -> bool {
        !matches!(self, Self::Back)
    }

    pub fn wants_back(self) -> bool {
        !matches!(self, Self::Front)
    }

    pub fn image_parts(self) -> &'static str {
        match self {
            Self::Front => "front",
            Self::Back => "back",
            Self::Both | Self::All => "both",
        }
    }
}

impl fmt::Display for Parts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Front => "front",
            Self::Back => "back",
            Self::Both => "both",
            Self::All => "all",
        })
    }
}

impl FromStr for Parts {
    type Err = CoreError;
    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "front" => Ok(Self::Front),
            "back" => Ok(Self::Back),
            "both" => Ok(Self::Both),
            "all" => Ok(Self::All),
            other => Err(CoreError::InvalidRequest(format!(
                "unknown parts {other:?}; expected front, back, both, or all"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ModelOverride {
    #[default]
    AutoDetect,
    Classic,
    Slim,
}

impl fmt::Display for ModelOverride {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::AutoDetect => "auto-detect",
            Self::Classic => "default",
            Self::Slim => "slim",
        })
    }
}

impl FromStr for ModelOverride {
    type Err = CoreError;
    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "auto-detect" | "auto" => Ok(Self::AutoDetect),
            "default" | "classic" => Ok(Self::Classic),
            "slim" => Ok(Self::Slim),
            other => Err(CoreError::InvalidRequest(format!(
                "unknown model {other:?}; expected auto-detect, default, or slim"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackEquipment {
    #[default]
    Cape,
    Elytra,
}

impl fmt::Display for BackEquipment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cape => "cape",
            Self::Elytra => "elytra",
        })
    }
}

impl FromStr for BackEquipment {
    type Err = CoreError;
    fn from_str(value: &str) -> Result<Self> {
        match value.to_ascii_lowercase().as_str() {
            "cape" => Ok(Self::Cape),
            "elytra" => Ok(Self::Elytra),
            other => Err(CoreError::InvalidRequest(format!(
                "unknown back equipment {other:?}; expected cape or elytra"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum View {
    Front,
    Back,
}

#[derive(Clone)]
pub struct SkinAssets {
    pub skin: Texture,
    pub back: Option<Texture>,
    pub display_name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct RenderOptions {
    pub style: String,
    pub parts: Parts,
    pub definition: f32,
    pub transparent: bool,
    /// Render the skin overlay (hat only for avatars), independently of equipment.
    pub show_outer_layer: bool,
    pub show_name: bool,
    pub name_tag: Option<String>,
    pub model: ModelOverride,
    pub back_equipment: BackEquipment,
    pub panel_width: u32,
    pub panel_height: u32,
    pub fov: f32,
    pub zoom: f32,
    pub target_y: f32,
    pub name_tag_height: f32,
    pub name_tag_y: f32,
    pub name_tag_max_width_ratio: f32,
    pub shadow: bool,
    pub supersample: u32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            style: "3d".to_owned(),
            parts: Parts::Both,
            definition: 1.5,
            transparent: false,
            show_outer_layer: true,
            show_name: true,
            name_tag: None,
            model: ModelOverride::AutoDetect,
            back_equipment: BackEquipment::Cape,
            panel_width: 350,
            panel_height: 670,
            fov: 70.0,
            zoom: 1.0,
            target_y: 4.0,
            name_tag_height: 4.0,
            name_tag_y: 22.0,
            name_tag_max_width_ratio: 0.80,
            shadow: true,
            supersample: 2,
        }
    }
}

impl RenderOptions {
    pub fn is_avatar(&self) -> bool {
        self.style == AvatarStyle::ID
    }

    pub fn validate(&self) -> Result<()> {
        if self.is_avatar() {
            return if self.parts == Parts::Front {
                Ok(())
            } else {
                Err(CoreError::InvalidRequest(
                    "avatar style requires front parts".to_owned(),
                ))
            };
        }
        if !(0.25..=5.0).contains(&self.definition) {
            return Err(CoreError::InvalidRequest(
                "definition must be between 0.25 and 5.0".to_owned(),
            ));
        }
        if self.panel_width == 0 || self.panel_height == 0 {
            return Err(CoreError::InvalidRequest(
                "panel dimensions must be positive".to_owned(),
            ));
        }
        if !(1.0..=179.0).contains(&self.fov) {
            return Err(CoreError::InvalidRequest(
                "fov must be between 1 and 179 degrees".to_owned(),
            ));
        }
        if self.zoom <= 0.0 || !self.zoom.is_finite() {
            return Err(CoreError::InvalidRequest(
                "zoom must be positive".to_owned(),
            ));
        }
        if self.supersample == 0 || self.supersample > 4 {
            return Err(CoreError::InvalidRequest(
                "supersample must be between 1 and 4".to_owned(),
            ));
        }
        let output_pixels = u64::from(self.panel_width)
            .saturating_mul(u64::from(self.panel_height))
            .saturating_mul(if self.parts == Parts::Both || self.parts == Parts::All {
                2
            } else {
                1
            });
        if output_pixels > 48_000_000 {
            return Err(CoreError::InvalidRequest(
                "output exceeds the 48 megapixel safety limit".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn panel_output_size(&self) -> (u32, u32) {
        (
            (self.panel_width as f32 * self.definition).round().max(1.0) as u32,
            (self.panel_height as f32 * self.definition)
                .round()
                .max(1.0) as u32,
        )
    }

    pub fn tag_text<'a>(&'a self, assets: &'a SkinAssets) -> Option<&'a str> {
        if !self.show_name {
            return None;
        }
        self.name_tag
            .as_deref()
            .or(assets.display_name.as_deref())
            .filter(|value| !value.is_empty())
    }
}

#[derive(Clone)]
pub struct RenderedImage {
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

#[derive(Default, Clone)]
pub struct RenderInfo {
    pub backend: String,
    pub style: String,
    pub model: String,
    pub parts: String,
    pub panel_width: u32,
    pub panel_height: u32,
    pub output_width: u32,
    pub output_height: u32,
    pub elapsed_ms: f64,
    pub triangles: usize,
    pub cached: bool,
}

#[derive(Default)]
pub struct RenderOutput {
    pub front: Option<RenderedImage>,
    pub back: Option<RenderedImage>,
    pub both: Option<RenderedImage>,
    pub info: RenderInfo,
}

impl RenderOutput {
    pub fn primary(&self) -> Option<&RenderedImage> {
        match self.info.parts.as_str() {
            "front" => self.front.as_ref(),
            "back" => self.back.as_ref(),
            _ => self.both.as_ref(),
        }
    }
}

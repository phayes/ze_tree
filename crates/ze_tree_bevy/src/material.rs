//! Create wind-animated bark and leaf materials, load textures, and style trellis wires.

use bevy::image::{ImageAddressMode, ImageLoaderSettings, ImageSampler, ImageSamplerDescriptor};
use bevy::math::Vec3;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{
    ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline,
};
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, Face, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use ze_tree::{
    BarkMaterial, BarkOptions, BarkUvMode, FlowerMaterial, FruitMaterial, LeafMaterial,
    LeafOptions, TrellisMaterial, TrellisOptions, WindParams,
};

use crate::mesh::ATTRIBUTE_WIND;

/// Vertex shader for drawing wind-animated trees in the main render pass.
pub(crate) const TREE_SHADER: &str = "embedded://ze_tree_bevy/shaders/tree.wesl";
/// Vertex shader that keeps depth, shadows and deferred rendering aligned with wind motion.
pub(crate) const TREE_PREPASS_SHADER: &str = "embedded://ze_tree_bevy/shaders/tree_prepass.wesl";

/// Input slot for [`ATTRIBUTE_WIND`], after Bevy's vertex attribute slots 0–7.
const WIND_LOCATION: u32 = 8;

/// Bevy's [`StandardMaterial`] with wind animation for bark and leaves.
///
/// Use meshes from [`bark_mesh`](crate::bark_mesh) or [`leaf_mesh`](crate::leaf_mesh),
/// or supply [`ATTRIBUTE_WIND`] on your own mesh. This material does not support
/// skinning (bone animation) or morph targets (blending between mesh shapes).
pub type TreeMaterial = ExtendedMaterial<StandardMaterial, WindExtension>;

/// Adds wind animation to a [`TreeMaterial`].
///
/// [`ZeTreePlugin`](crate::ZeTreePlugin) updates every tree material from the
/// global [`Wind`](crate::Wind) resource, including materials you create yourself.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
pub struct WindExtension {
    /// Controls the direction, speed and amount of tree movement.
    #[uniform(100)]
    pub wind: WindUniform,
}

/// Wind settings in the GPU-compatible layout used by the tree shaders.
///
/// Convert from [`WindParams`] with `WindUniform::from(params)`.
#[derive(ShaderType, Reflect, Debug, Clone, Copy, PartialEq)]
pub struct WindUniform {
    /// Direction and amount of movement in tree coordinates; see [`WindParams::strength`].
    pub strength: Vec3,
    /// Animation speed; larger values mean faster movement. See [`WindParams::frequency`].
    pub frequency: f32,
    /// Distance over which wind varies; must be nonzero. See [`WindParams::scale`].
    pub scale: f32,
    /// Trunk movement multiplier; zero keeps the trunk still. See [`WindParams::trunk_bend`].
    pub trunk_bend: f32,
    /// Branch movement multiplier; zero disables branch sway. See [`WindParams::branch_sway`].
    pub branch_sway: f32,
}

impl From<WindParams> for WindUniform {
    fn from(params: WindParams) -> Self {
        Self {
            strength: params.strength,
            frequency: params.frequency,
            scale: params.scale,
            trunk_bend: params.trunk_bend,
            branch_sway: params.branch_sway,
        }
    }
}

impl Default for WindUniform {
    fn default() -> Self {
        WindParams::default().into()
    }
}

impl MaterialExtension for WindExtension {
    fn vertex_shader() -> ShaderRef {
        TREE_SHADER.into()
    }

    fn prepass_vertex_shader() -> ShaderRef {
        TREE_PREPASS_SHADER.into()
    }

    fn deferred_vertex_shader() -> ShaderRef {
        TREE_PREPASS_SHADER.into()
    }

    fn specialize(
        _pipeline: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        layout: &MeshVertexBufferLayoutRef,
        _key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Add the wind input to the vertex attributes selected by Bevy for this render pass.
        let wind = layout
            .0
            .get_layout(&[ATTRIBUTE_WIND.at_shader_location(WIND_LOCATION)])?;
        descriptor.vertex.buffers[0]
            .attributes
            .extend(wind.attributes);
        Ok(())
    }
}

/// The colour, surface detail and roughness images for one bark kind.
#[derive(Debug, Clone, PartialEq)]
pub struct BarkTextures {
    /// Bark colour image, interpreted as sRGB colour.
    pub color: Handle<Image>,
    /// Surface detail normal map, using OpenGL directions and linear colour space.
    pub normal: Handle<Image>,
    /// Roughness map, with the same value in every channel and linear colour space.
    pub roughness: Handle<Image>,
}

impl BarkTextures {
    /// Requests the colour, normal and roughness images for a bark kind.
    ///
    /// Files must be at `<root>/bark/<kind>/{color,normal,roughness}.ktx2`, relative
    /// to Bevy's asset directory. For example, `root` can point to a copy of the
    /// repository's `assets/textures/bc7` folder. Use [`BarkOptions::kind`] for `kind`;
    /// see [`ze_tree::BARK_KINDS`] for available names. Images repeat across the bark
    /// and load in the background; the returned handles do not mean loading is complete.
    pub fn load(asset_server: &AssetServer, root: &str, kind: &str) -> Self {
        let path = |map: &str| format!("{root}/bark/{kind}/{map}.ktx2");
        Self {
            color: load_repeating(asset_server, path("color"), true),
            normal: load_repeating(asset_server, path("normal"), false),
            roughness: load_repeating(asset_server, path("roughness"), false),
        }
    }
}

/// Requests the leaf colour image, including transparency for the leaf outline.
///
/// Loads `<root>/leaves/<kind>.ktx2` as sRGB. The root is relative to Bevy's asset
/// directory, as in [`BarkTextures::load`]. Use [`LeafOptions::kind`] for `kind`;
/// see [`ze_tree::LEAF_KINDS`] for available names. Loading happens in the background.
pub fn load_leaf_texture(asset_server: &AssetServer, root: &str, kind: &str) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| settings.is_srgb = true)
        .load(format!("{root}/leaves/{kind}.ktx2"))
}

/// Requests an image with linear filtering and repetition in both texture directions.
fn load_repeating(asset_server: &AssetServer, path: String, srgb: bool) -> Handle<Image> {
    asset_server
        .load_builder()
        .with_settings(move |settings: &mut ImageLoaderSettings| {
            settings.is_srgb = srgb;
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                ..ImageSamplerDescriptor::linear()
            });
        })
        .load(path)
}

/// Creates a bark material with the requested appearance and initial wind settings.
///
/// Supply textures matching [`BarkOptions::kind`]. They are used only when
/// [`BarkOptions::textured`] is enabled; otherwise the bark uses its tint alone.
/// Normal maps add surface detail only with [`BarkUvMode::Continuous`]. The default
/// mirrored texture coordinates are unsuitable for these normal maps. Pair this
/// material with [`bark_mesh`](crate::bark_mesh) for texture repeat and flat shading.
/// [`ZeTreePlugin`](crate::ZeTreePlugin) replaces `wind` with the global [`Wind`](crate::Wind).
pub fn bark_material(
    bark: &BarkOptions,
    textures: Option<&BarkTextures>,
    wind: WindParams,
) -> TreeMaterial {
    let settings = BarkMaterial::new(bark);
    let [r, g, b] = settings.base_color;
    let mut base = StandardMaterial {
        base_color: Color::linear_rgb(r, g, b),
        metallic: settings.metallic,
        perceptual_roughness: settings.roughness,
        ..default()
    };
    if let Some(textures) = textures.filter(|_| settings.textured) {
        base.base_color_texture = Some(textures.color.clone());
        base.metallic_roughness_texture = Some(textures.roughness.clone());
        if bark.uv_mode == BarkUvMode::Continuous {
            base.normal_map_texture = Some(textures.normal.clone());
        }
    }
    TreeMaterial {
        base,
        extension: WindExtension { wind: wind.into() },
    }
}

/// Creates a leaf material with the requested appearance and initial wind settings.
///
/// Supply a texture matching [`LeafOptions::kind`]; its transparency defines the
/// leaf outline. Without one, leaves appear as solid rectangles. Both sides are
/// visible. [`LeafOptions::rounded_normals`] keeps the lighting directions on back
/// faces unchanged to give the canopy a rounded appearance.
/// [`ZeTreePlugin`](crate::ZeTreePlugin) replaces `wind` with the global [`Wind`](crate::Wind).
pub fn leaf_material(
    leaves: &LeafOptions,
    texture: Option<Handle<Image>>,
    wind: WindParams,
) -> TreeMaterial {
    let settings = LeafMaterial::new(leaves);
    let [r, g, b] = settings.base_color;
    let base = StandardMaterial {
        base_color: Color::linear_rgb(r, g, b),
        base_color_texture: texture,
        alpha_mode: AlphaMode::Mask(settings.alpha_cutoff),
        metallic: settings.metallic,
        perceptual_roughness: settings.roughness,
        cull_mode: (!settings.double_sided).then_some(Face::Back),
        double_sided: settings.double_sided && !settings.rounded_normals,
        ..default()
    };
    TreeMaterial {
        base,
        extension: WindExtension { wind: wind.into() },
    }
}

/// Creates a wind-animated fruit material. Pair it with
/// [`cluster_mesh`](crate::cluster_mesh): the colour comes from the mesh's
/// vertex colours. Both sides of each triangle are visible.
/// [`ZeTreePlugin`](crate::ZeTreePlugin) replaces `wind` with the global [`Wind`](crate::Wind).
pub fn fruit_material(wind: WindParams) -> TreeMaterial {
    let settings = FruitMaterial::default();
    vertex_color_material(settings.metallic, settings.roughness, settings.double_sided, wind)
}

/// Creates a wind-animated flower material. Pair it with
/// [`cluster_mesh`](crate::cluster_mesh): the colour comes from the mesh's
/// vertex colours. Both sides of each triangle are visible.
/// [`ZeTreePlugin`](crate::ZeTreePlugin) replaces `wind` with the global [`Wind`](crate::Wind).
pub fn flower_material(wind: WindParams) -> TreeMaterial {
    let settings = FlowerMaterial::default();
    vertex_color_material(settings.metallic, settings.roughness, settings.double_sided, wind)
}

/// Builds an untextured, white-based material whose colour comes from vertex colours.
fn vertex_color_material(
    metallic: f32,
    roughness: f32,
    double_sided: bool,
    wind: WindParams,
) -> TreeMaterial {
    let base = StandardMaterial {
        base_color: Color::WHITE,
        metallic,
        perceptual_roughness: roughness,
        cull_mode: (!double_sided).then_some(Face::Back),
        double_sided,
        ..default()
    };
    TreeMaterial {
        base,
        extension: WindExtension { wind: wind.into() },
    }
}

/// Creates a material for the support wires of a trellis. Wires remain still in the wind.
pub fn trellis_material(trellis: &TrellisOptions) -> StandardMaterial {
    let settings = TrellisMaterial::new(trellis);
    let [r, g, b] = settings.base_color;
    StandardMaterial {
        base_color: Color::linear_rgb(r, g, b),
        metallic: settings.metallic,
        perceptual_roughness: settings.roughness,
        ..default()
    }
}

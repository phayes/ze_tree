//! Export a tree as glTF 2.0 scene data for a renderer or modelling tool.
//!
//! [`to_glb`] returns a single binary `.glb` file; [`to_gltf`] returns JSON scene
//! text and a separate binary data file. Both include named tree parts and materials
//! that describe colour, roughness, and transparency. Optional joints and vertex
//! weights prepare the tree for animation; no animation clips are generated.
//!
//! Multiple meshes can represent different detail levels for distant viewing, but
//! the receiving tool must choose which to display. The generation settings are
//! saved in `asset.extras.treeOptions` for reuse with this crate.

use std::collections::BTreeMap;
use std::fmt;

use gltf_json as json;
use json::accessor::{ComponentType, GenericComponentType, Type};
use json::buffer::Target;
use json::extras::RawValue;
use json::material::{
    AlphaCutoff, AlphaMode, NormalTexture, PbrBaseColorFactor, PbrMetallicRoughness, StrengthFactor,
};
use json::mesh::{Mode, Semantic};
use json::texture::{MagFilter, MinFilter, WrappingMode};
use json::validation::Checked::Valid;
use json::validation::USize64;
use json::{Index, Root};

use super::{TextureMap, TextureRef, export_parts, scaled_bark_uvs};
use crate::{
    BarkMaterial, BarkUvMode, BranchGrouping, BranchShape, FlowerMaterial, FruitMaterial,
    LeafMaterial, MeshPart, MeshSkin, Rig, TreeMesh, TreeOptions, leaf_has_normal_map,
};

/// Callback returning a texture image URI, or `None` to omit that map.
/// Relative paths are resolved from the `.gltf` or `.glb` file. The caller must
/// make the referenced images available; the exporter does not copy them.
pub type TextureUriFn<'a> = &'a dyn Fn(TextureRef<'_>) -> Option<String>;

/// Callback supplying a complete encoded image to embed, or `None` to omit the map.
pub type TextureImageFn<'a> = &'a dyn Fn(TextureRef<'_>) -> Option<EmbeddedImage>;

/// Selects whether materials use external images, embedded images, or colours alone.
#[derive(Clone, Copy, Default)]
pub enum GltfTextures<'a> {
    /// Uses material colours without texture images (the default).
    #[default]
    None,
    /// References PNG or JPEG images by URI. These images must be accessible
    /// at the supplied locations when the exported scene is loaded.
    Uri(TextureUriFn<'a>),
    /// Includes image bytes in the binary data: inside the `.glb` file or, for
    /// [`to_gltf`], in the separate `.bin` file.
    Embed(TextureImageFn<'a>),
}

impl fmt::Debug for GltfTextures<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::Uri(_) => "Uri(<fn>)",
            Self::Embed(_) => "Embed(<fn>)",
        })
    }
}

/// An encoded image to include in the exported binary data.
///
/// The exporter stores the supplied bytes without decoding or validating them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedImage {
    /// Complete PNG or JPEG file contents, including the file header; not raw pixels.
    pub bytes: Vec<u8>,
    /// Encoding of [`Self::bytes`]. Must match the supplied image contents.
    pub format: ImageFormat,
}

/// Supported encodings for embedded texture images.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    /// PNG, which supports the transparency needed around leaf shapes.
    Png,
    /// JPEG, which has no transparency channel.
    Jpeg,
}

impl ImageFormat {
    /// Returns the media type label written for an embedded image.
    fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
        }
    }
}

/// Controls tree part grouping, animation rig data, textures, and wind attributes.
#[derive(Debug, Clone, Copy)]
pub struct GltfOptions<'a> {
    /// How non-trunk branches are split into meshes.
    /// Defaults to [`BranchGrouping::Merged`], combining them into one mesh.
    pub grouping: BranchGrouping,
    /// Joints to export, together with weights connecting each mesh to them.
    /// Must be built from the same skeleton as every supplied mesh. `None`
    /// (the default) omits the rig; a rig with no joints is also omitted.
    /// This does not create an animation clip.
    pub rig: Option<&'a Rig>,
    /// Texture image source. Defaults to [`GltfTextures::None`] for colours only.
    pub textures: GltfTextures<'a>,
    /// Includes available [`MeshBuffers::wind`](crate::MeshBuffers::wind) values
    /// under the custom vertex attribute `_WIND`. Defaults to `true`.
    /// The receiving renderer needs a compatible shader to animate this data;
    /// standard glTF viewers do not use it automatically.
    pub wind: bool,
}

impl Default for GltfOptions<'_> {
    fn default() -> Self {
        Self {
            grouping: BranchGrouping::default(),
            rig: None,
            textures: GltfTextures::None,
            wind: true,
        }
    }
}

/// The two file contents returned by [`to_gltf`], ready for the caller to save.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GltfFiles {
    /// JSON scene description to save as a `.gltf` file.
    pub json: String,
    /// Binary mesh and embedded image data. Save at the `bin_uri` supplied to
    /// [`to_gltf`], relative to the saved `.gltf` file.
    pub bin: Vec<u8>,
}

/// A failure while creating glTF or GLB file contents.
#[derive(Debug)]
#[non_exhaustive]
pub enum GltfError {
    /// The mesh slice was empty; supply at least one mesh.
    NoMeshes,
    /// The complete GLB file would exceed its maximum size of 4 GiB minus one byte.
    TooLarge,
    /// Serializing the scene or stored tree settings to JSON failed.
    Json(serde_json::Error),
}

impl fmt::Display for GltfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoMeshes => f.write_str("no meshes to export"),
            Self::TooLarge => f.write_str("the exported data is larger than 4 GiB"),
            Self::Json(err) => write!(f, "could not write glTF JSON: {err}"),
        }
    }
}

impl std::error::Error for GltfError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(err) => Some(err),
            _ => None,
        }
    }
}

/// Returns binary `.glb` file contents for the supplied tree meshes.
///
/// Pass at least one mesh, with the full-detail mesh first and simpler versions
/// in decreasing detail order. [`Tree::lod_meshes`](crate::Tree::lod_meshes) can
/// supply these versions. With multiple meshes, part names end in `_LOD0`,
/// `_LOD1`, and so on. These names identify detail levels but do not configure
/// distance-based switching; a viewer may display all versions at once.
///
/// Use the tree's generation settings for `options`. They supply materials and
/// are saved in the file. See [`to_gltf`] for exported attributes and limitations.
/// With embedded textures, the result needs no separate image files.
///
/// # Errors
///
/// Returns [`GltfError::NoMeshes`] for an empty slice, [`GltfError::TooLarge`]
/// if the complete file exceeds 4 GiB minus one byte, or [`GltfError::Json`]
/// if JSON serialization fails.
///
/// # Panics
///
/// May panic for inconsistent mesh buffers or a rig that does not match the
/// meshes' skeleton. See [`TreeMesh::part`] and [`Rig::skin`] for requirements.
pub fn to_glb(
    meshes: &[TreeMesh],
    options: &TreeOptions,
    gltf: &GltfOptions<'_>,
) -> Result<Vec<u8>, GltfError> {
    let (root, bin) = build(meshes, options, gltf, None)?;
    let json = json::serialize::to_vec(&root).map_err(GltfError::Json)?;
    glb(json, bin)
}

/// Returns JSON `.gltf` contents and a separate binary data buffer.
///
/// Supply the data file URI as `bin_uri`, usually a relative name such as
/// `tree.bin`. Save the returned binary data there, relative to the `.gltf` file.
/// Embedded images also go in this data file. Mesh order and generation settings
/// follow the same requirements as [`to_glb`].
///
/// Each part has positions, normals, texture coordinates, and any available
/// tangents. With [`BarkOptions::flat_shading`](crate::BarkOptions::flat_shading),
/// trunk and branch parts have no normals or tangents, so glTF viewers shade each
/// triangle flat and generate any tangents a normal map needs. To include the bark
/// normal map for surface lighting detail, use the default [`BarkUvMode::Continuous`].
/// Wind attributes and joint assignments are optional. Bark texture
/// coordinates already include the texture scale. Bark is opaque; leaves are
/// visible from both sides and hide pixels below the configured alpha threshold.
/// Meshes with [`BranchShape::Ribbons`] use the `bark_two_sided` material
/// for bark, so the flat strips remain visible from either side.
/// The bark roughness image supplies the green channel of glTF's combined
/// metallic-roughness texture; a zero metallic factor keeps bark non-metallic.
///
/// Fruit and flowers are exported as separate `fruit` and `flowers` parts.
/// Their colours are stored at points on the model (vertex colours, `COLOR_0`),
/// so no texture images are needed for them.
///
/// A supplied rig adds joints that can move the bark and leaves in another tool.
/// It includes no animation clips. Fruit and flowers are not attached to these
/// joints and remain in place when the joints move.
///
/// # Errors
///
/// Returns [`GltfError::NoMeshes`] for an empty mesh slice, or [`GltfError::Json`]
/// if JSON serialization fails.
///
/// # Panics
///
/// The same mesh and rig requirements as [`to_glb`] apply.
pub fn to_gltf(
    meshes: &[TreeMesh],
    options: &TreeOptions,
    gltf: &GltfOptions<'_>,
    bin_uri: &str,
) -> Result<GltfFiles, GltfError> {
    let (root, bin) = build(meshes, options, gltf, Some(bin_uri))?;
    let json = json::serialize::to_string_pretty(&root).map_err(GltfError::Json)?;
    Ok(GltfFiles { json, bin })
}

/// Assembles the scene description and binary payload for either output format.
/// Omit `bin_uri` for GLB, whose data is stored inside the same file.
fn build(
    meshes: &[TreeMesh],
    options: &TreeOptions,
    gltf: &GltfOptions<'_>,
    bin_uri: Option<&str>,
) -> Result<(Root, Vec<u8>), GltfError> {
    if meshes.is_empty() {
        return Err(GltfError::NoMeshes);
    }
    let mut b = Builder {
        root: Root::default(),
        bin: Vec::new(),
        textures: gltf.textures,
    };

    // Preserve generation settings alongside the exporter name and version.
    let extras = format!("{{\"treeOptions\":{}}}", options.to_json());
    b.root.asset = json::Asset {
        generator: Some(format!("ze_tree {}", env!("CARGO_PKG_VERSION"))),
        extras: Some(RawValue::from_string(extras).map_err(GltfError::Json)?),
        ..Default::default()
    };
    b.root.buffers.push(json::Buffer {
        byte_length: USize64(0),
        name: None,
        uri: bin_uri.map(str::to_owned),
        extensions: None,
        extras: None,
    });

    let bark = b.bark_material(options);
    // Give ribbon meshes a two-sided copy of the bark material, reusing its textures.
    let has_ribbons = |mesh: &TreeMesh| {
        mesh.branch_spans
            .iter()
            .any(|span| span.shape == BranchShape::Ribbons)
    };
    let two_sided_bark = meshes.iter().any(has_ribbons).then(|| {
        let mut material = b.root.materials[bark.value()].clone();
        material.name = Some("bark_two_sided".to_owned());
        material.double_sided = true;
        b.root.push(material)
    });
    let leaves = b.leaf_material(options);
    // Add fruit and flower materials only if at least one exported mesh needs them.
    let has = |part: MeshPart| {
        meshes
            .iter()
            .any(|mesh| export_parts(gltf.grouping, mesh).contains(&part))
    };
    let fruit = has(MeshPart::Fruit).then(|| {
        let fruit = FruitMaterial::default();
        b.colored_material("fruit", fruit.metallic, fruit.roughness, fruit.double_sided)
    });
    let flowers = has(MeshPart::Flowers).then(|| {
        let flower = FlowerMaterial::default();
        b.colored_material(
            "flowers",
            flower.metallic,
            flower.roughness,
            flower.double_sided,
        )
    });
    let mut scene = Vec::new();

    // Include the joint hierarchy in the scene if the rig contains any joints.
    let rig = gltf.rig.filter(|rig| rig.joint_count() > 0);
    let skin = rig.map(|rig| {
        let (skin, root_joint) = b.skin(rig);
        scene.push(root_joint);
        skin
    });

    // Place mesh nodes directly in the scene without an additional parent transform.
    for (lod, mesh) in meshes.iter().enumerate() {
        let weights = rig.map(|rig| rig.skin(mesh));
        for part in export_parts(gltf.grouping, mesh) {
            let name = if meshes.len() > 1 {
                format!("{}_LOD{lod}", part.name())
            } else {
                part.name()
            };
            let material = match part {
                MeshPart::Leaves => leaves,
                MeshPart::Fruit => fruit.expect("fruit material exists for fruit parts"),
                MeshPart::Flowers => flowers.expect("flower material exists for flower parts"),
                MeshPart::Trunk | MeshPart::Branches | MeshPart::BranchLevel(_) => {
                    match two_sided_bark.filter(|_| has_ribbons(mesh)) {
                        Some(two_sided) => two_sided,
                        None => bark,
                    }
                }
            };
            // Attach only bark and leaves to animation joints; fruit and flowers remain fixed.
            let skinned = part.is_bark() || part.is_leaves();
            let mesh_index = b.part_mesh(
                &name,
                mesh,
                part,
                options,
                weights.as_ref(),
                material,
                gltf.wind,
            );
            scene.push(b.root.push(json::Node {
                name: Some(name),
                mesh: Some(mesh_index),
                skin: skin.filter(|_| skinned),
                ..Default::default()
            }));
        }
    }
    let scene = b.root.push(json::Scene {
        name: Some("tree".to_owned()),
        nodes: scene,
        extensions: None,
        extras: None,
    });
    b.root.scene = Some(scene);

    // Omit the buffer declaration when there is no binary payload.
    pad(&mut b.bin, 0);
    if b.bin.is_empty() {
        b.root.buffers.clear();
    } else {
        b.root.buffers[0].byte_length = USize64(b.bin.len() as u64);
    }
    Ok((b.root, b.bin))
}

/// Packages scene JSON and optional binary data into a GLB file, checking total size.
fn glb(mut json: Vec<u8>, mut bin: Vec<u8>) -> Result<Vec<u8>, GltfError> {
    // Align chunk lengths to four bytes, using spaces for JSON and zeros for binary data.
    pad(&mut json, b' ');
    pad(&mut bin, 0);
    let bin_chunk = if bin.is_empty() { 0 } else { 8 + bin.len() };
    let total = u32::try_from(12 + 8 + json.len() + bin_chunk).map_err(|_| GltfError::TooLarge)?;

    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&(json.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json);
    if !bin.is_empty() {
        out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        out.extend_from_slice(&bin);
    }
    Ok(out)
}

/// Extends `bytes` with `fill` until its length is divisible by four.
fn pad(bytes: &mut Vec<u8>, fill: u8) {
    bytes.resize(bytes.len().next_multiple_of(4), fill);
}

/// Accumulates glTF scene records, binary data, and the caller's texture source.
struct Builder<'a> {
    root: Root,
    bin: Vec<u8>,
    textures: GltfTextures<'a>,
}

impl Builder<'_> {
    /// Appends bytes at a four-byte boundary and records their range as a buffer view.
    /// `target` optionally identifies vertex data or triangle indices.
    fn view(&mut self, bytes: &[u8], target: Option<Target>) -> Index<json::buffer::View> {
        pad(&mut self.bin, 0);
        let offset = self.bin.len();
        self.bin.extend_from_slice(bytes);
        self.root.push(json::buffer::View {
            buffer: Index::new(0),
            byte_length: USize64(bytes.len() as u64),
            byte_offset: Some(USize64(offset as u64)),
            byte_stride: None,
            name: None,
            target: target.map(Valid),
            extensions: None,
            extras: None,
        })
    }

    /// Stores typed data and returns an accessor describing how to read it.
    /// The byte length must match `count`, the component encoding, and `type_` shape.
    fn accessor(
        &mut self,
        bytes: &[u8],
        count: usize,
        component: ComponentType,
        type_: Type,
        target: Option<Target>,
    ) -> Index<json::Accessor> {
        let view = self.view(bytes, target);
        self.root.push(json::Accessor {
            buffer_view: Some(view),
            byte_offset: None,
            count: USize64(count as u64),
            component_type: Valid(GenericComponentType(component)),
            extensions: None,
            extras: None,
            type_: Valid(type_),
            min: None,
            max: None,
            name: None,
            normalized: false,
            sparse: None,
        })
    }

    /// Stores float vectors as little-endian vertex data. `type_` must match `N`.
    fn floats<const N: usize>(&mut self, data: &[[f32; N]], type_: Type) -> Index<json::Accessor> {
        let bytes: Vec<u8> = data
            .iter()
            .flatten()
            .flat_map(|f| f.to_le_bytes())
            .collect();
        self.accessor(
            &bytes,
            data.len(),
            ComponentType::F32,
            type_,
            Some(Target::ArrayBuffer),
        )
    }

    /// Stores nonempty vertex positions and records their minimum and maximum on each axis.
    fn positions(&mut self, data: &[[f32; 3]]) -> Index<json::Accessor> {
        let index = self.floats(data, Type::Vec3);
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in data {
            for axis in 0..3 {
                min[axis] = min[axis].min(p[axis]);
                max[axis] = max[axis].max(p[axis]);
            }
        }
        let accessor = &mut self.root.accessors[index.value()];
        accessor.min = Some(json::Value::from(min.to_vec()));
        accessor.max = Some(json::Value::from(max.to_vec()));
        index
    }

    /// Adds one selected tree part, its material, and optional joint and wind attributes.
    /// Any supplied skin must use the original mesh's vertex order.
    #[allow(clippy::too_many_arguments)]
    fn part_mesh(
        &mut self,
        name: &str,
        mesh: &TreeMesh,
        part: MeshPart,
        options: &TreeOptions,
        skin: Option<&MeshSkin>,
        material: Index<json::Material>,
        wind: bool,
    ) -> Index<json::Mesh> {
        let part_mesh = mesh.part(part);
        let buffers = &part_mesh.buffers;

        // Convert the vertical texture coordinate to glTF's top-left image origin.
        let uvs = if part.is_bark() {
            scaled_bark_uvs(&buffers.uvs, options)
        } else {
            buffers.uvs.clone()
        };
        let uvs: Vec<[f32; 2]> = uvs.into_iter().map(|[u, v]| [u, 1.0 - v]).collect();

        // Include optional attributes only when their buffers are populated.
        // Flat-shaded bark omits normals, which tells glTF viewers to compute
        // per-face normals; glTF ignores tangents without normals, so omit those too.
        let smooth = !part.is_bark() || !options.bark.flat_shading;
        let mut attributes = BTreeMap::new();
        attributes.insert(
            Valid(Semantic::Positions),
            self.positions(&buffers.positions),
        );
        if smooth {
            attributes.insert(
                Valid(Semantic::Normals),
                self.floats(&buffers.normals, Type::Vec3),
            );
        }
        attributes.insert(Valid(Semantic::TexCoords(0)), self.floats(&uvs, Type::Vec2));
        // Tangents need no change for the flipped v: glTF expects
        // cross(normal, tangent) * w to point toward the top of the image, which is
        // the direction of increasing v before the flip.
        if smooth && !buffers.tangents.is_empty() {
            attributes.insert(
                Valid(Semantic::Tangents),
                self.floats(&buffers.tangents, Type::Vec4),
            );
        }
        if wind && !buffers.wind.is_empty() {
            let accessor = self.floats(&buffers.wind, Type::Vec4);
            attributes.insert(Valid(Semantic::Extras("WIND".to_owned())), accessor);
        }
        // Preserve fruit and flower colours on each vertex, including opacity,
        // in the linear colour format required by glTF.
        if !buffers.colors.is_empty() {
            attributes.insert(
                Valid(Semantic::Colors(0)),
                self.floats(&buffers.colors, Type::Vec4),
            );
        }

        // Copy joint assignments using each vertex's position in the original mesh.
        // Only bark and leaves are attached to the rig.
        if let Some(skin) = skin.filter(|_| part.is_bark() || part.is_leaves()) {
            let source = if part.is_leaves() {
                &skin.leaves
            } else {
                &skin.branches
            };
            let vertices = &part_mesh.source_vertices;
            let joints: Vec<u8> = vertices
                .iter()
                .flat_map(|&v| source.joints[v as usize])
                .flat_map(u16::to_le_bytes)
                .collect();
            let joints = self.accessor(
                &joints,
                vertices.len(),
                ComponentType::U16,
                Type::Vec4,
                Some(Target::ArrayBuffer),
            );
            attributes.insert(Valid(Semantic::Joints(0)), joints);
            let weights: Vec<[f32; 4]> = vertices
                .iter()
                .map(|&v| source.weights[v as usize])
                .collect();
            attributes.insert(
                Valid(Semantic::Weights(0)),
                self.floats(&weights, Type::Vec4),
            );
        }

        let indices: Vec<u8> = buffers
            .indices
            .iter()
            .flat_map(|i| i.to_le_bytes())
            .collect();
        let indices = self.accessor(
            &indices,
            buffers.indices.len(),
            ComponentType::U32,
            Type::Scalar,
            Some(Target::ElementArrayBuffer),
        );
        self.root.push(json::Mesh {
            name: Some(name.to_owned()),
            primitives: vec![json::mesh::Primitive {
                attributes,
                indices: Some(indices),
                material: Some(material),
                mode: Valid(Mode::Triangles),
                targets: None,
                extensions: None,
                extras: None,
            }],
            weights: None,
            extensions: None,
            extras: None,
        })
    }

    /// Adds bark colour and surface properties, requesting images only if texturing is enabled.
    fn bark_material(&mut self, options: &TreeOptions) -> Index<json::Material> {
        let bark = BarkMaterial::new(&options.bark);
        let kind = options.bark.kind.as_str();
        let texture = |b: &mut Self, map| {
            if bark.textured {
                b.texture(kind, map, WrappingMode::Repeat)
            } else {
                None
            }
        };
        let color = texture(self, TextureMap::BarkColor);
        let roughness = texture(self, TextureMap::BarkRoughness);
        // Omit the normal map in mirrored mode to avoid reversed lighting detail
        // on every other branch section.
        let normal = if options.bark.uv_mode == BarkUvMode::Continuous {
            texture(self, TextureMap::BarkNormal)
        } else {
            None
        };

        let [r, g, b] = bark.base_color;
        self.root.push(json::Material {
            name: Some("bark".to_owned()),
            pbr_metallic_roughness: PbrMetallicRoughness {
                base_color_factor: PbrBaseColorFactor([r, g, b, 1.0]),
                base_color_texture: color.map(info),
                metallic_factor: StrengthFactor(bark.metallic),
                roughness_factor: StrengthFactor(bark.roughness),
                metallic_roughness_texture: roughness.map(info),
                extensions: None,
                extras: None,
            },
            normal_texture: normal.map(normal_texture),
            ..Default::default()
        })
    }

    /// Creates a material that uses the model's vertex colours (`COLOR_0`)
    /// without requiring a texture image.
    fn colored_material(
        &mut self,
        name: &str,
        metallic: f32,
        roughness: f32,
        double_sided: bool,
    ) -> Index<json::Material> {
        self.root.push(json::Material {
            name: Some(name.to_owned()),
            double_sided,
            pbr_metallic_roughness: PbrMetallicRoughness {
                base_color_factor: PbrBaseColorFactor([1.0; 4]),
                base_color_texture: None,
                metallic_factor: StrengthFactor(metallic),
                roughness_factor: StrengthFactor(roughness),
                metallic_roughness_texture: None,
                extensions: None,
                extras: None,
            },
            ..Default::default()
        })
    }

    /// Adds a leaf material that shows both sides and cuts out transparent pixels
    /// using the configured alpha threshold. Requests colour and, for supported leaf
    /// kinds, a normal map to add lighting detail.
    fn leaf_material(&mut self, options: &TreeOptions) -> Index<json::Material> {
        let leaf = LeafMaterial::new(&options.leaves);
        let kind = options.leaves.kind.as_str();
        let color = self.texture(kind, TextureMap::LeafColor, WrappingMode::ClampToEdge);
        let normal = if leaf_has_normal_map(kind) {
            self.texture(kind, TextureMap::LeafNormal, WrappingMode::ClampToEdge)
        } else {
            None
        };
        let [r, g, b] = leaf.base_color;
        self.root.push(json::Material {
            name: Some("leaves".to_owned()),
            alpha_mode: Valid(AlphaMode::Mask),
            alpha_cutoff: Some(AlphaCutoff(leaf.alpha_cutoff)),
            double_sided: leaf.double_sided,
            pbr_metallic_roughness: PbrMetallicRoughness {
                base_color_factor: PbrBaseColorFactor([r, g, b, 1.0]),
                base_color_texture: color.map(info),
                metallic_factor: StrengthFactor(leaf.metallic),
                roughness_factor: StrengthFactor(leaf.roughness),
                metallic_roughness_texture: None,
                extensions: None,
                extras: None,
            },
            normal_texture: normal.map(normal_texture),
            ..Default::default()
        })
    }

    /// Requests an image for `kind` and `map`, then adds its texture and sampling settings.
    /// Returns `None` when the caller supplies no image for that request.
    fn texture(
        &mut self,
        kind: &str,
        map: TextureMap,
        wrap: WrappingMode,
    ) -> Option<Index<json::Texture>> {
        let reference = TextureRef { kind, map };
        let image = match self.textures {
            GltfTextures::None => return None,
            GltfTextures::Uri(uri) => json::Image {
                uri: Some(uri(reference)?),
                buffer_view: None,
                mime_type: None,
                name: Some(texture_name(reference)),
                extensions: None,
                extras: None,
            },
            GltfTextures::Embed(embed) => {
                let image = embed(reference)?;
                json::Image {
                    uri: None,
                    buffer_view: Some(self.view(&image.bytes, None)),
                    mime_type: Some(json::image::MimeType(image.format.mime_type().to_owned())),
                    name: Some(texture_name(reference)),
                    extensions: None,
                    extras: None,
                }
            }
        };
        let source = self.root.push(image);
        let sampler = self.root.push(json::texture::Sampler {
            mag_filter: Some(Valid(MagFilter::Linear)),
            min_filter: Some(Valid(MinFilter::LinearMipmapLinear)),
            wrap_s: Valid(wrap),
            wrap_t: Valid(wrap),
            name: None,
            extensions: None,
            extras: None,
        });
        Some(self.root.push(json::Texture {
            name: Some(texture_name(reference)),
            sampler: Some(sampler),
            source,
            extensions: None,
            extras: None,
        }))
    }

    /// Adds a nonempty rig's joint hierarchy and the skin connecting it to meshes.
    /// Returns the skin index and the root joint's node index.
    fn skin(&mut self, rig: &Rig) -> (Index<json::Skin>, Index<json::Node>) {
        let first = self.root.nodes.len();
        let node = |i: usize| Index::new((first + i) as u32);
        for joint in rig.joints() {
            self.root.push(json::Node {
                name: Some(format!("joint_{}_{}", joint.branch, joint.section)),
                translation: Some(joint.translation.to_array()),
                rotation: Some(json::scene::UnitQuaternion(joint.rotation.to_array())),
                ..Default::default()
            });
        }
        for (i, joint) in rig.joints().iter().enumerate() {
            if let Some(parent) = joint.parent {
                let children = self.root.nodes[first + parent]
                    .children
                    .get_or_insert_with(Vec::new);
                children.push(node(i));
            }
        }

        // glTF stores the matrices mapping original mesh positions into joint space by columns.
        let matrices: Vec<u8> = rig
            .joints()
            .iter()
            .flat_map(|j| j.inverse_bind.to_cols_array())
            .flat_map(f32::to_le_bytes)
            .collect();
        let inverse_bind = self.accessor(
            &matrices,
            rig.joint_count(),
            ComponentType::F32,
            Type::Mat4,
            None,
        );
        let skin = self.root.push(json::Skin {
            name: Some("tree_rig".to_owned()),
            inverse_bind_matrices: Some(inverse_bind),
            joints: (0..rig.joint_count()).map(node).collect(),
            skeleton: Some(node(0)),
            extensions: None,
            extras: None,
        });
        (skin, node(0))
    }
}

/// Connects a material input to a texture using the first texture coordinate set.
fn info(index: Index<json::Texture>) -> json::texture::Info {
    json::texture::Info {
        index,
        tex_coord: 0,
        extensions: None,
        extras: None,
    }
}

/// Uses a texture as a normal map at full strength, with the mesh's first set
/// of texture coordinates.
fn normal_texture(index: Index<json::Texture>) -> NormalTexture {
    NormalTexture {
        index,
        scale: 1.0,
        tex_coord: 0,
        extensions: None,
        extras: None,
    }
}

/// Names an exported texture by its kind and purpose, for example `oak_leaf`
/// for leaf colour or `white_oak_normal` for leaf lighting detail.
fn texture_name(reference: TextureRef<'_>) -> String {
    let map = match reference.map {
        TextureMap::BarkColor => "color",
        TextureMap::BarkNormal | TextureMap::LeafNormal => "normal",
        TextureMap::BarkRoughness => "roughness",
        TextureMap::LeafColor => "leaf",
    };
    format!("{}_{map}", reference.kind)
}

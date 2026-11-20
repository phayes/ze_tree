//! Generate a tree from settings, a preset, or JSON, and access its shape and meshes.

use std::fmt;

use crate::{LodDetail, LodLevel, OptionsError, Skeleton, TreeMesh, TreeOptions, TreePreset};

/// A generated tree with settings, a branch and leaf layout, and triangles for rendering.
///
/// Create a tree from custom [`TreeOptions`], a built-in [`TreePreset`], or JSON
/// settings. Use [`Tree::mesh`] to access its full-detail geometry, or
/// [`Tree::lod_meshes`] to create simpler versions that cost less to draw at a distance.
/// Your application supplies materials, textures, and rendering.
///
/// To change the settings, clone [`Tree::options`], edit the clone, and pass it
/// to [`Tree::new`] to generate a new tree.
///
/// Generate an ash tree and check that its farthest-distance mesh uses fewer triangles:
///
/// ```
/// use ze_tree::{Tree, TreePreset, default_lod_levels};
///
/// let tree = Tree::from_preset(TreePreset::AshSmall);
/// let lods = tree.lod_meshes(&default_lod_levels());
/// assert!(lods[2].1.triangle_count() < tree.triangle_count());
/// ```
#[derive(Debug, Clone, PartialEq)]
pub struct Tree {
    options: TreeOptions,
    skeleton: Skeleton,
    mesh: TreeMesh,
}

impl Tree {
    /// Generates a tree and its full-detail mesh from the supplied settings.
    ///
    /// # Errors
    ///
    /// Returns an [`OptionsError`] if the branch depth exceeds the supported limit
    /// or a used branch level has zero sections along its length. See
    /// [`TreeOptions::validate`] for the checks performed; other values, such as
    /// dimensions, are not validated.
    pub fn new(options: TreeOptions) -> Result<Self, OptionsError> {
        let skeleton = Skeleton::generate(&options)?;
        let mesh = skeleton.mesh(&options, &LodDetail::default());
        Ok(Self {
            options,
            skeleton,
            mesh,
        })
    }

    /// Generates a tree and its full-detail mesh using a ready-made set of settings.
    pub fn from_preset(preset: TreePreset) -> Self {
        Self::new(preset.options()).expect("built-in presets are valid")
    }

    /// Generates a tree and its full-detail mesh from ez-tree JSON settings.
    ///
    /// Pass a complete settings object or just the fields to change. Omitted
    /// fields, including nested fields, use [`TreeOptions::default`] values.
    /// Unknown fields are ignored, and a non-object JSON value uses all defaults.
    /// See [`TreeOptions::from_json`] for other accepted inputs.
    ///
    /// # Errors
    ///
    /// Returns [`TreeError::Json`] if the JSON cannot be read as settings, or
    /// [`TreeError::Options`] if the resulting settings fail the branch depth or
    /// section count checks in [`TreeOptions::validate`].
    pub fn from_json(json: &str) -> Result<Self, TreeError> {
        let options = TreeOptions::from_json(json).map_err(TreeError::Json)?;
        Self::new(options).map_err(TreeError::Options)
    }

    /// Borrows the settings used to generate this tree.
    ///
    /// Clone them to make changes, then pass the edited settings to [`Tree::new`].
    pub fn options(&self) -> &TreeOptions {
        &self.options
    }

    /// Borrows the tree's skeleton: its branch shapes and leaf positions.
    ///
    /// Use this to inspect the tree's layout; use [`Tree::mesh`] for rendering geometry.
    pub fn skeleton(&self) -> &Skeleton {
        &self.skeleton
    }

    /// Borrows the full-detail mesh, containing bark and leaf triangles for rendering.
    pub fn mesh(&self) -> &TreeMesh {
        &self.mesh
    }

    /// Creates a mesh with the amount of branch and leaf detail specified by `detail`.
    ///
    /// Use [`LodDetail::default`] for full detail, or adjust it to simplify the
    /// geometry for distant trees. The stored skeleton and full-detail mesh remain unchanged.
    pub fn mesh_with_detail(&self, detail: &LodDetail) -> TreeMesh {
        self.skeleton.mesh(&self.options, detail)
    }

    /// Creates meshes for the supplied detail levels, ordered by increasing camera distance.
    ///
    /// Each result pairs a [`LodLevel`] with its mesh. Levels with equal distances
    /// keep their input order; an empty slice returns an empty vector.
    /// Your application decides when to switch meshes using [`LodLevel::distance`].
    /// For ez-tree's standard three levels, pass `&default_lod_levels()` from
    /// [`default_lod_levels`](crate::default_lod_levels).
    pub fn lod_meshes(&self, levels: &[LodLevel]) -> Vec<(LodLevel, TreeMesh)> {
        let mut ordered = levels.to_vec();
        ordered.sort_by(|a, b| a.distance.total_cmp(&b.distance));
        ordered
            .into_iter()
            .map(|level| {
                let mesh = self.mesh_with_detail(&level.detail);
                (level, mesh)
            })
            .collect()
    }

    /// Returns the total number of vertices (points) in the full-detail bark and leaf meshes.
    ///
    /// Duplicate points are counted separately.
    pub fn vertex_count(&self) -> usize {
        self.mesh.vertex_count()
    }

    /// Returns the total number of triangles in the full-detail bark and leaf meshes.
    pub fn triangle_count(&self) -> usize {
        self.mesh.triangle_count()
    }

    /// Consumes the tree and returns its settings, branch and leaf layout, and
    /// full-detail mesh, in that order, transferring ownership to the caller.
    pub fn into_parts(self) -> (TreeOptions, Skeleton, TreeMesh) {
        (self.options, self.skeleton, self.mesh)
    }
}

/// An error returned by [`Tree::from_json`] when reading or validating settings.
#[derive(Debug)]
#[non_exhaustive]
pub enum TreeError {
    /// The JSON could not be read as tree settings; the enclosed error describes why.
    Json(serde_json::Error),
    /// The settings were read, but their branch depth or section counts are unsupported.
    Options(OptionsError),
}

impl fmt::Display for TreeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(err) => write!(f, "invalid tree options JSON: {err}"),
            Self::Options(err) => write!(f, "unsupported tree options: {err}"),
        }
    }
}

impl std::error::Error for TreeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(err) => Some(err),
            Self::Options(err) => Some(err),
        }
    }
}

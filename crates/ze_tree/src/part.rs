//! Extract the trunk, branches, leaves, fruit, or flowers from a [`TreeMesh`] for
//! separate drawing or export.

use std::ops::Range;

use crate::{MeshBuffers, TreeMesh};

/// Selects which bark, leaves, fruit, or flowers to copy from a [`TreeMesh`].
///
/// Pass this to [`TreeMesh::part`] to get a separate mesh. To list the nonempty
/// bark and leaf parts first, use [`BranchGrouping::parts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshPart {
    /// Bark on the trunk, also called branch level 0.
    Trunk,
    /// Bark on all branches growing from the trunk, including their descendants.
    Branches,
    /// Bark at the given branch depth. Level 1 grows directly from the trunk,
    /// level 2 grows from level 1, and so on. Level 0 selects the trunk.
    BranchLevel(u32),
    /// Every leaf in the mesh.
    Leaves,
    /// All fruit and their supporting stalks.
    Fruit,
    /// All flowers and their supporting stalks.
    Flowers,
}

impl MeshPart {
    /// Returns a name suitable for an exported object: `trunk`, `branches`,
    /// `branches_<level>`, `leaves`, `fruit`, or `flowers`.
    pub fn name(self) -> String {
        match self {
            Self::Trunk => "trunk".to_owned(),
            Self::Branches => "branches".to_owned(),
            Self::BranchLevel(level) => format!("branches_{level}"),
            Self::Leaves => "leaves".to_owned(),
            Self::Fruit => "fruit".to_owned(),
            Self::Flowers => "flowers".to_owned(),
        }
    }

    /// Returns whether this part contains leaves.
    pub fn is_leaves(self) -> bool {
        self == Self::Leaves
    }

    /// Returns whether this part selects trunk or branch bark.
    pub fn is_bark(self) -> bool {
        matches!(self, Self::Trunk | Self::Branches | Self::BranchLevel(_))
    }

    /// Checks whether this selection includes bark at the given branch depth.
    fn includes_level(self, level: u32) -> bool {
        match self {
            Self::Trunk => level == 0,
            Self::Branches => level > 0,
            Self::BranchLevel(l) => level == l,
            Self::Leaves | Self::Fruit | Self::Flowers => false,
        }
    }
}

/// Controls whether branches are combined or separated by depth.
///
/// The trunk and leaves remain separate parts with either choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BranchGrouping {
    /// Combines all branches except the trunk into one `branches` part (the default).
    #[default]
    Merged,
    /// Separates branches by depth into `branches_1`, `branches_2`, and so on.
    PerLevel,
}

impl BranchGrouping {
    /// Lists nonempty bark and leaf parts in `mesh`, with the trunk first and
    /// leaves last. [`Self::PerLevel`] lists branches in increasing depth order.
    ///
    /// Fruit and flowers are omitted. Request them separately with
    /// [`TreeMesh::part`] and [`MeshPart::Fruit`] or [`MeshPart::Flowers`].
    pub fn parts(self, mesh: &TreeMesh) -> Vec<MeshPart> {
        // Ignore levels whose branches have no triangles.
        let has_triangles = |level: u32| {
            mesh.branch_spans
                .iter()
                .any(|span| span.level == level && !span.indices.is_empty())
        };
        let deepest = mesh.branch_spans.iter().map(|s| s.level).max().unwrap_or(0);

        let mut parts = Vec::new();
        if has_triangles(0) {
            parts.push(MeshPart::Trunk);
        }
        match self {
            Self::Merged => {
                if (1..=deepest).any(has_triangles) {
                    parts.push(MeshPart::Branches);
                }
            }
            Self::PerLevel => {
                parts.extend(
                    (1..=deepest)
                        .filter(|&l| has_triangles(l))
                        .map(MeshPart::BranchLevel),
                );
            }
        }
        if !mesh.leaves.indices.is_empty() {
            parts.push(MeshPart::Leaves);
        }
        parts
    }
}

/// An extracted mesh and a mapping back to its original vertices.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PartMesh {
    /// Vertex data and triangles for this part. Triangle indices start at zero
    /// and refer to vertices in these buffers.
    pub buffers: MeshBuffers,
    /// Maps each vertex in [`Self::buffers`] to its index in the original mesh.
    ///
    /// For leaves, fruit, and flowers, indices refer to the corresponding
    /// [`TreeMesh::leaves`], [`TreeMesh::fruit`], or [`TreeMesh::flowers`] buffer.
    /// For bark, they refer to [`TreeMesh::branches`]. Use this mapping to copy
    /// additional vertex data, such as joint weights from [`Rig::skin`](crate::Rig::skin).
    pub source_vertices: Vec<u32>,
}

impl TreeMesh {
    /// Copies the selected part into independent buffers, with triangle indices
    /// that refer to the copied vertices.
    ///
    /// Bark follows the order in [`TreeMesh::branch_spans`]; an absent branch
    /// level returns an empty part. Leaves, fruit, and flowers each copy their
    /// entire buffer, including an empty buffer when none are present.
    ///
    /// # Panics
    ///
    /// May panic if manually edited spans or buffers contain invalid ranges or
    /// indices. Keep them consistent, as produced by [`Skeleton::mesh`](crate::Skeleton::mesh).
    pub fn part(&self, part: MeshPart) -> PartMesh {
        // Copying an entire buffer preserves its vertex indices.
        let whole = match part {
            MeshPart::Leaves => Some(&self.leaves),
            MeshPart::Fruit => Some(&self.fruit),
            MeshPart::Flowers => Some(&self.flowers),
            MeshPart::Trunk | MeshPart::Branches | MeshPart::BranchLevel(_) => None,
        };
        if let Some(buffers) = whole {
            return PartMesh {
                buffers: buffers.clone(),
                source_vertices: (0..buffers.next_index()).collect(),
            };
        }

        // Append selected branches in the order recorded by the mesh.
        let mut out = PartMesh::default();
        for span in &self.branch_spans {
            if part.includes_level(span.level) {
                out.append(&self.branches, span.vertices.clone(), span.indices.clone());
            }
        }
        out
    }

    /// Copies the selected part’s crossed fallback shapes from [`TreeMesh::billboard_cull`].
    /// Use these when drawing fixed shapes; skip them when rotating the main billboards
    /// toward the camera.
    ///
    /// Bark selections copy their fallback strips. [`MeshPart::Leaves`] and
    /// [`MeshPart::Fruit`] copy the corresponding [`BillboardCull`](crate::BillboardCull)
    /// buffers. [`MeshPart::Flowers`] and selections without crossed shapes return
    /// an empty part. [`PartMesh::source_vertices`] indexes the matching fallback
    /// buffer, not the main mesh.
    ///
    /// # Panics
    ///
    /// May panic for invalid manually edited spans or indices, as with [`TreeMesh::part`].
    pub fn billboard_cull_part(&self, part: MeshPart) -> PartMesh {
        let cull = &self.billboard_cull;
        let whole = match part {
            MeshPart::Leaves => Some(&cull.leaves),
            MeshPart::Fruit => Some(&cull.fruit),
            MeshPart::Flowers => return PartMesh::default(),
            MeshPart::Trunk | MeshPart::Branches | MeshPart::BranchLevel(_) => None,
        };
        if let Some(buffers) = whole {
            return PartMesh {
                buffers: buffers.clone(),
                source_vertices: (0..buffers.next_index()).collect(),
            };
        }

        let mut out = PartMesh::default();
        for span in &self.branch_spans {
            if part.includes_level(span.level) {
                let (vertices, indices) = (span.cull_vertices.clone(), span.cull_indices.clone());
                out.append(&cull.branches, vertices, indices);
            }
        }
        out
    }
}

impl PartMesh {
    /// Copies the given vertex and triangle ranges from `source` into this part.
    /// Ranges must be in bounds, and all copied indices must refer to `vertices`.
    fn append(&mut self, source: &MeshBuffers, vertices: Range<u32>, indices: Range<u32>) {
        let out = &mut self.buffers;
        let base = out.next_index();
        let v = vertices.start as usize..vertices.end as usize;

        // Copy each optional vertex attribute only when the source contains it.
        out.positions
            .extend_from_slice(&source.positions[v.clone()]);
        out.uvs.extend_from_slice(&source.uvs[v.clone()]);
        if !source.normals.is_empty() {
            out.normals.extend_from_slice(&source.normals[v.clone()]);
        }
        if !source.tangents.is_empty() {
            out.tangents.extend_from_slice(&source.tangents[v.clone()]);
        }
        if !source.wind.is_empty() {
            out.wind.extend_from_slice(&source.wind[v.clone()]);
        }
        if !source.billboard.is_empty() {
            out.billboard.extend_from_slice(&source.billboard[v]);
        }
        self.source_vertices.extend(vertices.clone());

        // Adjust triangle indices to the copied vertices' new positions.
        let i = indices.start as usize..indices.end as usize;
        out.indices.extend(
            source.indices[i]
                .iter()
                .map(|&index| index - vertices.start + base),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LodDetail, Skeleton, TreeOptions, TreePreset};

    fn mesh() -> TreeMesh {
        let options = TreePreset::OakMedium.options();
        Skeleton::generate(&options)
            .unwrap()
            .mesh(&options, &LodDetail::default())
    }

    #[test]
    fn part_names() {
        assert_eq!(MeshPart::Trunk.name(), "trunk");
        assert_eq!(MeshPart::Branches.name(), "branches");
        assert_eq!(MeshPart::BranchLevel(2).name(), "branches_2");
        assert_eq!(MeshPart::Leaves.name(), "leaves");
    }

    #[test]
    fn parts_cover_the_mesh_exactly_once() {
        let mesh = mesh();
        for grouping in [BranchGrouping::Merged, BranchGrouping::PerLevel] {
            let parts = grouping.parts(&mesh);
            let bark: Vec<PartMesh> = parts
                .iter()
                .filter(|p| !p.is_leaves())
                .map(|&p| mesh.part(p))
                .collect();

            // Splitting must neither lose nor duplicate bark vertices or triangles.
            let mut sources: Vec<u32> = bark
                .iter()
                .flat_map(|p| p.source_vertices.clone())
                .collect();
            sources.sort_unstable();
            assert_eq!(
                sources,
                (0..mesh.branches.positions.len() as u32).collect::<Vec<_>>()
            );
            let triangles: usize = bark.iter().map(|p| p.buffers.triangle_count()).sum();
            assert_eq!(triangles, mesh.branches.triangle_count());

            // Use the source mapping to check each copied position and texture coordinate.
            for part in &bark {
                let b = &part.buffers;
                assert_eq!(b.tangents.len(), b.positions.len());
                assert_eq!(b.wind.len(), b.positions.len());
                for &i in &b.indices {
                    let src = part.source_vertices[i as usize] as usize;
                    assert_eq!(b.positions[i as usize], mesh.branches.positions[src]);
                    assert_eq!(b.uvs[i as usize], mesh.branches.uvs[src]);
                }
            }
            assert_eq!(parts.last(), Some(&MeshPart::Leaves));
        }

        // This oak preset has three branch depths beyond the trunk.
        assert_eq!(
            BranchGrouping::PerLevel.parts(&mesh),
            [
                MeshPart::Trunk,
                MeshPart::BranchLevel(1),
                MeshPart::BranchLevel(2),
                MeshPart::BranchLevel(3),
                MeshPart::Leaves
            ]
        );
    }

    #[test]
    fn re_based_indices_keep_triangles() {
        let mesh = mesh();
        let part = mesh.part(MeshPart::BranchLevel(2));
        let first = mesh.branch_spans.iter().find(|s| s.level == 2).unwrap();
        // Mapping the first triangle back must recover the first selected branch's triangle.
        let src_tri = &mesh.branches.indices[first.indices.start as usize..][..3];
        let part_tri: Vec<u32> = part.buffers.indices[..3]
            .iter()
            .map(|&i| part.source_vertices[i as usize])
            .collect();
        assert_eq!(part_tri, src_tri);
        assert_eq!(part.source_vertices[0], first.vertices.start);
    }

    #[test]
    fn empty_parts_are_skipped() {
        let mut options = TreeOptions::default();
        options.branch.levels = 0;
        options.leaves.count = 1;
        let mesh = Skeleton::generate(&options)
            .unwrap()
            .mesh(&options, &LodDetail::default());
        // A trunk-only tree has leaves but no child branches.
        assert_eq!(
            BranchGrouping::Merged.parts(&mesh),
            [MeshPart::Trunk, MeshPart::Leaves]
        );
        assert!(mesh.part(MeshPart::Branches).buffers.positions.is_empty());

        // With no leaves, only the trunk remains.
        options.leaves.count = 0;
        let bare = Skeleton::generate(&options)
            .unwrap()
            .mesh(&options, &LodDetail::default());
        assert_eq!(BranchGrouping::Merged.parts(&bare), [MeshPart::Trunk]);
    }
}

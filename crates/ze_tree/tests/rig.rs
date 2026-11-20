//! Check joint limits, parent relationships, and vertex movement for generated trees.
//! The rest pose (before animation) must reproduce the original mesh.

use glam::{Mat4, Quat, Vec3};
use ze_tree::{
    MAX_RIG_JOINTS, MeshSkin, Rig, RigError, RigOptions, SkinWeights, Tree, TreeMesh, TreePreset,
    default_lod_levels,
};

/// Computes each joint's transform in tree coordinates, including its ancestors.
/// When supplied, `pose` applies an extra local rotation to one joint.
fn world_transforms(rig: &Rig, pose: Option<(usize, Quat)>) -> Vec<Mat4> {
    let mut world: Vec<Mat4> = Vec::with_capacity(rig.joint_count());
    for (i, joint) in rig.joints().iter().enumerate() {
        let mut rotation = joint.rotation;
        if let Some((posed, extra)) = pose
            && posed == i
        {
            rotation *= extra;
        }
        let local = Mat4::from_rotation_translation(rotation, joint.translation);
        let parent = joint.parent.map_or(Mat4::IDENTITY, |p| world[p]);
        world.push(parent * local);
    }
    world
}

/// Moves each vertex by its assigned joints and blends the results using its weights.
fn skinned(positions: &[[f32; 3]], skin: &SkinWeights, rig: &Rig, world: &[Mat4]) -> Vec<Vec3> {
    let matrices: Vec<Mat4> = world
        .iter()
        .zip(rig.joints())
        .map(|(w, j)| *w * j.inverse_bind)
        .collect();
    positions
        .iter()
        .zip(skin.joints.iter().zip(&skin.weights))
        .map(|(&p, (joints, weights))| {
            let p = Vec3::from_array(p);
            joints
                .iter()
                .zip(weights)
                .map(|(&j, &w)| matrices[j as usize].transform_point3(p) * w)
                .sum()
        })
        .collect()
}

fn check_weights(skin: &SkinWeights, vertices: usize, joints: usize) {
    assert_eq!(skin.joints.len(), vertices);
    assert_eq!(skin.weights.len(), vertices);
    for (js, ws) in skin.joints.iter().zip(&skin.weights) {
        let sum: f32 = ws.iter().sum();
        assert!((sum - 1.0).abs() < 1e-6, "weights sum to {sum}");
        for (&j, &w) in js.iter().zip(ws) {
            assert!(w >= 0.0);
            assert!((j as usize) < joints, "joint {j} of {joints}");
        }
    }
}

fn assert_close(a: &[Vec3], b: &[[f32; 3]]) {
    assert_eq!(a.len(), b.len());
    for (a, &b) in a.iter().zip(b) {
        assert!(a.abs_diff_eq(Vec3::from_array(b), 1e-3), "{a} != {b:?}");
    }
}

fn check_rest_pose(rig: &Rig, mesh: &TreeMesh, skin: &MeshSkin) {
    let world = world_transforms(rig, None);
    let bark = skinned(&mesh.branches.positions, &skin.branches, rig, &world);
    assert_close(&bark, &mesh.branches.positions);
    let leaves = skinned(&mesh.leaves.positions, &skin.leaves, rig, &world);
    assert_close(&leaves, &mesh.leaves.positions);
}

#[test]
fn every_preset_rigs_within_budget_and_holds_its_rest_pose() {
    for preset in TreePreset::ALL {
        let tree = Tree::from_preset(preset);
        let rig = tree.rig(&RigOptions::default()).unwrap();
        let name = preset.name();
        // Every preset has enough branches to use the whole default limit.
        assert_eq!(rig.joint_count(), 64, "{name}");
        assert!(!rig.level_strides().is_empty(), "{name}");

        // The trunk base is the only root, and each parent precedes its children.
        let joints = rig.joints();
        assert_eq!(joints[0].parent, None, "{name}");
        assert_eq!((joints[0].branch, joints[0].section), (0, 0));
        for (i, joint) in joints.iter().enumerate().skip(1) {
            let parent = joint
                .parent
                .unwrap_or_else(|| panic!("{name}: joint {i} has no parent"));
            assert!(parent < i, "{name}: joint {i} parent {parent}");
        }

        let skin = rig.skin(tree.mesh());
        check_weights(
            &skin.branches,
            tree.mesh().branches.positions.len(),
            joints.len(),
        );
        check_weights(
            &skin.leaves,
            tree.mesh().leaves.positions.len(),
            joints.len(),
        );
        check_rest_pose(&rig, tree.mesh(), &skin);
    }
}

#[test]
fn simpler_meshes_share_the_rig() {
    let tree = Tree::from_preset(TreePreset::OakLarge);
    let rig = tree
        .rig(&RigOptions {
            max_joints: MAX_RIG_JOINTS,
            ..RigOptions::default()
        })
        .unwrap();
    assert!(rig.joint_count() > 64);
    for (_, mesh) in tree.lod_meshes(&default_lod_levels()) {
        let skin = rig.skin(&mesh);
        check_weights(
            &skin.branches,
            mesh.branches.positions.len(),
            rig.joint_count(),
        );
        check_rest_pose(&rig, &mesh, &skin);
    }
}

#[test]
fn joint_limit_and_stride_are_checked() {
    let tree = Tree::from_preset(TreePreset::AshMedium);
    for max_joints in [0, MAX_RIG_JOINTS + 1] {
        let options = RigOptions {
            max_joints,
            ..RigOptions::default()
        };
        assert_eq!(
            tree.rig(&options),
            Err(RigError::InvalidJointLimit { max_joints })
        );
    }

    // One joint per non-tip section exceeds this preset's default budget.
    let every_section = RigOptions {
        section_stride: Some(1),
        ..RigOptions::default()
    };
    assert!(matches!(
        tree.rig(&every_section),
        Err(RigError::TooManyJoints { max_joints: 64, .. })
    ));

    // Limiting joints to the trunk allows spacing of one section within the budget.
    let trunk = RigOptions {
        max_level: 0,
        ..every_section
    };
    let rig = tree.rig(&trunk).unwrap();
    let sections = tree.skeleton().branches[0].sections.len();
    assert_eq!(rig.joint_count(), sections - 1);
    assert_eq!(rig.level_strides(), [1]);
    assert!(rig.joints().iter().all(|j| j.branch == 0));
}

#[test]
fn leftover_joints_rig_the_largest_pine_limbs() {
    let tree = Tree::from_preset(TreePreset::PineMedium);
    let rig = tree.rig(&RigOptions::default()).unwrap();
    let branches = &tree.skeleton().branches;
    let limbs: Vec<usize> = (0..branches.len())
        .filter(|&b| branches[b].level == 1)
        .collect();

    // There are too many limbs to cover all of them. Fully detail the trunk,
    // then assign one base joint to each selected limb until the budget is exhausted.
    assert_eq!(rig.level_strides(), [1]);
    assert_eq!(rig.joint_count(), 64);
    let rigged: Vec<usize> = limbs
        .iter()
        .copied()
        .filter(|&b| rig.is_rigged(b))
        .collect();
    assert_eq!(rigged.len(), 64 - (branches[0].sections.len() - 1));

    // Size priority must not leave a larger limb without a joint while covering a smaller one.
    let size = |b: usize| {
        let s = &branches[b].sections;
        let length: f32 = s
            .windows(2)
            .map(|w| w[0].origin.distance(w[1].origin))
            .sum();
        length * branches[b].base_radius
    };
    let smallest_rigged = rigged
        .iter()
        .map(|&b| size(b))
        .fold(f32::INFINITY, f32::min);
    let largest_unrigged = limbs
        .iter()
        .filter(|&&b| !rig.is_rigged(b))
        .map(|&b| size(b))
        .fold(0.0, f32::max);
    assert!(smallest_rigged >= largest_unrigged);

    // Partial branch coverage must also reproduce the original mesh before animation.
    check_rest_pose(&rig, tree.mesh(), &rig.skin(tree.mesh()));
}

#[test]
fn leaves_move_rigidly_with_their_branch() {
    let tree = Tree::from_preset(TreePreset::AspenMedium);
    let rig = tree.rig(&RigOptions::default()).unwrap();
    let skin = rig.skin(tree.mesh());

    // Check equal assignments across each leaf; this does not test shape under blended rotations.
    for span in &tree.mesh().leaf_spans {
        let range = span.vertices.start as usize..span.vertices.end as usize;
        let first = range.start;
        for v in range {
            assert_eq!(skin.leaves.joints[v], skin.leaves.joints[first]);
            assert_eq!(skin.leaves.weights[v], skin.leaves.weights[first]);
        }
    }
}

#[test]
fn rotating_a_joint_moves_what_grows_beyond_it() {
    let tree = Tree::from_preset(TreePreset::OakMedium);
    let mesh = tree.mesh();
    let rig = tree.rig(&RigOptions::default()).unwrap();
    let skin = rig.skin(mesh);

    // Rotating the root should rotate every leaf around the trunk base.
    let turn = Quat::from_rotation_z(0.5);
    let world = world_transforms(&rig, Some((0, turn)));
    let root = rig.joints()[0].translation;
    let expected: Vec<[f32; 3]> = mesh
        .leaves
        .positions
        .iter()
        .map(|&p| (turn * (Vec3::from_array(p) - root) + root).to_array())
        .collect();
    assert_close(
        &skinned(&mesh.leaves.positions, &skin.leaves, &rig, &world),
        &expected,
    );

    // A joint at a main branch's base should move that branch while leaving the trunk fixed.
    let limb = rig
        .joints()
        .iter()
        .position(|j| j.branch != 0 && j.section == 0)
        .expect("a level 1 joint");
    let world = world_transforms(&rig, Some((limb, turn)));
    let bark = skinned(&mesh.branches.positions, &skin.branches, &rig, &world);
    let span = |branch: usize| {
        let s = &mesh.branch_spans[branch];
        s.vertices.start as usize..s.vertices.end as usize
    };
    let trunk = span(0);
    assert_close(&bark[trunk.clone()], &mesh.branches.positions[trunk]);
    let limb_vertices = span(rig.joints()[limb].branch);
    let tip = limb_vertices.end - 1;
    let moved = bark[tip].distance(Vec3::from_array(mesh.branches.positions[tip]));
    assert!(moved > 0.1, "limb tip moved {moved}");
}

/// Bush 2 covers every branch level with joints to spare; the spare joints give
/// its largest branches closer spacing, so the rig fills the limit.
#[test]
fn spare_joints_refine_the_largest_branches() {
    let tree = Tree::from_preset(TreePreset::Bush2);
    let rig = tree.rig(&RigOptions::default()).unwrap();
    let branches = &tree.skeleton().branches;
    assert_eq!(rig.joint_count(), 64);

    // Every branch is rigged, and each reported stride matches its joints.
    // Count branches with more joints than their level's spacing gives.
    let mut refined = 0;
    for (b, branch) in branches.iter().enumerate() {
        let stride = rig.branch_stride(b).unwrap_or_else(|| panic!("branch {b}"));
        let level_stride = rig.level_strides()[branch.level as usize];
        assert!(
            stride <= level_stride,
            "branch {b}: {stride} > {level_stride}"
        );
        let segments = branch.sections.len() as u32 - 1;
        let joints = rig.joints().iter().filter(|j| j.branch == b).count() as u32;
        assert_eq!(joints, segments.div_ceil(stride), "branch {b}");
        if joints > segments.div_ceil(level_stride) {
            refined += 1;
        }
    }
    assert!(refined > 0);

    check_rest_pose(&rig, tree.mesh(), &rig.skin(tree.mesh()));
}

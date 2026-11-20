//! Control wind across all tree materials with one resource.

use bevy::prelude::*;
use ze_tree::WindParams;

use crate::material::{TreeMaterial, WindUniform};

/// Global wind settings for every [`TreeMaterial`], including custom materials.
///
/// Change this resource to update the wind during the plugin's next wind sync.
/// The plugin overwrites individual materials' wind settings with these values.
/// Defaults to [`WindParams::default`], which moves trunks, branches and leaves.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default, Deref, DerefMut)]
pub struct Wind(pub WindParams);

/// Updates materials to match the global wind, leaving matching materials untouched.
pub(crate) fn sync_wind(wind: Res<Wind>, mut materials: ResMut<Assets<TreeMaterial>>) {
    let uniform = WindUniform::from(wind.0);
    let stale: Vec<AssetId<TreeMaterial>> = materials
        .iter()
        .filter(|(_, material)| material.extension.wind != uniform)
        .map(|(id, _)| id)
        .collect();
    for id in stale {
        if let Some(mut material) = materials.get_mut(id) {
            material.extension.wind = uniform;
        }
    }
}

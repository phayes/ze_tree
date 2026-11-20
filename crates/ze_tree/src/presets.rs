//! Ready-made tree settings from ez-tree, covering several species and sizes.
//!
//! Use [`TreePreset::options`] to get settings that can be customised, for example
//! `TreePreset::OakSmall.options()`. Missing settings in the embedded preset JSON
//! are filled from [`TreeOptions::default()`].

use crate::options::TreeOptions;

/// Built-in starting points for tree settings, including several sizes and species.
///
/// Use [`options`](Self::options) to get editable settings, for example
/// `TreePreset::OakSmall.options()`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreePreset {
    AshSmall,
    AshMedium,
    AshLarge,
    AspenSmall,
    AspenMedium,
    AspenLarge,
    Bush1,
    Bush2,
    Bush3,
    OakSmall,
    OakMedium,
    OakLarge,
    PineSmall,
    PineMedium,
    PineLarge,
    /// A broadleaf tree configured to grow along a grid of guide wires.
    Trellis,
}

impl TreePreset {
    /// All 16 presets in ez-tree display order, suitable for a selection menu.
    pub const ALL: [TreePreset; 16] = [
        Self::AshSmall,
        Self::AshMedium,
        Self::AshLarge,
        Self::AspenSmall,
        Self::AspenMedium,
        Self::AspenLarge,
        Self::Bush1,
        Self::Bush2,
        Self::Bush3,
        Self::OakSmall,
        Self::OakMedium,
        Self::OakLarge,
        Self::PineSmall,
        Self::PineMedium,
        Self::PineLarge,
        Self::Trellis,
    ];

    /// Returns the ez-tree display name, such as `"Ash Small"`.
    pub fn name(self) -> &'static str {
        match self {
            Self::AshSmall => "Ash Small",
            Self::AshMedium => "Ash Medium",
            Self::AshLarge => "Ash Large",
            Self::AspenSmall => "Aspen Small",
            Self::AspenMedium => "Aspen Medium",
            Self::AspenLarge => "Aspen Large",
            Self::Bush1 => "Bush 1",
            Self::Bush2 => "Bush 2",
            Self::Bush3 => "Bush 3",
            Self::OakSmall => "Oak Small",
            Self::OakMedium => "Oak Medium",
            Self::OakLarge => "Oak Large",
            Self::PineSmall => "Pine Small",
            Self::PineMedium => "Pine Medium",
            Self::PineLarge => "Pine Large",
            Self::Trellis => "Trellis",
        }
    }

    /// Looks up an exact display name from [`name`](Self::name).
    ///
    /// Matching is case-sensitive and includes spaces: `"Ash Small"` matches,
    /// while `"ash_small"` does not. Returns `None` for an unrecognised name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.name() == name)
    }

    /// Returns the embedded ez-tree preset JSON. It may omit settings; use
    /// [`TreeOptions::from_json`] or [`options`](Self::options) to fill in defaults.
    pub fn json(self) -> &'static str {
        match self {
            Self::AshSmall => include_str!("../presets/ash_small.json"),
            Self::AshMedium => include_str!("../presets/ash_medium.json"),
            Self::AshLarge => include_str!("../presets/ash_large.json"),
            Self::AspenSmall => include_str!("../presets/aspen_small.json"),
            Self::AspenMedium => include_str!("../presets/aspen_medium.json"),
            Self::AspenLarge => include_str!("../presets/aspen_large.json"),
            Self::Bush1 => include_str!("../presets/bush_1.json"),
            Self::Bush2 => include_str!("../presets/bush_2.json"),
            Self::Bush3 => include_str!("../presets/bush_3.json"),
            Self::OakSmall => include_str!("../presets/oak_small.json"),
            Self::OakMedium => include_str!("../presets/oak_medium.json"),
            Self::OakLarge => include_str!("../presets/oak_large.json"),
            Self::PineSmall => include_str!("../presets/pine_small.json"),
            Self::PineMedium => include_str!("../presets/pine_medium.json"),
            Self::PineLarge => include_str!("../presets/pine_large.json"),
            Self::Trellis => include_str!("../presets/trellis.json"),
        }
    }

    /// Returns a fresh set of options with preset values applied over the defaults.
    /// The returned settings can be edited without changing the preset.
    pub fn options(self) -> TreeOptions {
        TreeOptions::from_json(self.json()).expect("built-in presets are valid tree options")
    }
}

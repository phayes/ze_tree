//! Ready-made tree settings covering several species and sizes: the presets
//! from ez-tree, plus trees that grow fruit or flowers.
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
    /// A single-trunked bigleaf maple of moderate height with a broad crown
    /// of large autumn-coloured leaves.
    BigLeafMapleMedium,
    /// A tall, many-stemmed bigleaf maple with a broad crown of large
    /// autumn-coloured leaves.
    BigLeafMapleLarge,
    /// A short, spreading apple tree hung with ripe apples.
    Apple,
    /// A low-forking flowering dogwood in full bloom, covered in white,
    /// four-bracted flowers that face the sky.
    FloweringDogwood,
    /// A many-stemmed pussy willow shrub in early spring: bare upright stems
    /// studded with fuzzy, upward-pointing catkins.
    SpringPussyWillow,
    /// A dense, many-stemmed willow thicket in winter: bare upright stems
    /// with no leaves, fruit or flowers.
    WinterThicket,
    /// A tall, narrow cedar with a single straight trunk and layered branches
    /// carrying flat, horizontal sprays of green foliage.
    Cedar,
}

impl TreePreset {
    /// Every preset, in an order suitable for a selection menu: the general
    /// species and sizes first, followed by the specialty trees such as fruit,
    /// flowering and winter trees.
    pub const ALL: [TreePreset; 23] = [
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
        Self::BigLeafMapleMedium,
        Self::BigLeafMapleLarge,
        Self::Apple,
        Self::FloweringDogwood,
        Self::SpringPussyWillow,
        Self::WinterThicket,
        Self::Cedar,
    ];

    /// The 13 presets whose settings match ez-tree's own presets, in ez-tree
    /// display order. They grow no fruit or flowers.
    pub const EZ_TREE: [TreePreset; 13] = [
        Self::AshSmall,
        Self::AshMedium,
        Self::AshLarge,
        Self::AspenSmall,
        Self::AspenMedium,
        Self::AspenLarge,
        Self::Bush1,
        Self::Bush2,
        Self::Bush3,
        Self::PineSmall,
        Self::PineMedium,
        Self::PineLarge,
        Self::Trellis,
    ];

    /// Returns the display name, such as `"Ash Small"`.
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
            Self::BigLeafMapleMedium => "Big Leaf Maple Medium",
            Self::BigLeafMapleLarge => "Big Leaf Maple Large",
            Self::Apple => "Apple",
            Self::FloweringDogwood => "Flowering Dogwood",
            Self::SpringPussyWillow => "Spring Pussy Willow",
            Self::WinterThicket => "Winter Thicket",
            Self::Cedar => "Cedar",
        }
    }

    /// Looks up an exact display name from [`name`](Self::name).
    ///
    /// Matching is case-sensitive and includes spaces: `"Ash Small"` matches,
    /// while `"ash_small"` does not. Returns `None` for an unrecognised name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.name() == name)
    }

    /// Returns the embedded preset JSON. It may omit settings; use
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
            Self::BigLeafMapleMedium => include_str!("../presets/big_leaf_maple_medium.json"),
            Self::BigLeafMapleLarge => include_str!("../presets/big_leaf_maple_large.json"),
            Self::Apple => include_str!("../presets/apple.json"),
            Self::FloweringDogwood => include_str!("../presets/flowering_dogwood.json"),
            Self::SpringPussyWillow => include_str!("../presets/spring_pussy_willow.json"),
            Self::WinterThicket => include_str!("../presets/winter_thicket.json"),
            Self::Cedar => include_str!("../presets/cedar.json"),
        }
    }

    /// Returns a fresh set of options with preset values applied over the defaults.
    /// The returned settings can be edited without changing the preset.
    pub fn options(self) -> TreeOptions {
        TreeOptions::from_json(self.json()).expect("built-in presets are valid tree options")
    }
}

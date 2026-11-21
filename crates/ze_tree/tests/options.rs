//! Check defaults, presets, and JSON handling against the ez-tree reference files.

mod common;

use glam::Vec3;
use serde_json::Value;
use ze_tree::{
    BarkUvMode, ClusterKind, Facing, FlowerOptions, FruitOptions, OptionsError, TreeOptions,
    TreePreset,
};

/// Converts a display name to the file stem shared by presets and fixtures,
/// for example `"Ash Small"` to `"ash_small"`.
fn preset_stem(preset: TreePreset) -> String {
    preset.name().to_lowercase().replace(' ', "_")
}

/// Returns true for values that hold only nulls, such as ez-tree's texture slots
/// (`bark.maps`, `leaves.map`), which the port does not have.
fn is_all_null(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(fields) => fields.values().all(is_all_null),
        _ => false,
    }
}

/// Compares expected fields recursively, allowing extra fields in `actual`.
/// Expected fields that hold only nulls are skipped, even if `actual` lacks them.
/// The `path` identifies the failing field in assertion messages.
///
/// # Panics
///
/// Panics if any other expected key is missing or a value differs.
fn assert_contains(expected: &Value, actual: &Value, path: &str) {
    match expected {
        Value::Null => {}
        Value::Object(fields) => {
            for (key, value) in fields {
                if is_all_null(value) {
                    continue;
                }
                let child = format!("{path}.{key}");
                let Some(actual_value) = actual.get(key) else {
                    panic!("{child} is missing from the output");
                };
                assert_contains(value, actual_value, &child);
            }
        }
        Value::Number(n) => {
            // Expected whole numbers must match exactly. For fractional values,
            // compare at f32 precision to allow for rounding in float settings.
            let actual = actual
                .as_f64()
                .unwrap_or_else(|| panic!("{path} is not a number"));
            let expected = n.as_f64().unwrap();
            if expected.fract() == 0.0 {
                assert_eq!(actual, expected, "{path}");
            } else {
                assert_eq!(actual as f32, expected as f32, "{path}");
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}

#[test]
fn defaults_match_ez_tree() {
    let from_js = TreeOptions::from_json(&common::fixture_text("options/default.json")).unwrap();
    assert_eq!(from_js, TreeOptions::default());
}

#[test]
fn default_json_matches_ez_tree_format() {
    let expected = common::fixture_json("options/default.json");
    let actual: Value = serde_json::from_str(&TreeOptions::default().to_json()).unwrap();
    assert_contains(&expected, &actual, "options");
}

#[test]
fn presets_match_ez_tree_merge() {
    let merged = common::fixture_json("options/presets_merged.json");
    for preset in TreePreset::EZ_TREE {
        let stem = preset_stem(preset);
        let expected = merged
            .get(&stem)
            .unwrap_or_else(|| panic!("no fixture for {stem}"));
        let expected = TreeOptions::from_json(&expected.to_string()).unwrap();
        assert_eq!(preset.options(), expected, "{stem}");

        // Check that the preset API returns the full embedded file contents.
        let file = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("presets/{stem}.json")),
        )
        .unwrap();
        assert_eq!(preset.json(), file, "{stem}");
    }
}

#[test]
fn preset_json_output_matches_ez_tree_format() {
    let merged = common::fixture_json("options/presets_merged.json");
    for preset in TreePreset::EZ_TREE {
        let stem = preset_stem(preset);
        let actual: Value = serde_json::from_str(&preset.options().to_json()).unwrap();
        assert_contains(&merged[&stem], &actual, &stem);
    }
}

#[test]
fn presets_are_valid_and_round_trip() {
    for preset in TreePreset::ALL {
        let options = preset.options();
        assert_eq!(options.validate(), Ok(()), "{}", preset.name());
        assert_eq!(
            TreeOptions::from_json(&options.to_json()).unwrap(),
            options,
            "{}",
            preset.name()
        );
        assert_eq!(TreePreset::from_name(preset.name()), Some(preset));
    }
    assert_eq!(TreePreset::from_name("Maple"), None);
}

#[test]
fn partial_json_keeps_nested_defaults() {
    let options = TreeOptions::from_json(
        r#"{
            "branch": { "angle": { "1": 10 }, "levels": 2 },
            "trellis": { "position": { "z": 5 } },
            "leaves": { "billboard": "single" }
        }"#,
    )
    .unwrap();

    let mut expected = TreeOptions::default();
    expected.branch.angle[1] = 10.0;
    expected.branch.levels = 2;
    expected.trellis.position = Vec3::new(0.0, 0.0, 5.0);
    expected.leaves.billboard = ze_tree::Billboard::Single;
    assert_eq!(options, expected);
}

#[test]
fn bark_uv_mode_defaults_to_continuous_and_reads_from_json() {
    assert_eq!(TreeOptions::default().bark.uv_mode, BarkUvMode::Continuous);
    for preset in TreePreset::ALL {
        assert_eq!(preset.options().bark.uv_mode, BarkUvMode::Continuous);
    }
    let options = TreeOptions::from_json(r#"{ "bark": { "uvMode": "mirrored" } }"#).unwrap();
    assert_eq!(options.bark.uv_mode, BarkUvMode::Mirrored);
    let json: Value = serde_json::from_str(&options.to_json()).unwrap();
    assert_eq!(json["bark"]["uvMode"], "mirrored");
}

#[test]
fn unknown_fields_are_ignored() {
    let options = TreeOptions::from_json(
        r#"{ "seed": 4, "colour": 1, "branch": { "angle": { "7": 1 }, "extra": true } }"#,
    )
    .unwrap();
    let expected = TreeOptions {
        seed: 4,
        ..TreeOptions::default()
    };
    assert_eq!(options, expected);
}

#[test]
fn serde_deserialize_requires_complete_options() {
    let options = TreePreset::OakMedium.options();
    let complete: TreeOptions = serde_json::from_str(&options.to_json()).unwrap();
    assert_eq!(complete, options);

    assert!(serde_json::from_str::<TreeOptions>(r#"{ "seed": 4 }"#).is_err());
}

#[test]
fn wrong_types_are_errors() {
    assert!(TreeOptions::from_json(r#"{ "seed": "12" }"#).is_err());
    assert!(TreeOptions::from_json(r#"{ "type": "palm" }"#).is_err());
    assert!(TreeOptions::from_json(r#"{ "branch": { "levels": -1 } }"#).is_err());
    assert!(TreeOptions::from_json("not json").is_err());
}

#[test]
fn seed_is_converted_like_javascript() {
    let seed = |json: &str| TreeOptions::from_json(json).unwrap().seed;
    assert_eq!(seed(r#"{ "seed": 26867 }"#), 26867);
    assert_eq!(seed(r#"{ "seed": -5 }"#), -5);
    assert_eq!(seed(r#"{ "seed": 3000000000 }"#), -1_294_967_296);
    assert_eq!(seed(r#"{ "seed": 4294967301 }"#), 5);
    assert_eq!(seed(r#"{ "seed": -2.7 }"#), -2);
}

#[test]
fn too_many_levels_is_rejected() {
    let mut options = TreeOptions::default();
    options.branch.levels = 4;
    assert_eq!(
        options.validate(),
        Err(OptionsError::TooManyLevels { levels: 4 })
    );
    options.branch.levels = 3;
    assert_eq!(options.validate(), Ok(()));
}

#[test]
fn zero_sections_is_rejected_only_for_levels_in_use() {
    let mut options = TreeOptions::default();
    options.branch.levels = 1;
    options.branch.sections[1] = 0;
    assert_eq!(
        options.validate(),
        Err(OptionsError::ZeroSections { level: 1 })
    );

    // With levels 0 and 1 configured, a zero section count at level 2 is ignored.
    options.branch.sections[1] = 5;
    options.branch.sections[2] = 0;
    assert_eq!(options.validate(), Ok(()));
}

#[test]
fn ez_tree_json_leaves_fruit_and_flowers_off() {
    let options = TreeOptions::from_json(&common::fixture_text("options/default.json")).unwrap();
    assert_eq!(options.fruit, FruitOptions::default());
    assert_eq!(options.flowers, FlowerOptions::default());
    for placement in [&options.fruit.placement, &options.flowers.placement] {
        assert_eq!((placement.count, placement.tips), (0, 0.0));
    }
}

#[test]
fn partial_fruit_and_flower_json_keeps_nested_defaults() {
    let options = TreeOptions::from_json(
        r#"{
            "fruit": { "cluster": { "kind": "panicle", "items": 40 }, "maturity": 0.25 },
            "flowers": { "placement": { "facing": "up", "count": 3 }, "shape": { "petalCount": 6 } }
        }"#,
    )
    .unwrap();

    let mut expected = TreeOptions::default();
    expected.fruit.cluster.kind = ClusterKind::Panicle;
    expected.fruit.cluster.items = 40;
    expected.fruit.maturity = 0.25;
    expected.flowers.placement.facing = Facing::Up;
    expected.flowers.placement.count = 3;
    expected.flowers.shape.petal_count = 6;
    assert_eq!(options, expected);
}

#[test]
fn fruit_and_flower_settings_round_trip() {
    let mut options = TreePreset::OakMedium.options();
    options.fruit.placement.count = 4;
    options.fruit.placement.facing = Facing::Outward;
    options.fruit.cluster.kind = ClusterKind::Umbel;
    options.fruit.shape.lobes = 5;
    options.fruit.shape.calyx = false;
    options.flowers.placement.tips = 0.5;
    options.flowers.cluster.kind = ClusterKind::Catkin;
    options.flowers.shape.fade_color = 0x123456;
    options.flowers.maturity = 0.9;

    let json = options.to_json();
    assert_eq!(TreeOptions::from_json(&json).unwrap(), options);

    // Serialized settings should use the documented JSON names and enum values.
    let value: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["fruit"]["placement"]["facing"], "outward");
    assert_eq!(value["fruit"]["cluster"]["kind"], "umbel");
    assert_eq!(value["fruit"]["shape"]["calyx"], false);
    assert_eq!(value["flowers"]["cluster"]["kind"], "catkin");
    assert_eq!(value["flowers"]["shape"]["fadeColor"], 0x123456);
    assert!(value["flowers"]["placement"]["sizeVariance"].is_number());
}

#[test]
fn fruit_and_flower_defaults_are_valid() {
    assert_eq!(TreeOptions::default().validate(), Ok(()));
}

#[test]
fn out_of_range_fruit_and_flower_settings_are_rejected() {
    let invalid = |field: &str, requirement: &'static str| OptionsError::InvalidSetting {
        field: field.to_owned(),
        requirement,
    };
    let check = |change: fn(&mut TreeOptions), error: OptionsError| {
        let mut options = TreeOptions::default();
        change(&mut options);
        assert_eq!(options.validate(), Err(error));
    };

    check(
        |o| o.fruit.maturity = 1.5,
        invalid("fruit.maturity", "between 0 and 1"),
    );
    check(
        |o| o.flowers.maturity = -0.1,
        invalid("flowers.maturity", "between 0 and 1"),
    );
    check(
        |o| o.fruit.maturity = f32::NAN,
        invalid("fruit.maturity", "between 0 and 1"),
    );
    check(
        |o| o.fruit.placement.size = -1.0,
        invalid("fruit.placement.size", "a finite value of 0 or more"),
    );
    check(
        |o| o.flowers.cluster.stalk_length = f32::INFINITY,
        invalid("flowers.cluster.stalkLength", "a finite value of 0 or more"),
    );
    check(
        |o| o.flowers.placement.size_variance = 1.5,
        invalid("flowers.placement.sizeVariance", "between 0 and 1"),
    );
    check(
        |o| o.fruit.cluster.side_stalks = 0,
        invalid("fruit.cluster.sideStalks", "at least 1"),
    );
    check(
        |o| o.fruit.shape.segments = 2,
        invalid("fruit.shape.segments", "at least 3"),
    );
    check(
        |o| o.fruit.shape.rings = 1,
        invalid("fruit.shape.rings", "at least 2"),
    );
    check(
        |o| o.fruit.shape.elongation = 0.0,
        invalid("fruit.shape.elongation", "a finite value above 0"),
    );
    check(
        |o| o.fruit.shape.lobe_depth = 1.0,
        invalid("fruit.shape.lobeDepth", "at least 0 and below 1"),
    );
    check(
        |o| o.flowers.shape.petal_count = 0,
        invalid("flowers.shape.petalCount", "at least 1"),
    );
    check(
        |o| o.flowers.shape.petal_segments = 0,
        invalid("flowers.shape.petalSegments", "at least 1"),
    );
    check(
        |o| o.flowers.shape.cup = 2.0,
        invalid("flowers.shape.cup", "between 0 and 1"),
    );

    // An invalid branch depth should be reported before an invalid fruit setting.
    check(
        |o| {
            o.branch.levels = 4;
            o.fruit.maturity = 2.0;
        },
        OptionsError::TooManyLevels { levels: 4 },
    );
}

#[test]
fn invalid_setting_message_names_the_field() {
    let mut options = TreeOptions::default();
    options.fruit.cluster.droop = 3.0;
    let message = options.validate().unwrap_err().to_string();
    assert_eq!(message, "fruit.cluster.droop must be between 0 and 1");
}

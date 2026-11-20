//! Compare random draws with reference sequences recorded from ez-tree.

mod common;

use ze_tree::{Rng, TreeOptions};

#[test]
fn matches_ez_tree_streams() {
    let streams = common::fixture_json("rng.json");
    let streams = streams.as_array().expect("rng.json is an array");
    assert!(!streams.is_empty());

    for stream in streams {
        // Import seeds through the options API to cover the conversion used by
        // presets, including values outside the signed 32-bit range.
        let seed_json = &stream["seed"];
        let options =
            TreeOptions::from_json(&format!(r#"{{"seed": {seed_json}}}"#)).expect("seed parses");
        let mut rng = Rng::new(options.seed);

        let expected = stream["values"].as_array().expect("values array");
        assert!(!expected.is_empty());
        for (i, value) in expected.iter().enumerate() {
            let expected = u32::try_from(value.as_u64().expect("u32 value")).expect("fits u32");
            assert_eq!(rng.next_u32(), expected, "seed {seed_json}, draw {i}");
        }
    }
}

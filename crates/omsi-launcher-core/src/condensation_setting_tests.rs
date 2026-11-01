/// (NexiconDK, 1 November 2026) The condensation on the windows and the chrono events' card, as
/// the game's menu saves them through the launcher's settings file: what was chosen is written
/// (once), not the old line again; an unknown value is the default.
#[test]
fn the_condensation_and_the_chrono_card_are_saved_as_chosen() {
    let fresh = super::settings_from_text(None);
    assert_eq!((fresh["condensation"].clone(), fresh["chrono_info"].clone()), (serde_json::json!("realistic"), serde_json::json!(true)));
    for look in ["realistic", "light", "off"] {
        let mut settings = super::settings_from_text(Some("condensation=realistic\nchrono_info=1\n"));
        settings["condensation"] = serde_json::json!(look);
        settings["chrono_info"] = serde_json::json!(false);
        let saved = super::settings_to_text(&settings, Some("condensation=realistic\nchrono_info=1\n"));
        assert_eq!(saved.lines().filter(|l| l.starts_with("condensation=")).count(), 1, "{saved}");
        assert_eq!(saved.lines().filter(|l| l.starts_with("chrono_info=")).count(), 1, "{saved}");
        let back = super::settings_from_text(Some(&saved));
        assert_eq!((back["condensation"].clone(), back["chrono_info"].clone()), (serde_json::json!(look), serde_json::json!(false)));
    }
    assert_eq!(super::settings_from_text(Some("condensation=foggy\n"))["condensation"], serde_json::json!("realistic"));
}

/// (NexiconDK, 1 November 2026) The speed cameras' setting goes through the launcher's settings
/// file as chosen in the game's menu, once; an unknown value is `visible`.
#[test]
fn the_speed_cameras_are_saved_as_chosen() {
    assert_eq!(super::settings_from_text(None)["speed_cameras"], serde_json::json!("visible"));
    for how in ["visible", "invisible", "off"] {
        let mut settings = super::settings_from_text(Some("speed_cameras=visible\n"));
        settings["speed_cameras"] = serde_json::json!(how);
        let saved = super::settings_to_text(&settings, Some("speed_cameras=visible\n"));
        assert_eq!(saved.lines().filter(|l| l.starts_with("speed_cameras=")).count(), 1, "{saved}");
        assert_eq!(super::settings_from_text(Some(&saved))["speed_cameras"], serde_json::json!(how));
    }
    assert_eq!(super::settings_from_text(Some("speed_cameras=hidden\n"))["speed_cameras"], serde_json::json!("visible"));
}

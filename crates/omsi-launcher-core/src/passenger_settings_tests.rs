//! (openOMSI PR #3427/#3428) The passengers' optional visual settings survive the launcher's saves.
use super::*;

#[test]
fn optional_passenger_preference_survives_launcher_saves() {
    let key = "passenger_hailing";
    {
        assert_eq!(settings_from_text(None)[key], false);
        assert_eq!(
            settings_from_text(Some("version=1\n"))[key],
            false
        );
        for on in [false, true, false] {
            let old = format!("{key}=0\n{key}=1\n");
            let mut settings = settings_from_text(Some(&old));
            settings[key] = json!(on);
            let text = settings_to_text(&settings, Some(&old));
            assert_eq!(
                text.lines()
                    .filter(|line| line.starts_with(&format!("{key}=")))
                    .count(),
                1
            );
            assert_eq!(settings_from_text(Some(&text))[key], on);
        }
    }
}


#[test]
fn optional_full_bus_preference_survives_launcher_saves() {
    let key = "passenger_full_bus_reaction";
    {
        assert_eq!(settings_from_text(None)[key], false);
        assert_eq!(
            settings_from_text(Some("version=1\n"))[key],
            false
        );
        for on in [false, true, false] {
            let old = format!("{key}=0\n{key}=1\n");
            let mut settings = settings_from_text(Some(&old));
            settings[key] = json!(on);
            let text = settings_to_text(&settings, Some(&old));
            assert_eq!(
                text.lines()
                    .filter(|line| line.starts_with(&format!("{key}=")))
                    .count(),
                1
            );
            assert_eq!(settings_from_text(Some(&text))[key], on);
        }
    }
}

/// (NexiconDK) Waving after a bus that left them: on unless the player turned it off.
#[test]
fn the_wave_after_the_bus_is_on_by_default_and_kept_off() {
    let key = "passenger_wave_after";
    assert_eq!(settings_from_text(None)[key], true);
    let mut settings = settings_from_text(Some("version=1\n"));
    assert_eq!(settings[key], true);
    settings[key] = json!(false);
    let text = settings_to_text(&settings, None);
    assert!(text.contains("passenger_wave_after=0"));
    assert_eq!(settings_from_text(Some(&text))[key], false);
}

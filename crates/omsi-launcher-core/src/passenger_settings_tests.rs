//! The passengers' gestures for a bus survive the launcher's saves: (NexiconDK, 31 October 2026)
//! what share of them signal to a coming bus, wave to it, glance at a full one, hold up their
//! phone's light in the dark - in per cent, "sometimes" (30) by default. (They took the place of
//! the switches of openOMSI PR #3427/#3428 and of the waving after a bus.)
use super::*;

#[test]
fn the_gesture_shares_are_sometimes_by_default_and_kept() {
    for key in ["pax_signal", "pax_wave", "pax_full", "pax_phone", "ai_greet"] {
        assert_eq!(settings_from_text(None)[key], 30, "{key}");
        assert_eq!(settings_from_text(Some("version=1\n"))[key], 30, "{key}");
        for want in [100, 10, 0, 30] {
            let old = format!("{key}=30\n{key}=100\n");
            let mut settings = settings_from_text(Some(&old));
            settings[key] = json!(want);
            let text = settings_to_text(&settings, Some(&old));
            assert_eq!(text.lines().filter(|line| line.starts_with(&format!("{key}="))).count(), 1, "{key}");
            assert_eq!(settings_from_text(Some(&text))[key], want, "{key}");
        }
        // (a share is a share: at most everybody)
        let text = settings_to_text(&settings_from_text(Some(&format!("{key}=250%\n"))), None);
        assert!(text.contains(&format!("{key}=100\n")), "{key}");
    }
}

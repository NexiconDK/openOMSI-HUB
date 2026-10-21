//! Omsi-Hub: the extra text window - a text of two lines ("EKSTRABUS", "Via Hovedbanen")
//! that takes turns with the destination on the bus's displays every few seconds, switched
//! on and off here (`player::extra_sign`). It stays on through a change of destination. The
//! destination window's second tab (`sign_tabs`), and the vehicle menu's "Extra text".

use super::{button, card, grow_row, input, note, sign_tabs, text, title};
use omsi_plugin::api::Value;
use omsi_plugin::ui::{Anchor, Panel, Weight};

/// What is typed into the window.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ExtraForm {
    pub lines: [String; 2],
    /// Seconds each text is shown (empty: `player::EXTRA_EVERY`).
    pub every: String,
    /// The extra text is on.
    pub on: bool,
    pub(super) said: Option<String>,
}

impl ExtraForm {
    pub(crate) fn panel(&self) -> Panel {
        let state = if self.on { "On: the extra text takes turns with the destination" } else { "Off" };
        let children = vec![
            title("Destination display"),
            sign_tabs(1),
            text(state, 13.0, Weight::Medium),
            text("Extra text, two lines", 13.0, Weight::Medium),
            input("line1", &self.lines[0], "EKSTRABUS", 60),
            input("line2", &self.lines[1], "Second line (may stay empty)", 60),
            text("Seconds each text is shown", 13.0, Weight::Medium),
            input("every", &self.every, "4", 3),
            grow_row(vec![button("off", "Turn off", Some("block")), button("on", "Turn on", Some("check")), button("close", "Close", Some("close"))]),
            note(self.said.as_deref().unwrap_or("The destination and the extra text take turns on the displays. The IBIS and the passengers keep the destination.")),
        ];
        card(Anchor::Center, 0.0, 0.0, 420.0, 236, children)
    }
}

/// The window opened, with the extra text the bus has.
pub(crate) fn open(app: &mut crate::App) {
    let Some(p) = app.player.as_ref() else {
        app.service_msg = Some(("No vehicle to set a destination for".into(), 3.0));
        return;
    };
    let form = match p.extra_text.as_ref() {
        Some(x) => ExtraForm { lines: x.text.clone(), every: format!("{:.0}", x.every), on: true, said: None },
        None => ExtraForm::default(),
    };
    app.menus.game_panels.extra = Some(form);
}

/// A field typed into.
pub(crate) fn change(app: &mut crate::App, id: &str, value: &Value) {
    if id == "sign_tab" {
        return super::sign_tab(app, value);
    }
    let (Some(f), Value::Str(s)) = (app.menus.game_panels.extra.as_mut(), value) else { return };
    match id {
        "line1" => f.lines[0] = s.clone(),
        "line2" => f.lines[1] = s.clone(),
        "every" => f.every = s.clone(),
        _ => {}
    }
}

pub(crate) fn click(app: &mut crate::App, id: &str) {
    match id {
        "close" => app.menus.game_panels.extra = None,
        "on" | "off" => {
            let Some(form) = app.menus.game_panels.extra.clone() else { return };
            let Some(p) = app.player.as_mut() else { return };
            let want = (id == "on").then(|| crate::player::ExtraText { text: [form.lines[0].trim().to_string(), form.lines[1].trim().to_string()], every: every_of(&form.every) });
            let said = p.set_extra_text(want);
            let on = p.extra_text.is_some();
            if let Some(f) = app.menus.game_panels.extra.as_mut() {
                f.said = Some(said);
                f.on = on;
            }
        }
        // (Enter in a field: as "Turn on")
        "line1" | "line2" | "every" => click(app, "on"),
        _ => {}
    }
}

/// The seconds typed, between 1 and 60 (the default when none).
fn every_of(typed: &str) -> f32 {
    typed.trim().replace(',', ".").parse::<f32>().ok().filter(|s| s.is_finite()).unwrap_or(crate::player::EXTRA_EVERY).clamp(1.0, 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The destination window's two tabs: the destination's card has the first picked, the
    /// extra text's the second, and both are the same window (its title, width and place).
    #[test]
    fn the_destination_and_the_extra_text_are_tabs_of_one_window() {
        use omsi_plugin::ui::Kind;
        let tab = |p: &Panel| p.children.iter().find(|e| e.id.as_deref() == Some("sign_tab")).map(|e| e.kind.clone());
        let dest = super::super::dest_form::DestForm::default().panel();
        let extra = ExtraForm::default().panel();
        assert!(matches!(tab(&dest), Some(Kind::Tabs { selected: 0, ref tabs }) if tabs.len() == 2));
        assert!(matches!(tab(&extra), Some(Kind::Tabs { selected: 1, .. })));
        assert_eq!((dest.width, dest.x, dest.y), (extra.width, extra.x, extra.y));
        assert_eq!(format!("{:?}", dest.children[0]), format!("{:?}", extra.children[0]), "the same title");
    }

    #[test]
    fn the_seconds_typed() {
        assert_eq!(every_of(""), crate::player::EXTRA_EVERY);
        assert_eq!(every_of("2,5"), 2.5);
        assert_eq!(every_of("0"), 1.0);
        assert_eq!(every_of("600"), 60.0);
        assert_eq!(every_of("x"), crate::player::EXTRA_EVERY);
    }
}

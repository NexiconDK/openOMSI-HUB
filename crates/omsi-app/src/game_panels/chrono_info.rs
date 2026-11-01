//! (NexiconDK, 1 November 2026) The map's chrono scenarios in force, as OMSI 2 tells them: at
//! the start of a drive and when one comes into force at midnight, a card with each one's
//! name, its dates and what its `[description]` says (a match day's shuttles, a fair's
//! duties). It waits until the bus stands, as the trip's summary does: it takes the mouse.
//! Settings → Gameplay → "Chrono events" turns it off.

use super::{button, card, divider, el, grow_row, note, row, text, title};
use omsi_map::ailists::ChronoCfg;
use omsi_plugin::ui::{Anchor, Element, Kind, Panel, Rgba, RowAlign, Weight};
use omsi_ui::tr;

const ACCENT: Rgba = Rgba([232, 160, 48, 255]);

/// Most scenarios one card shows (the newest; a map seldom has more in force at once).
const SHOWN: usize = 5;
/// Most lines of a description shown.
const LINES: usize = 14;

/// A date code (yyyymmdd) as the maps' folders write it: 02.11.2025.
fn day(code: i32) -> String {
    format!("{:02}.{:02}.{}", code % 100, code / 100 % 100, code / 10000)
}

/// When a scenario is in force, in words: one day, from - to, or from on.
pub(crate) fn dates_of(c: &ChronoCfg) -> String {
    match (c.start_date, c.end_date) {
        (s, e) if s > 0 && s == e => day(s),
        (s, e) if s > 0 && e > 0 => format!("{} – {}", day(s), day(e)),
        (s, _) if s > 0 => tr("from %{date}").replace("%{date}", &day(s)),
        (_, e) if e > 0 => tr("until %{date}").replace("%{date}", &day(e)),
        _ => String::new(),
    }
}

pub(crate) struct ChronoPanel {
    events: Vec<ChronoCfg>,
}

impl ChronoPanel {
    pub(crate) fn new(events: Vec<ChronoCfg>) -> ChronoPanel {
        ChronoPanel { events }
    }

    /// More scenarios to tell (another came into force before this card was closed).
    pub(crate) fn add(&mut self, more: Vec<ChronoCfg>) {
        for m in more {
            if !self.events.iter().any(|e| e.name == m.name && e.start_date == m.start_date) {
                self.events.push(m);
            }
        }
    }

    pub(crate) fn panel(&self) -> Panel {
        let icon = Element { color: Some(ACCENT), ..el(None, Kind::Icon { name: "event".to_string(), size: 22.0 }) };
        let mut children = vec![row(vec![icon, Element { grow: true, ..title(&tr("Chrono events")) }], RowAlign::Start)];
        let from = self.events.len().saturating_sub(SHOWN);
        for (k, e) in self.events[from..].iter().enumerate() {
            if k > 0 {
                children.push(divider());
            }
            children.push(text(&e.name, 15.0, Weight::Bold));
            let dates = dates_of(e);
            if !dates.is_empty() {
                children.push(note(&dates));
            }
            if !e.description.is_empty() {
                let mut lines: Vec<&str> = e.description.iter().map(String::as_str).take(LINES).collect();
                if e.description.len() > LINES {
                    lines.push("…");
                }
                children.push(text(&lines.join("\n"), 13.0, Weight::Regular));
            }
        }
        children.push(grow_row(vec![button("close", &tr("Close"), Some("close"))]));
        children.push(row(vec![note(&tr("Settings → Gameplay → Chrono events turns it off."))], RowAlign::Start));
        card(Anchor::Center, 0.0, 0.0, 540.0, 236, children)
    }
}

pub(crate) fn click(app: &mut crate::App, id: &str) {
    if id == "close" {
        app.menus.game_panels.chrono = None;
    }
}

/// The scenarios to tell of in `dirs` (in force): those with a description.
pub(crate) fn events_in(dirs: &[std::path::PathBuf]) -> Vec<ChronoCfg> {
    omsi_map::chrono_events(dirs).into_iter().map(|(_, c)| c).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scenarios_dates_in_words() {
        let c = |s, e| ChronoCfg { start_date: s, end_date: e, ..Default::default() };
        assert_eq!(dates_of(&c(20251102, 20251102)), "02.11.2025");
        assert_eq!(dates_of(&c(20251119, 20251123)), "19.11.2025 – 23.11.2025");
        assert!(dates_of(&c(20251013, 0)).contains("13.10.2025"));
        assert_eq!(dates_of(&c(0, 0)), "");
    }

    /// The card: the scenario's name, its date, its description, and the way to turn it off.
    #[test]
    fn the_card_tells_the_scenario() {
        let e = ChronoCfg { name: "FCA Heimspiel 02.11.2025 - 14.00 Uhr".into(), start_date: 20251102, end_date: 20251102, description: vec!["FCA Heimspiel im Sportpark Seppenfeld.".into()], ..Default::default() };
        let mut p = ChronoPanel::new(vec![e.clone()]);
        p.add(vec![e]);
        assert_eq!(p.events.len(), 1, "the same scenario once");
        let shown = format!("{:?}", p.panel().children);
        for want in ["FCA Heimspiel 02.11.2025 - 14.00 Uhr", "02.11.2025", "Sportpark Seppenfeld", "close"] {
            assert!(shown.contains(want), "{want}");
        }
    }
}

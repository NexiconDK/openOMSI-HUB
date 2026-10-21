//! Omsi-Hub: an extra text on the bus's destination displays that takes turns with the
//! destination ("EKSTRABUS", "Via Hovedbanen"), as the Danish and German matrices show it:
//! switched on and off in its own window (`game_panels::extra_form`), and kept on through a
//! change of destination until the driver switches it off.
//!
//! The IBIS and the passengers are left as they are: the destination keeps its row of the
//! depot file, its code and its name, and only the row's texts change for the turns of the
//! extra text - the displays read them from the depot file (`GetTerminusString`). Each turn
//! sets that same row again, as picking it by hand does, so that the display scripts write
//! the texts anew. A destination set otherwise (the IBIS typed, the list, the duty's next
//! trip) ends the turns.

use super::*;
use std::sync::Arc;

/// The turns of the extra text.
#[derive(Debug, Clone)]
pub(crate) struct ExtraSign {
    /// The depot file with the destination's own texts, and one with the extra text in their
    /// place (row `row`).
    main: Arc<omsi_vehicle::Hof>,
    extra: Arc<omsi_vehicle::Hof>,
    row: usize,
    line: String,
    /// Seconds each text is shown, how long the current one has been, and whether it is the
    /// extra text.
    every: f32,
    t: f32,
    showing_extra: bool,
    /// The IBIS has held the destination's row (a display in the middle of a change takes
    /// it a moment later, see `schedule::apply_pending_destination`).
    seen: bool,
}

/// How long each text is shown when the form says nothing (s).
pub(crate) const EVERY_DEFAULT: f32 = 4.0;

impl ExtraSign {
    /// The turns of `text` (two lines) with row `row` of `hof`, each shown `every` seconds;
    /// None when there is no text, or no such row.
    pub(crate) fn new(hof: Arc<omsi_vehicle::Hof>, row: usize, line: &str, text: [&str; 2], every: f32) -> Option<ExtraSign> {
        let (a, b) = (text[0].trim(), text[1].trim());
        if a.is_empty() && b.is_empty() {
            return None;
        }
        let own = hof.termini.get(row)?;
        // (as many texts as the row has: the first line where the row has one text, the
        // second on its second, and the first again on the rest - the side and rear signs;
        // not the destination's picture, which a Krüger matrix shows instead of the text, nor
        // the strings the file leaves empty, the plug-in sign's: `Hof::text_strings`)
        let mut strings = hof.text_strings([a, b]);
        strings.resize(own.strings.len().max(2), String::new());
        let mut extra = (*hof).clone();
        extra.termini[row].strings = strings;
        Some(ExtraSign { main: hof, extra: Arc::new(extra), row, line: line.trim().to_string(), every: every.clamp(1.0, 60.0), t: 0.0, showing_extra: false, seen: false })
    }

    /// The text shown now is the extra one.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn showing_extra(&self) -> bool {
        self.showing_extra
    }

    /// A frame of `dt` seconds: the other text once this one has had its time. False when
    /// the turns are over: the bus shows another destination now.
    pub(crate) fn tick(&mut self, v: &mut omsi_sim::VehicleInstance, dt: f32) -> bool {
        // (another destination was set: the IBIS holds another row, or the bus another depot file)
        let row_now = v.var("IBIS_TerminusIndex").filter(|i| i.is_finite() && *i >= 0.0).map(|i| i.round() as usize);
        let ours = v.host.hof.as_ref().is_some_and(|h| Arc::ptr_eq(h, &self.main) || Arc::ptr_eq(h, &self.extra));
        if row_now == Some(self.row) {
            self.seen = true;
        }
        if !ours || (self.seen && row_now.is_some_and(|r| r != self.row)) {
            return false;
        }
        self.t += dt;
        if self.t < self.every {
            return true;
        }
        self.t = 0.0;
        self.showing_extra = !self.showing_extra;
        self.show(v, self.showing_extra);
        true
    }

    /// Put the destination's own texts back (the turns are over).
    pub(crate) fn stop(&self, v: &mut omsi_sim::VehicleInstance) {
        if self.showing_extra && v.host.hof.as_ref().is_some_and(|h| Arc::ptr_eq(h, &self.extra)) {
            self.show(v, false);
        }
    }

    /// The depot file with the extra text off the bus, where it is still there (the bus set
    /// to another row of it): the same file with the destination's own texts in its place.
    pub(crate) fn release(&self, v: &mut omsi_sim::VehicleInstance) {
        if v.host.hof.as_ref().is_some_and(|h| Arc::ptr_eq(h, &self.extra)) {
            v.host.hof = Some(self.main.clone());
        }
    }

    fn show(&self, v: &mut omsi_sim::VehicleInstance, extra: bool) {
        let hof = if extra { self.extra.clone() } else { self.main.clone() };
        v.host.hof = Some(hof.clone());
        crate::schedule::set_player_destination_at(v, &hof, &self.line, self.row, &[]);
    }
}

/// What the driver asked for: the extra text and how many seconds each text is shown, until
/// it is switched off again (`Player::extra_text`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExtraText {
    pub text: [String; 2],
    pub every: f32,
}

impl Player {
    /// The extra text's turns, a frame of them: while it is switched on it takes turns with
    /// whichever destination the bus shows, a new one as well (the IBIS typed, the list, the
    /// duty's next trip).
    pub(crate) fn tick_extra_sign(&mut self, dt: f32) {
        if let Some(mut s) = self.extra_sign.take() {
            if s.tick(&mut self.vehicle, dt) {
                self.extra_sign = Some(s);
                return;
            }
            // (another destination: the destination's own texts - a depot file of ours left
            // on the bus has them in the other rows, ours back as well)
            s.release(&mut self.vehicle);
            self.extra_wait = 0.0;
        }
        let Some(want) = self.extra_text.clone() else { return };
        // (once a second, until the bus shows a destination to take turns with)
        self.extra_wait -= dt;
        if self.extra_wait > 0.0 {
            return;
        }
        self.extra_wait = 1.0;
        let row = self.vehicle.var("IBIS_TerminusIndex").filter(|i| i.is_finite() && *i >= 0.0).map(|i| i.round() as usize);
        let (Some(row), Some(hof)) = (row, self.vehicle.host.hof.clone()) else { return };
        let line = crate::game_lists::destination_line(&self.vehicle);
        self.extra_sign = ExtraSign::new(hof, row, &line, [&want.text[0], &want.text[1]], want.every);
        if self.extra_sign.is_some() {
            log::info!("extra text on the displays: {:?} / {:?} every {:.0} s with destination row {row}", want.text[0], want.text[1], want.every);
        }
    }

    /// The extra text's turn over and the destination's own texts back on the bus, for a
    /// destination about to be set by hand (it takes turns again with the new one).
    pub(crate) fn pause_extra_sign(&mut self) {
        if let Some(s) = self.extra_sign.take() {
            s.stop(&mut self.vehicle);
            s.release(&mut self.vehicle);
        }
        self.extra_wait = 0.5;
    }

    /// Switch the extra text on (`Some`) or off. What was done, in words, for the form.
    pub(crate) fn set_extra_text(&mut self, want: Option<ExtraText>) -> String {
        self.pause_extra_sign();
        self.extra_wait = 0.0;
        let want = want.filter(|w| w.text.iter().any(|t| !t.trim().is_empty()));
        let said = match &want {
            None => omsi_ui::tr("Extra text off").into_owned(),
            Some(_) if crate::schedule::has_roller_blind(&self.vehicle) => omsi_ui::tr("A roller blind cannot show an extra text").into_owned(),
            Some(_) => omsi_ui::tr("Extra text on: it takes turns with the destination").into_owned(),
        };
        self.extra_text = want.filter(|_| !crate::schedule::has_roller_blind(&self.vehicle));
        if self.extra_text.is_none() {
            log::info!("extra text on the displays switched off");
        }
        said
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_vehicle::hof::{Hof, Terminus};

    #[test]
    fn the_extra_text_takes_turns_and_the_ibis_keeps_its_destination() {
        let t = |code: i32, id: &str, s: &[&str]| Terminus { code, texture_id: id.into(), strings: s.iter().map(|x| x.to_string()).collect(), ..Default::default() };
        let hof = Arc::new(Hof { termini: vec![t(0, "Leer", &[""]), t(282, "Hovedbanen", &["HOVEDBANEN", "via Centrum"])], ..Default::default() });
        // (a matrix as the stock scripts write it: the row's first text on the trigger)
        let osc = "{frame}\n{end}\n{trigger:ai_scheduled_settarget}\n(L.L.AI_target_index) 0 (M.V.GetTerminusString) (S.$.sign)\n(L.L.AI_target_index) 1 (M.V.GetTerminusString) (S.$.sign2)\n{end}\n";
        let mut v = crate::schedule::tests::script_test_vehicle(osc, "AI_target_index\nIBIS_TerminusIndex\nIBIS_TerminusCode\n", "SetLineTo\nsign\nsign2\n");
        v.host.hof = Some(hof.clone());
        crate::schedule::set_player_destination_at(&mut v, &hof, "5", 1, &[]);
        assert_eq!(v.str_var("sign"), "HOVEDBANEN");
        let mut s = ExtraSign::new(hof.clone(), 1, "5", ["EKSTRABUS", ""], 4.0).unwrap();
        assert!(s.tick(&mut v, 3.0) && v.str_var("sign") == "HOVEDBANEN", "not yet");
        assert!(s.tick(&mut v, 1.5) && s.showing_extra());
        assert_eq!((v.str_var("sign"), v.str_var("sign2")), ("EKSTRABUS".to_string(), String::new()));
        // the IBIS and what the passengers read: the same row, the same code, the same name
        assert_eq!((v.var("IBIS_TerminusIndex"), v.var("IBIS_TerminusCode")), (Some(1.0), Some(282.0)));
        assert_eq!(v.host.hof.as_ref().unwrap().termini[1].texture_id, "Hovedbanen");
        assert!(s.tick(&mut v, 4.0) && !s.showing_extra());
        assert_eq!((v.str_var("sign"), v.str_var("sign2")), ("HOVEDBANEN".to_string(), "via Centrum".to_string()));
        // another destination set: the turns end
        let other = Arc::new(Hof { termini: hof.termini.clone(), ..Default::default() });
        v.host.hof = Some(other);
        assert!(!s.tick(&mut v, 4.0));
        assert!(ExtraSign::new(hof.clone(), 1, "5", [" ", ""], 4.0).is_none());
        assert!(ExtraSign::new(hof, 7, "5", ["X", ""], 4.0).is_none());
    }
}

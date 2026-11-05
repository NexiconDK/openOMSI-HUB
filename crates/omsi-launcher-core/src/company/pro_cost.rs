//! RosterFlow Pro, round B (the player's, NexiconDK 5 November 2026: "Hvad ugen koster"): what a
//! week of the calendar costs in wages, as planned, and what a duty costs a driver extra.
//!
//! - **A driver's week** (`week`): his hours of the week as planned (`pro_ops::week_minutes`) paid
//!   by his employment - a permanent driver his contract week at least (the guarantee), the hours
//!   above it as overtime (`econ_uk::OVERTIME_RATE`), a reserve his frames and the rest, the hourly
//!   paid their hours -, the supplements of the evening, the night and the weekend on the pieces
//!   worked (`pay::supplement_minutes`), and the days of time off in lieu at the normal rate. The
//!   overtime of a duty given as time off in lieu (`pro_ops::Call::toil`) is saved, not paid.
//!   The wages without the on-costs, as the popup's prices.
//! - **A duty for a driver** (`duty_supplement`): the supplements of its pieces for him, on top
//!   of its price (`pro_ops::price_of`).

use super::model::{Cents, Company, Employee, Employment};
use super::{econ_uk, pay};

/// What a week costs in wages (cents, without the on-costs): all of it, of it the overtime and
/// the supplements; the overtime's minutes saved as time off in lieu instead.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeekCost {
    pub wages: Cents,
    pub overtime: Cents,
    pub supplements: Cents,
    pub saved: f64,
}

impl std::ops::AddAssign for WeekCost {
    fn add_assign(&mut self, o: WeekCost) {
        self.wages += o.wages;
        self.overtime += o.overtime;
        self.supplements += o.supplements;
        self.saved += o.saved;
    }
}

/// Driver `e`'s week: `minutes` of it planned (`pro_ops::week_minutes`), `days` its days (each its
/// weekday, `dates::weekday`, and pieces of work: minutes of the day), `toil_days` the minutes of
/// its days of time off in lieu, `saving` whether a duty of it was given as time off in lieu (its
/// overtime saved, at most `saving_minutes` of it).
pub fn week(c: &Company, e: &Employee, minutes: f64, days: &[(u32, Vec<(i32, i32)>)], toil_days: f64, saving_minutes: f64) -> WeekCost {
    let hour = pay::hour_wage(e);
    let rate = if econ_uk::mode(c.game_mode).overtime { econ_uk::OVERTIME_RATE } else { 1.0 };
    let full = pay::contract_week_minutes(c);
    let (paid, over) = match e.employment {
        Employment::Hourly => (minutes, (minutes - full).max(0.0)),
        Employment::Reserve(f) => (minutes, (minutes - f.week_minutes() as f64).max(0.0)),
        _ => {
            let k = super::pro_ops::contract_minutes(c, e).unwrap_or(full);
            (minutes.max(k), (minutes - k).max(0.0))
        }
    };
    let saved = if super::pro_toil::can_save(e) { over.min(saving_minutes.max(0.0)) } else { 0.0 };
    let over_paid = over - saved;
    let normal = paid - over;
    let supp: f64 = if super::pro_duties::on(c) { days.iter().map(|(wd, ps)| pay::supplement_minutes(*wd, ps)).sum() } else { 0.0 };
    let cents = |x: f64| x.round() as Cents;
    let overtime = cents(hour * over_paid * rate / 60.0);
    let supplements = cents(hour * supp / 60.0);
    let wages = cents(hour * (normal + toil_days) / 60.0) + overtime + supplements;
    WeekCost { wages, overtime, supplements, saved }
}

/// The supplements of duty pieces `pieces` (minutes of the day) on `date` for driver `e` (cents,
/// without the on-costs): none without RosterFlow Pro.
pub fn duty_supplement(c: &Company, e: &Employee, date: &str, pieces: &[(i32, i32)]) -> Cents {
    pay::supplement_cost(c, e, date, pieces).round() as Cents
}

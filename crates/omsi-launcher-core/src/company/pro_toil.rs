//! RosterFlow Pro, round B (the player's, NexiconDK 5 November 2026: "Afspadsering"): overtime
//! saved as time off in lieu instead of paid.
//!
//! - **Saving**: a duty of overtime given a permanent driver (full or part time) is paid or saved
//!   (`pro_ops::set_toil`; the company's setting decides what the popup offers first:
//!   `Ops::toil_default`). Saved, the day's overtime is not paid but added to his hours of time off
//!   in lieu, hour for hour (`pay::day`, `Employee::toil`) - cheaper now.
//! - **Taking it**: a day of time off in lieu (`give_day`) takes a working day's hours from them
//!   (his contract week's fifth, fewer when fewer are saved) and pays them at the normal rate that
//!   day; he is off (`model::FreeDay`: the plan keeps him off, his rota's duty is open). Taken back
//!   before the day (`take_back`), the hours are his again.
//! - **Too many saved**: above a full week of them (`TOO_MANY`) he is less content every day, and
//!   with half a working week saved he asks the control room now and then when he can have them
//!   ("Kan jeg snart få mine 12 timers afspadsering?" - `daily`, at most once a week).

use super::model::{Company, Employee, Employment, FreeDay, FreeWhy};
use super::{dates, messages, staff};

/// The hours of time off in lieu (minutes) above which a driver is less content every day
/// (`TOO_MANY_COST` a day), and from which he asks for them now and then (`ASKS_FROM`).
pub const TOO_MANY: f64 = 37.0 * 60.0;
pub const TOO_MANY_COST: f64 = 0.5;
pub const ASKS_FROM: f64 = 12.0 * 60.0;
/// The chance a day that he asks, at most once in `ASK_DAYS` days.
pub const ASK_CHANCE: f64 = 0.25;
pub const ASK_DAYS: i64 = 7;
/// The least a day of time off in lieu takes (minutes).
pub const LEAST: f64 = 60.0;

/// The texts (English: keys of the translations).
pub const ONLY_PERMANENT: &str = "Only a permanent driver saves time off in lieu.";
pub const TOO_FEW: &str = "Not enough time off in lieu saved: at least 1 hour.";
pub const OFF_ALREADY: &str = "The driver is off that day already.";
pub const GONE: &str = "That day has gone.";
pub const ASKS: &str = "Can I have my %{h} hours of time off in lieu soon?";
pub const ASKED_FEED: &str = "%{name}: Can I have my %{h} hours of time off in lieu soon?";

/// Whether driver `e` saves overtime as time off in lieu: a permanent driver, full or part time.
pub fn can_save(e: &Employee) -> bool {
    staff::drives(e) && matches!(e.employment, Employment::FullTime | Employment::PartTime { .. })
}

/// The minutes a day of time off in lieu takes of driver `e`'s: his contract week's fifth.
pub fn day_of(c: &Company, e: &Employee) -> f64 {
    let full = super::pay::contract_week_minutes(c);
    let week = match e.employment {
        Employment::PartTime { hours } => (hours * 60.0).clamp(60.0, full),
        _ => full,
    };
    (week / 5.0).round()
}

/// Whether overtime is saved as time off in lieu unless the player chooses otherwise.
pub fn default_on(c: &Company) -> bool {
    c.planning.ops.toil_default
}

pub fn set_default(c: &mut Company, on: bool) {
    c.planning.ops.toil_default = on;
}

/// A day of time off in lieu for driver `driver` on `date` (today or later): a working day's hours
/// of his saved ones, fewer when fewer are saved (at least `LEAST`). The minutes taken.
pub fn give_day(c: &mut Company, driver: u32, date: &str) -> Result<f64, &'static str> {
    if dates::parse(date).is_none() || dates::between(&c.date, date) < 0 {
        return Err(GONE);
    }
    let e = c.employee(driver).ok_or(super::pro_ops::NOT_A_DRIVER)?;
    if !can_save(e) {
        return Err(ONLY_PERMANENT);
    }
    if e.absent(date) || !e.employed_on(date) {
        return Err(OFF_ALREADY);
    }
    if e.toil < LEAST {
        return Err(TOO_FEW);
    }
    let minutes = day_of(c, e).min(e.toil).max(LEAST).round();
    let e = c.staff.iter_mut().find(|e| e.id == driver).ok_or(super::pro_ops::NOT_A_DRIVER)?;
    e.toil = (e.toil - minutes).max(0.0);
    e.free_days.push(FreeDay { date: date.to_string(), why: FreeWhy::Toil, minutes });
    e.free_days.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(minutes)
}

/// Driver `driver`'s day of time off in lieu on `date` (not begun) taken back: its hours his again.
/// Whether there was one.
pub fn take_back(c: &mut Company, driver: u32, date: &str) -> bool {
    if dates::between(&c.date, date) < 0 {
        return false;
    }
    let Some(e) = c.staff.iter_mut().find(|e| e.id == driver) else { return false };
    let Some(k) = e.free_days.iter().position(|f| f.date == date && f.why == FreeWhy::Toil) else { return false };
    let f = e.free_days.remove(k);
    e.toil += f.minutes;
    true
}

/// The company's day for time off in lieu (`clock::on_minute`, with the wishes): too many saved
/// make a driver less content; with `ASKS_FROM` saved he asks for them now and then (his message
/// to the control room). Who asked, and his hours.
pub fn daily(c: &mut Company) -> Vec<(u32, String)> {
    let today = c.date.clone();
    let mut out = Vec::new();
    let ids: Vec<u32> = c.staff.iter().filter(|e| can_save(e) && e.toil > 0.0 && e.employed_on(&today)).map(|e| e.id).collect();
    for id in ids {
        let Some(e) = c.staff.iter_mut().find(|e| e.id == id) else { continue };
        if e.toil > TOO_MANY {
            e.satisfaction = (e.satisfaction - TOO_MANY_COST).clamp(0.0, 100.0);
        }
        if e.toil < ASKS_FROM || c.planning.ops.toil_asked.iter().any(|x| x.driver == id && dates::between(&x.date, &today) < ASK_DAYS) {
            continue;
        }
        let mut rng = super::rng::Rng::of(&[&c.id, &today, "pro-toil-ask"], id as i64);
        if !rng.chance(ASK_CHANCE) {
            continue;
        }
        let Some(e) = c.employee(id) else { continue };
        let (name, h) = (e.name.clone(), format!("{:.0}", e.toil / 60.0));
        let from = messages::From::Driver { line: String::new(), tour: String::new(), name };
        messages::send(c, from, messages::To::Control, messages::Priority::Info, ASKS, vec![("h".into(), h.clone())], false, None);
        c.planning.ops.toil_asked.retain(|x| x.driver != id);
        c.planning.ops.toil_asked.push(super::pro_rest::Extra { driver: id, date: today.clone() });
        out.push((id, h));
    }
    out
}

/// The days gone dropped (the free days of time off in lieu before the company's day: taken).
pub fn tidy(c: &mut Company) {
    let today = c.date.clone();
    for e in c.staff.iter_mut() {
        e.free_days.retain(|f| dates::between(&today, &f.date) >= 0);
    }
    let drivers: Vec<u32> = c.staff.iter().map(|e| e.id).collect();
    c.planning.ops.toil_asked.retain(|x| drivers.contains(&x.driver) && dates::between(&x.date, &today) < 2 * ASK_DAYS);
}

/// Every English text of this module.
pub fn texts() -> Vec<&'static str> {
    vec![ONLY_PERMANENT, TOO_FEW, OFF_ALREADY, GONE, ASKS, ASKED_FEED]
}

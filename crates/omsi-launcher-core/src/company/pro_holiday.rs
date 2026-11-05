//! RosterFlow Pro, round D (the player's, NexiconDK 5 November 2026: "Helligdagstillæg"): the
//! public holidays in the calendar and in the pay.
//!
//! - **Which days** (`name`, `is`): a company playing in Denmark (`econ_uk::country_of`) has the
//!   Danish holidays (`service::danish_public_holiday`: New Year's Day, Maundy Thursday, Good
//!   Friday, Easter Sunday and Monday, Ascension Day, Whit Sunday and Monday, Christmas Day and
//!   the Second Day of Christmas - Great Prayer Day until 2023); every other company the common
//!   German and Dutch ones (`service::common_public_holiday`: New Year's Day, Good Friday, Easter
//!   Monday, Labour Day, Ascension Day, Whit Monday and the two days of Christmas). Christmas
//!   Eve and New Year's Eve are no public holidays in either.
//! - **The pay** (`pay::supplement_rate_on`): a holiday is paid as a Sunday (`pay::SUNDAY`,
//!   35 %) all day - not on top of the evening's, the night's or the Saturday's: the highest
//!   counts, as before. The week's cost line shows the share of the holidays.
//! - **The drivers**: asked for an extra duty on a holiday a driver says yes less often
//!   (`bias`, ×0.75); one who works many holidays is less content - from the fourth worked in
//!   twelve months two points each (`after_day`, `MANY`, `COST`).

use super::model::{Company, Employee};
use super::{dates, econ_uk, pro_duties};
use crate::service;

/// The yes chance of an extra duty on a holiday, against a normal day.
pub const YES: f64 = 0.75;
/// Holidays worked in twelve months before working another costs satisfaction.
pub const MANY: usize = 3;
/// What each holiday worked beyond `MANY` costs of satisfaction.
pub const COST: f64 = 2.0;

pub const NEW_YEAR: &str = "New Year's Day";
pub const MAUNDY_THURSDAY: &str = "Maundy Thursday";
pub const GOOD_FRIDAY: &str = "Good Friday";
pub const EASTER_SUNDAY: &str = "Easter Sunday";
pub const EASTER_MONDAY: &str = "Easter Monday";
pub const PRAYER_DAY: &str = "Great Prayer Day";
pub const LABOUR_DAY: &str = "Labour Day";
pub const ASCENSION: &str = "Ascension Day";
pub const WHIT_SUNDAY: &str = "Whit Sunday";
pub const WHIT_MONDAY: &str = "Whit Monday";
pub const CHRISTMAS: &str = "Christmas Day";
pub const SECOND_CHRISTMAS: &str = "Second Day of Christmas";

/// Whether the company keeps the Danish holidays (it plays in Denmark).
fn danish(c: &Company) -> bool {
    econ_uk::country_of(c).key.eq_ignore_ascii_case("dk")
}

/// The name of the public holiday `date` is for the company (English, translated when shown):
/// None on any other day.
pub fn name(c: &Company, date: &str) -> Option<&'static str> {
    name_in(danish(c), date)
}

/// The holiday's name of `date` in the Danish set (`dk`) or the common one.
pub fn name_in(dk: bool, date: &str) -> Option<&'static str> {
    let day = dates::parse(date)?;
    let (y, m, d) = dates::civil_from_days(day);
    let code = y * 10000 + m as i32 * 100 + d as i32;
    let ok = if dk { service::danish_public_holiday(code) } else { service::common_public_holiday(code) };
    if !ok {
        return None;
    }
    match (m, d) {
        (1, 1) => return Some(NEW_YEAR),
        (5, 1) => return Some(LABOUR_DAY),
        (12, 25) => return Some(CHRISTMAS),
        (12, 26) => return Some(SECOND_CHRISTMAS),
        _ => {}
    }
    let (em, ed) = service::easter(y);
    let k = day - dates::days_from_civil(y, em, ed);
    Some(match k {
        -3 => MAUNDY_THURSDAY,
        -2 => GOOD_FRIDAY,
        0 => EASTER_SUNDAY,
        1 => EASTER_MONDAY,
        26 => PRAYER_DAY,
        39 => ASCENSION,
        49 => WHIT_SUNDAY,
        50 => WHIT_MONDAY,
        _ => return None,
    })
}

/// Whether `date` is a public holiday for the company.
pub fn is(c: &Company, date: &str) -> bool {
    name(c, date).is_some()
}

/// Whether `date` and the day after it are public holidays (a duty past midnight is paid by the
/// next day's): the pay's supplements (`pay::supplement_rate_on`). None without RosterFlow Pro.
pub fn days(c: &Company, date: &str) -> [bool; 2] {
    if !pro_duties::on(c) {
        return [false; 2];
    }
    [is(c, date), is(c, &dates::add(date, 1))]
}

/// What a holiday does to a driver's yes chance of an extra duty on `date`.
pub fn bias(c: &Company, date: &str) -> f64 {
    if pro_duties::on(c) && is(c, date) {
        YES
    } else {
        1.0
    }
}

/// The holidays driver `e` worked in the twelve months up to `date`.
pub fn worked(e: &Employee, date: &str) -> usize {
    e.holidays_worked.iter().filter(|d| (0..365).contains(&dates::between(d, date))).count()
}

/// The night after `date` (`day::close_day`; `pieces` the drivers' pieces of work: id, from,
/// to): on a holiday each driver who worked it has it counted; from the fourth in twelve months
/// it costs them `COST` of their satisfaction. The old ones are let go.
pub fn after_day(c: &mut Company, date: &str, pieces: &[(u32, i32, i32)]) {
    if !pro_duties::on(c) {
        return;
    }
    let holiday = is(c, date);
    for e in c.staff.iter_mut() {
        if holiday && pieces.iter().any(|x| x.0 == e.id && x.2 > x.1) && !e.holidays_worked.iter().any(|d| d == date) {
            e.holidays_worked.push(date.to_string());
            if worked(e, date) > MANY {
                e.satisfaction = (e.satisfaction - COST).max(0.0);
            }
        }
        e.holidays_worked.retain(|d| dates::between(d, date) < 365);
    }
}

/// Every English text this module hands the pages.
pub fn texts() -> Vec<&'static str> {
    vec![NEW_YEAR, MAUNDY_THURSDAY, GOOD_FRIDAY, EASTER_SUNDAY, EASTER_MONDAY, PRAYER_DAY, LABOUR_DAY, ASCENSION, WHIT_SUNDAY, WHIT_MONDAY, CHRISTMAS, SECOND_CHRISTMAS]
}

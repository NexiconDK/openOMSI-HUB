//! RosterFlow Pro, round C (the player's, NexiconDK 5 November 2026): illness and holidays set in
//! the Turnus tab are the staff card's own - one record, not two.
//!
//! - **Ill**: a day set "Ill" in a driver's next weeks is his illness of the staff card: from today
//!   on (or the day after an illness that runs until the day before) it lengthens `sick_until`;
//!   a day further ahead is a day he is known to be ill (`Employee::sick_planned`), which becomes
//!   his illness of the day when it comes (`tidy`).
//! - **On holiday**: a day set "On holiday" is an approved holiday of his (`holidays::Request`,
//!   by the planner): next to one of his, not begun, it lengthens it; else it is one of its own.
//!   Its working days are taken from what is left of his year's when it begins
//!   (`holidays::night`), as any holiday's; with too few left it is refused.
//! - **Back as in the rota**: the day taken out of his illness or his holiday (a holiday begun
//!   gives the day back to his year's).
//! - The other way round needs nothing: the rota's day (`pro_turnus::decided`) is off when he is
//!   ill or on holiday, whatever set it - the staff card, a sick call, a holiday asked for.
//! - **Older files**: an exception "Ill" or "On holiday" of the Turnus tab is moved into the
//!   records when the company is opened (`migrate`).

use super::holidays::{self, By, Reason, Request, State};
use super::model::{Company, Employee};
use super::dates;

/// The texts (English: keys of the translations).
pub const GONE: &str = "That day has gone.";
pub const NO_DAYS: &str = "They have not enough holiday days left for it.";
pub use super::pro_ops::NOT_A_DRIVER;

/// The last day of the illness driver `e` has on `date` (the days known ahead counted on).
pub fn ill_until(e: &Employee, date: &str) -> String {
    if let Some(u) = e.sick_until.as_deref().filter(|u| dates::between(date, u) >= 0) {
        // (and the days ahead that follow on from it)
        return follow_on(e, u);
    }
    follow_on(e, date)
}

/// From `last`, the days of `sick_planned` that follow on, the last of them.
fn follow_on(e: &Employee, last: &str) -> String {
    let mut last = last.to_string();
    loop {
        let next = dates::add(&last, 1);
        if e.sick_planned.contains(&next) {
            last = next;
        } else {
            return last;
        }
    }
}

/// The spells of illness and holiday of driver `e` from `today` on, in order: (`"sick"` or
/// `"holiday"`, its first day, its last day) - the staff card's lines ("Syg 6.–9. Mar").
pub fn spells(e: &Employee, today: &str) -> Vec<(&'static str, String, String)> {
    let mut out: Vec<(&'static str, String, String)> = Vec::new();
    // ill: now, and the days known ahead (in runs)
    let mut days: Vec<String> = e.sick_planned.iter().filter(|d| d.as_str() >= today).cloned().collect();
    if let Some(u) = e.sick_until.as_deref().filter(|u| *u >= today) {
        days.extend(holidays::days_of(today, u));
    }
    days.sort();
    days.dedup();
    for d in days {
        match out.last_mut() {
            Some(last) if last.0 == "sick" && dates::add(&last.2, 1) == d => last.2 = d,
            _ => out.push(("sick", d.clone(), d)),
        }
    }
    // on holiday: the holiday begun before the requests, the approved ones
    if let Some(u) = e.holiday_until.as_deref().filter(|u| *u >= today) {
        out.push(("holiday", today.to_string(), u.to_string()));
    }
    for r in e.leave.requests.iter().filter(|r| r.state == State::Approved && r.until.as_str() >= today) {
        out.push(("holiday", r.from.clone().max(today.to_string()), r.until.clone()));
    }
    out.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(b.0)));
    out
}

/// Driver `id` ill on `date` (today or ahead).
pub fn set_ill(c: &mut Company, id: u32, date: &str) -> Result<(), &'static str> {
    let today = c.date.clone();
    if date < today.as_str() {
        return Err(GONE);
    }
    clear_holiday(c, id, date);
    let e = c.staff.iter_mut().find(|e| e.id == id).ok_or(NOT_A_DRIVER)?;
    // (his illness of now runs until the day before - or it is today: it goes on)
    let now = e.sick_until.clone().filter(|u| u.as_str() >= dates::add(&today, -1).as_str());
    match now {
        Some(u) if dates::add(&u, 1).as_str() >= date => {
            if date > u.as_str() {
                e.sick_until = Some(date.to_string());
            }
        }
        _ if date == today => e.sick_until = Some(date.to_string()),
        _ => {
            if !e.sick_planned.iter().any(|d| d == date) {
                e.sick_planned.push(date.to_string());
                e.sick_planned.sort();
            }
        }
    }
    absorb(e);
    Ok(())
}

/// The days known ahead that now follow on from his illness of now taken into it.
fn absorb(e: &mut Employee) {
    if let Some(u) = e.sick_until.clone() {
        let last = follow_on(e, &u);
        if last != u {
            e.sick_planned.retain(|d| d.as_str() > last.as_str());
            e.sick_until = Some(last);
        }
    }
}

/// Driver `id` on holiday on `date` (today or ahead): next to an approved holiday of his not begun,
/// it lengthens it; else a holiday of its own, approved by the planner. Too few days left: refused.
pub fn set_holiday(c: &mut Company, id: u32, date: &str) -> Result<(), &'static str> {
    let today = c.date.clone();
    if date < today.as_str() {
        return Err(GONE);
    }
    let e = c.employee(id).ok_or(NOT_A_DRIVER)?;
    if e.on_holiday(date).is_some() {
        return Ok(());
    }
    if holidays::free_days(e, None) < 1 {
        return Err(NO_DAYS);
    }
    clear_ill(c, id, date);
    let no = c.staff_rules.holidays.last + 1;
    let e = c.staff.iter_mut().find(|e| e.id == id).ok_or(NOT_A_DRIVER)?;
    let before = dates::add(date, -1);
    let after = dates::add(date, 1);
    let next = |r: &Request| r.state == State::Approved && !r.begun && r.by == By::Planner;
    if let Some(r) = e.leave.requests.iter_mut().find(|r| next(r) && r.until == before) {
        r.until = date.to_string();
    } else if let Some(r) = e.leave.requests.iter_mut().find(|r| next(r) && r.from == after) {
        r.from = date.to_string();
    } else {
        e.leave.requests.push(Request {
            no,
            from: date.to_string(),
            until: date.to_string(),
            reason: Reason::Break,
            asked: today.clone(),
            due: today.clone(),
            state: State::Approved,
            answered: Some(today.clone()),
            by: By::Planner,
            begun: false,
        });
        c.staff_rules.holidays.last = no;
        return Ok(());
    }
    // (two of his now meet: one holiday)
    join(e);
    Ok(())
}

/// His approved holidays by the planner, not begun, that now meet joined into one.
fn join(e: &mut Employee) {
    let mut v: Vec<usize> = (0..e.leave.requests.len()).filter(|&k| { let r = &e.leave.requests[k]; r.state == State::Approved && !r.begun && r.by == By::Planner }).collect();
    v.sort_by(|a, b| e.leave.requests[*a].from.cmp(&e.leave.requests[*b].from));
    let mut gone: Vec<usize> = Vec::new();
    for w in v.windows(2) {
        let (a, b) = (w[0], w[1]);
        if gone.contains(&a) {
            continue;
        }
        if dates::add(&e.leave.requests[a].until, 1) >= e.leave.requests[b].from {
            let until = e.leave.requests[a].until.clone().max(e.leave.requests[b].until.clone());
            e.leave.requests[a].until = until;
            gone.push(b);
        }
    }
    gone.sort_unstable();
    for k in gone.into_iter().rev() {
        e.leave.requests.remove(k);
    }
}

/// `date` taken out of driver `id`'s illness. Whether it was in it.
pub fn clear_ill(c: &mut Company, id: u32, date: &str) -> bool {
    let today = c.date.clone();
    let Some(e) = c.staff.iter_mut().find(|e| e.id == id) else { return false };
    let mut was = false;
    if let Some(k) = e.sick_planned.iter().position(|d| d == date) {
        e.sick_planned.remove(k);
        was = true;
    }
    if let Some(u) = e.sick_until.clone().filter(|u| dates::between(date, u) >= 0 && date >= today.as_str()) {
        // (ill until the day before; the days after it still known ahead)
        e.sick_until = (date > today.as_str()).then(|| dates::add(date, -1));
        for d in holidays::days_of(&dates::add(date, 1), &u) {
            if !e.sick_planned.contains(&d) {
                e.sick_planned.push(d);
            }
        }
        e.sick_planned.sort();
        // (well again today: his illness of now ends)
        if e.sick_until.is_none() {
            let first = e.sick_planned.first().cloned();
            if first.as_deref() == Some(today.as_str()) {
                e.sick_until = first;
                absorb(e);
            }
        }
        was = true;
    }
    was
}

/// `date` taken out of driver `id`'s holidays (one begun gives the day back to his year's). Whether
/// it was in one.
pub fn clear_holiday(c: &mut Company, id: u32, date: &str) -> bool {
    let next = c.staff_rules.holidays.last + 1;
    let Some(e) = c.staff.iter_mut().find(|e| e.id == id) else { return false };
    let Some(k) = e.leave.requests.iter().position(|r| r.state == State::Approved && r.covers(date)) else {
        // (a holiday begun before the requests: it ends the day before)
        if e.holiday_until.as_deref().is_some_and(|u| dates::between(date, u) >= 0) {
            e.holiday_until = Some(dates::add(date, -1)).filter(|d| dates::between(&c.date, d) >= 0);
            return true;
        }
        return false;
    };
    let r = e.leave.requests[k].clone();
    let before = r.working_days();
    let mut split: Option<Request> = None;
    if r.from == date && r.until == date {
        e.leave.requests.remove(k);
    } else if r.from == date {
        e.leave.requests[k].from = dates::add(date, 1);
    } else if r.until == date {
        e.leave.requests[k].until = dates::add(date, -1);
    } else {
        e.leave.requests[k].until = dates::add(date, -1);
        split = Some(Request { no: next, from: dates::add(date, 1), ..r.clone() });
    }
    let after = e.leave.requests.iter().filter(|x| x.no == r.no).map(Request::working_days).sum::<u32>() + split.as_ref().map_or(0, Request::working_days);
    if r.begun && after < before {
        e.holiday_left += before - after;
    }
    if let Some(s) = split {
        e.leave.requests.push(s);
        c.staff_rules.holidays.last = next;
    }
    true
}

/// The days known ahead that have come become the illness of the day; those gone fall away (the
/// night's, `pro_turnus::tidy`).
pub fn tidy(c: &mut Company) {
    let today = c.date.clone();
    for e in c.staff.iter_mut() {
        if e.sick_planned.is_empty() {
            continue;
        }
        if e.sick_planned.iter().any(|d| *d == today) {
            let u = e.sick_until.clone().filter(|u| u.as_str() >= today.as_str()).unwrap_or_else(|| today.clone());
            e.sick_until = Some(u);
        }
        e.sick_planned.retain(|d| d.as_str() >= today.as_str());
        absorb(e);
        e.sick_planned.retain(|d| d.as_str() > today.as_str() || e.sick_until.as_deref().is_none_or(|u| d.as_str() > u));
    }
}

/// An older file's exceptions "Ill" and "On holiday" of the Turnus tab moved into the records
/// (`store::migrate`): the day of each the driver's illness or holiday (a holiday with too few days
/// left stays a day off).
pub fn migrate(c: &mut Company) {
    let today = c.date.clone();
    let olds: Vec<(u32, String, String)> = c.planning.rotas.exceptions.iter().filter(|x| x.note == "sick" || x.note == "holiday").map(|x| (x.driver, x.date.clone(), x.note.clone())).collect();
    if olds.is_empty() {
        return;
    }
    for (id, date, note) in olds {
        if date.as_str() < today.as_str() {
            continue;
        }
        let moved = if note == "sick" { set_ill(c, id, &date).is_ok() } else { set_holiday(c, id, &date).is_ok() };
        let ex = &mut c.planning.rotas.exceptions;
        if moved {
            ex.retain(|x| !(x.driver == id && x.date == date));
        } else if let Some(x) = ex.iter_mut().find(|x| x.driver == id && x.date == date) {
            x.note = String::new();
        }
    }
    c.planning.rotas.exceptions.retain(|x| x.note != "sick" && x.note != "holiday");
}

/// Every English text this module writes, for the translations.
pub fn texts() -> Vec<&'static str> {
    vec![GONE, NO_DAYS]
}

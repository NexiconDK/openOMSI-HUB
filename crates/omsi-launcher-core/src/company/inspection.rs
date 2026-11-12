//! (NexiconDK, ticket inspection round S, 9 November 2026) The ticket inspectors' day, as it was:
//! their patrols (`staff::patrols` - in teams of two, a lone one where the number is odd or for
//! the hours one of them stays on) check the passengers of their line's buses and catch those
//! without a ticket - the trip's share of dodgers (`dodgers`), less the few who get off before they
//! are reached (`ESCAPE`). Each caught pays the country's fine (`fine`: 750 kr. in Denmark, 60 € in
//! Germany, …); about two thirds of it is paid (`PAID`), the rest is written off. The paid fines
//! are booked as their own income (`BookingKind::InspectionFines`), a line a day.
//!
//! (round T, 10 November 2026: boardings) A team does not ride a trip from end to end: it boards a
//! bus at a stop, rides `STRETCH` stops (8 to 15 minutes or so, by the trip's stops and times) and
//! checks who is aboard on the way - `ON_BOARD` of a trip's riders at once, more over a longer
//! stretch, as many as the team gets to (`CHECK_RATE` an inspector a minute, a lone one a little
//! faster; at most `PAIR_CHECKS` a boarding for a pair, `LONE_CHECKS` alone, more with the
//! researched working method) -, gets off, and after `TRANSFER` takes the next bus of the line
//! passing that stop, the earliest either way, the other direction when they come together. A
//! storaktion stands at its stop instead (`inspection_plan`).
//!
//! What the day saw is kept (`Book`): its checks for `KEPT_DAYS`, the lines' passengers and their
//! share of dodgers (what the deterrence of the days after reads, `dodgers::deterrence`, and the
//! page's trend), and every month's totals for `KEPT_MONTHS`. Drawn the same for the same company
//! on the same day (`Rng`). The trips a player drove are the game's: the inspectors check the
//! modelled ones (round U brings them into the game).

use super::currency::Currency;
use super::dates;
use super::inbox::{self, Kind, Message};
use super::model::{BookingKind, Cents, Company};
use super::rng::Rng;
use serde::{Deserialize, Serialize};

/// The days whose checks are kept, and the months whose totals are.
pub const KEPT_DAYS: usize = 60;
pub const KEPT_MONTHS: usize = 36;
/// The share of the fines written that is paid.
pub const PAID: f64 = 0.65;
/// The dodgers on board who get off before the inspectors reach them.
pub const ESCAPE: f64 = 0.15;
/// The share of a trip's riders aboard at once (the inspectors check those).
pub const ON_BOARD: (f64, f64) = (0.45, 0.7);
/// The most passengers a pair checks on a boarding, and a lone inspector.
pub const PAIR_CHECKS: f64 = 45.0;
pub const LONE_CHECKS: f64 = 22.0;
/// The stops a boarding rides, and the minutes at the stop before the next bus is taken.
pub const STRETCH: (i64, i64) = (3, 8);
pub const TRANSFER: (i64, i64) = (2, 5);
/// Passengers an inspector checks a minute aboard; a lone one, without a colleague to share the
/// bus with, a little faster.
pub const CHECK_RATE: f64 = 0.45;
pub const LONE_RATE: f64 = 1.4;

/// What the inspectors saw, kept.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Book {
    #[serde(default)]
    pub days: Vec<Day>,
    #[serde(default)]
    pub months: Vec<MonthTotal>,
    /// (round U) What the inspectors did on the trips the players drove, as their games reported
    /// it (`inspection_game::book_game`), by date: the day's close takes them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub game: Vec<(String, Check)>,
}

/// A day closed.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Day {
    pub date: String,
    /// Every line that carried passengers: its riders and their dodgers.
    #[serde(default)]
    pub lines: Vec<LineDay>,
    /// The trips the inspectors checked.
    #[serde(default)]
    pub checks: Vec<Check>,
    /// The fines written, and what of them is paid.
    #[serde(default)]
    pub written: Cents,
    #[serde(default)]
    pub paid: Cents,
    /// (round T) The inspection duties the boss planned for the day, and who worked them (none:
    /// the automatic patrols).
    #[serde(default)]
    pub planned: Vec<PlannedRun>,
}

/// An inspection duty of the day as it was worked (`inspection_plan::Assigned`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct PlannedRun {
    pub duty: u32,
    #[serde(default)]
    pub kind: super::inspection_plan::Kind,
    /// The lines' names (a storaktion's: those through its stop), and its stop.
    #[serde(default)]
    pub lines: Vec<String>,
    #[serde(default)]
    pub stop: String,
    pub from: i32,
    pub to: i32,
    /// Who worked it, their names then, how many it asked for and lacked.
    #[serde(default)]
    pub ids: Vec<u32>,
    #[serde(default)]
    pub names: Vec<String>,
    #[serde(default)]
    pub wants: u32,
    #[serde(default)]
    pub missing: u32,
}

/// A line's day.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct LineDay {
    /// Its name (`CompanyLine::name`).
    pub line: String,
    /// The share of the day the inspectors checked it (`staff::inspected`, 0 to 1).
    #[serde(default)]
    pub checked: f64,
    /// Its passengers (the modelled trips), and how many of them rode without a ticket.
    #[serde(default)]
    pub passengers: u32,
    #[serde(default)]
    pub dodgers: f64,
    /// Its passengers at night (22:00 to 05:00), in the evening (20:00 to 22:00) and on a crowded
    /// bus (why its share is what it is: `dodgers::of_line`).
    #[serde(default)]
    pub night: f64,
    #[serde(default)]
    pub evening: f64,
    #[serde(default)]
    pub full: f64,
}

impl LineDay {
    /// The share of its passengers without a ticket.
    pub fn share(&self) -> f64 {
        if self.passengers == 0 {
            0.0
        } else {
            self.dodgers / self.passengers as f64
        }
    }
}

/// A trip the inspectors checked.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Check {
    /// The line's name, and the number its displays show.
    pub line: String,
    #[serde(default)]
    pub number: String,
    pub dep: i32,
    pub arr: i32,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub to: String,
    /// The team (employee ids) and their names then.
    #[serde(default)]
    pub team: Vec<u32>,
    #[serde(default)]
    pub names: Vec<String>,
    pub checked: u32,
    pub caught: u32,
    /// The fine of each (written).
    #[serde(default)]
    pub fine: Cents,
    /// (round T) The inspection duty of the plan (0: the automatic patrols); a storaktion's stop
    /// and the minute the bus passed it.
    #[serde(default)]
    pub duty: u32,
    #[serde(default)]
    pub stop: String,
    #[serde(default)]
    pub at: i32,
    /// (round T) A boarding: when the team got off (0: a check of before - the whole trip), the
    /// stops it got on and off at (0 its first) and the trip's stops.
    #[serde(default)]
    pub off: i32,
    #[serde(default)]
    pub board: u32,
    #[serde(default)]
    pub alight: u32,
    #[serde(default)]
    pub stops: u32,
    /// (round U) A check on a trip a player drove, as the game reported it: the driver's name
    /// (empty: the model's).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub game: String,
}

/// A month's checks added up.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct MonthTotal {
    /// `YYYY-MM`.
    pub month: String,
    #[serde(default)]
    pub checks: u32,
    #[serde(default)]
    pub checked: u32,
    #[serde(default)]
    pub caught: u32,
    #[serde(default)]
    pub written: Cents,
    #[serde(default)]
    pub paid: Cents,
}

/// A modelled trip of the day, as the day's close saw it (`day::close_day_with`).
#[derive(Clone, Debug, Default)]
pub struct Seen {
    pub line: String,
    pub number: String,
    pub dep: i32,
    pub arr: i32,
    pub from: String,
    pub to: String,
    pub pax: u32,
    /// Its share of dodgers (`dodgers::share`).
    pub share: f64,
    pub crowded: bool,
    /// (round T) Its stops (0: not known - about one every two minutes).
    pub stops: u32,
}

/// A trip's stops: its own, else about one every two minutes.
fn stops_of(s: &Seen) -> u32 {
    if s.stops >= 2 {
        s.stops
    } else {
        ((s.arr - s.dep) / 2).clamp(4, 40) as u32
    }
}

/// When a trip is at its stop `i` (0 its first), its minutes shared evenly between its stops.
fn at_stop(s: &Seen, i: u32) -> i32 {
    let n = stops_of(s);
    s.dep + ((s.arr - s.dep) as f64 * i.min(n - 1) as f64 / (n - 1) as f64).round() as i32
}

/// The next bus of a line's `trips` a team standing at stop `at` of trip `cur` (its direction)
/// boards from minute `ready` on: the earliest passing that stop either way with stops left to
/// ride - a trip of the same direction there at the same stop, one of the other direction (from
/// where `cur` goes) at the same place counted from its end -, the other direction when they come
/// together; None: none passes before `until`.
fn next_bus<'a>(trips: &[&'a Seen], cur: &Seen, at: u32, ready: i32, until: i32) -> Option<(&'a Seen, u32, i32)> {
    let n = stops_of(cur);
    let mut best: Option<(i32, u8, &'a Seen, u32)> = None;
    for s in trips {
        let m = stops_of(s);
        let (i, same) = if s.from.eq_ignore_ascii_case(&cur.from) && m == n {
            (at, 0u8)
        } else if s.from.eq_ignore_ascii_case(&cur.to) && m == n {
            (n - 1 - at.min(n - 1), 1u8)
        } else {
            continue;
        };
        if i + 1 >= m {
            continue;
        }
        let when = at_stop(s, i);
        if when < ready || when >= until {
            continue;
        }
        // (the other direction first when they come together: `same` 0 sorts before 1)
        let key = (when, if same == 1 { 0 } else { 1 });
        if best.as_ref().is_none_or(|b| (key.0, key.1) < (b.0, b.1)) {
            best = Some((key.0, key.1, s, i));
        }
    }
    best.map(|b| (b.2, b.3, b.0))
}

/// The fine a passenger without a ticket pays, by the company's country (else its currency):
/// Denmark's kontrolafgift 750 kr., Germany's erhöhtes Beförderungsentgelt 60 €, Vienna's 105 €,
/// a London penalty fare £80, the Netherlands' 50 €, Sweden's 1,500 kr., Norway's 1,150 kr.,
/// Finland's 80 €, Switzerland's CHF 100, Poland's 266 zł, Czechia's 1,000 Kč; anywhere else 60 €
/// (in euro cents, the game's money).
pub fn fine(c: &Company) -> Cents {
    use Currency::*;
    let by_country = match c.country.trim().to_ascii_lowercase().as_str() {
        "dk" => Some((Dkk, 750.0)),
        "de" => Some((Eur, 60.0)),
        "at" => Some((Eur, 105.0)),
        "gb" => Some((Gbp, 80.0)),
        "nl" => Some((Eur, 50.0)),
        "se" => Some((Sek, 1500.0)),
        "no" => Some((Nok, 1150.0)),
        "fi" => Some((Eur, 80.0)),
        "ch" => Some((Chf, 100.0)),
        "pl" => Some((Pln, 266.0)),
        "cz" => Some((Czk, 1000.0)),
        _ => None,
    };
    let (currency, units) = by_country.unwrap_or(match c.currency {
        Dkk => (Dkk, 750.0),
        Gbp => (Gbp, 80.0),
        Sek => (Sek, 1500.0),
        Nok => (Nok, 1150.0),
        Chf => (Chf, 100.0),
        Pln => (Pln, 266.0),
        Czk => (Czk, 1000.0),
        _ => (Eur, 60.0),
    });
    currency.cents(units)
}

/// The day's checks of the inspectors at work on `date` over the modelled trips `seen` (of the
/// lines they check), kept with the lines' day `lines`; the paid fines booked, and one message
/// when they caught anyone. Returns the day kept.
pub fn close(c: &mut Company, date: &str, mut lines: Vec<LineDay>, seen: &[Seen]) -> Day {
    let patrols = super::staff::patrols(c, date);
    let plan = super::inspection_plan::assigned(c, date);
    for l in lines.iter_mut() {
        l.checked = super::staff::inspected(c, date).iter().find(|x| x.0.eq_ignore_ascii_case(&l.line)).map_or(0.0, |x| x.1);
    }
    let each = fine(c);
    let factor = super::research::inspection_factor(c);
    let day_n = dates::parse(date).unwrap_or(0);
    let mut checks: Vec<Check> = Vec::new();
    for p in &patrols {
        let ids: Vec<String> = p.ids.iter().map(|i| i.to_string()).collect();
        let mut parts: Vec<&str> = vec![&c.id, "inspection", &p.line];
        parts.extend(ids.iter().map(String::as_str));
        let mut rng = Rng::of(&parts, day_n * 1440 + p.from as i64);
        let names: Vec<String> = p.ids.iter().map(|id| c.employee(*id).map(|e| e.name.clone()).unwrap_or_default()).collect();
        // (round T: a storaktion boards every bus of the line passing its stop within its hours
        // and checks everyone aboard, as many as its inspectors get to)
        if let Some(stop) = &p.stop {
            let Some(duty) = plan.as_ref().and_then(|a| a.iter().find(|x| x.duty.id == p.duty)).map(|x| &x.duty) else { continue };
            let most = super::inspection_plan::ACTION_EACH * p.ids.len() as f64 * factor;
            let mut trips: Vec<(i32, &Seen)> = seen.iter().filter(|s| s.line.eq_ignore_ascii_case(&p.line)).filter_map(|s| super::inspection_plan::passes_at(duty, &s.line, &s.from, s.dep, s.arr).map(|at| (at, s))).filter(|(at, _)| *at >= p.from && *at < p.to).collect();
            trips.sort_by_key(|(at, s)| (*at, s.dep));
            for (at, s) in trips {
                let aboard = s.pax as f64 * rng.range(ON_BOARD.0, ON_BOARD.1);
                let checked = aboard.min(most).round() as u32;
                let caught = (0..checked).filter(|_| rng.chance(s.share * (1.0 - super::inspection_plan::ACTION_ESCAPE))).count() as u32;
                checks.push(Check { line: s.line.clone(), number: s.number.clone(), dep: s.dep, arr: s.arr, from: s.from.clone(), to: s.to.clone(), team: p.ids.clone(), names: names.clone(), checked, caught, fine: each, duty: p.duty, stop: stop.clone(), at, off: at, board: 0, alight: 0, stops: s.stops, game: String::new() });
            }
            continue;
        }
        let most = if p.ids.len() >= 2 { PAIR_CHECKS } else { LONE_CHECKS } * factor;
        let rate = CHECK_RATE * p.ids.len() as f64 * if p.ids.len() >= 2 { 1.0 } else { LONE_RATE } * factor;
        let mut trips: Vec<&Seen> = seen.iter().filter(|s| s.line.eq_ignore_ascii_case(&p.line)).collect();
        trips.sort_by(|a, b| (a.dep, a.arr, &a.from).cmp(&(b.dep, b.arr, &b.from)));
        // (round T: boardings - the first bus leaving its first stop in their hours, then a few
        // stops at a time, the next bus where they got off)
        let mut cur: Option<(&Seen, u32, i32)> = trips.iter().find(|s| s.dep >= p.from && s.dep < p.to && stops_of(s) >= 2).map(|s| (*s, 0, s.dep));
        while let Some((s, board, on)) = cur {
            let n = stops_of(s);
            let alight = (board + rng.int(STRETCH.0, STRETCH.1) as u32).min(n - 1);
            let off = at_stop(s, alight).max(on + 1);
            let minutes = (off - on) as f64;
            // (who is aboard over the stretch: those aboard at once, and those getting on on the way)
            let frac = (alight - board) as f64 / (n - 1).max(1) as f64;
            let aboard = (s.pax as f64 * rng.range(ON_BOARD.0, ON_BOARD.1) * (1.0 + frac)).min(s.pax as f64);
            let checked = aboard.min(most).min(rate * minutes).round() as u32;
            let caught = (0..checked).filter(|_| rng.chance(s.share * (1.0 - ESCAPE))).count() as u32;
            checks.push(Check { line: s.line.clone(), number: s.number.clone(), dep: s.dep, arr: s.arr, from: s.from.clone(), to: s.to.clone(), team: p.ids.clone(), names: names.clone(), checked, caught, fine: each, duty: p.duty, stop: String::new(), at: on, off, board, alight, stops: n, game: String::new() });
            let ready = off + rng.int(TRANSFER.0, TRANSFER.1) as i32;
            cur = next_bus(&trips, s, alight, ready, p.to).or_else(|| {
                // (no bus passes there: the next one from either end)
                trips.iter().find(|x| x.dep >= ready && x.dep < p.to && stops_of(x) >= 2).map(|x| (*x, 0, x.dep))
            });
        }
    }
    // (round U: the players' trips are checked in their games - what the games reported; the model
    // does not check those trips again)
    let game = super::inspection_game::of_day(c, date);
    checks.retain(|k| !game.iter().any(|g| (g.line.eq_ignore_ascii_case(&k.line) || g.number.eq_ignore_ascii_case(&k.number)) && (k.dep - g.dep).abs() <= 1));
    checks.extend(game);
    c.inspection.game.retain(|(d, _)| d != date);
    checks.sort_by(|a, b| a.at.cmp(&b.at).then(a.number.cmp(&b.number)));
    // (the paid fines, a line a booking)
    let mut written: Cents = 0;
    let mut paid: Cents = 0;
    let mut by_line: Vec<(String, u32, Cents)> = Vec::new();
    for k in &checks {
        let w = k.caught as Cents * k.fine;
        written += w;
        match by_line.iter_mut().find(|x| x.0 == k.number) {
            Some(x) => {
                x.1 += k.caught;
                x.2 += w;
            }
            None => by_line.push((k.number.clone(), k.caught, w)),
        }
    }
    for (number, n, w) in by_line.into_iter().filter(|x| x.1 > 0) {
        let p = (w as f64 * PAID).round() as Cents;
        paid += p;
        c.book(BookingKind::InspectionFines, p, format!("Line {number}: {n} fines"), false);
    }
    // (round T: what the boss planned for the day, and who worked it)
    let planned: Vec<PlannedRun> = plan
        .unwrap_or_default()
        .into_iter()
        .map(|x| {
            let lines = if x.duty.kind == super::inspection_plan::Kind::Action { x.duty.action_lines() } else { x.duty.lines.clone() };
            PlannedRun { duty: x.duty.id, kind: x.duty.kind, lines, stop: x.duty.stop.clone(), from: x.duty.from, to: x.duty.to, names: x.ids.iter().map(|id| c.employee(*id).map(|e| e.name.clone()).unwrap_or_default()).collect(), ids: x.ids, wants: x.duty.wants() as u32, missing: x.missing }
        })
        .collect();
    super::inspection_plan::prune(c, date);
    super::inspection_patrol::prune(c, date);
    let day = Day { date: date.to_string(), lines, checks, written, paid, planned };
    // (the month's totals)
    let month = dates::month_of(date);
    let checked: u32 = day.checks.iter().map(|k| k.checked).sum();
    let caught: u32 = day.checks.iter().map(|k| k.caught).sum();
    if !day.checks.is_empty() {
        let m = match c.inspection.months.iter().position(|m| m.month == month) {
            Some(i) => &mut c.inspection.months[i],
            None => {
                c.inspection.months.push(MonthTotal { month: month.clone(), ..Default::default() });
                c.inspection.months.last_mut().unwrap()
            }
        };
        m.checks += day.checks.len() as u32;
        m.checked += checked;
        m.caught += caught;
        m.written += written;
        m.paid += paid;
        if c.inspection.months.len() > KEPT_MONTHS {
            let extra = c.inspection.months.len() - KEPT_MONTHS;
            c.inspection.months.drain(..extra);
        }
    }
    // (one message a day, replacing yesterday's unread one)
    if caught > 0 {
        let mut numbers: Vec<String> = Vec::new();
        for k in &day.checks {
            if !numbers.contains(&k.number) {
                numbers.push(k.number.clone());
            }
        }
        inbox::post(
            c,
            Message::new(Kind::Staff, "Ticket inspection: %{n} without a ticket", "The ticket inspectors checked %{checked} passengers on line %{lines} and caught %{n} without a ticket: %{amount} in fines written, about two thirds of which get paid. The Ticket inspection tab of the Staff page shows each check.")
                .arg("n", caught.to_string())
                .arg("checked", checked.to_string())
                .arg("lines", numbers.join(", "))
                .arg("amount", written.to_string())
                .key("inspection")
                .dated(date),
        );
    }
    c.inspection.days.retain(|d| d.date != date);
    c.inspection.days.push(day.clone());
    c.inspection.days.sort_by(|a, b| a.date.cmp(&b.date));
    if c.inspection.days.len() > KEPT_DAYS {
        let extra = c.inspection.days.len() - KEPT_DAYS;
        c.inspection.days.drain(..extra);
    }
    day
}

/// The checks of the last `days` days closed up to `date` (`date` itself included) added up:
/// (checks, passengers checked, caught, written, paid).
pub fn totals(c: &Company, date: &str, days: i64) -> (u32, u32, u32, Cents, Cents) {
    let from = dates::add(date, -(days - 1));
    c.inspection.days.iter().filter(|d| d.date.as_str() >= from.as_str() && d.date.as_str() <= date).fold((0, 0, 0, 0, 0), |t, d| {
        (t.0 + d.checks.len() as u32, t.1 + d.checks.iter().map(|k| k.checked).sum::<u32>(), t.2 + d.checks.iter().map(|k| k.caught).sum::<u32>(), t.3 + d.written, t.4 + d.paid)
    })
}

#[cfg(test)]
mod tests {
    use super::super::model::{CompanyLine, GameMode, Role};
    use super::super::research::keys;
    use super::super::{found, staff, Founding};
    use super::*;

    fn company(country: &str) -> Company {
        let mut c = found(&Founding { name: "Kontrol".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), ..Default::default() }, "Luc");
        c.country = country.into();
        c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), km: 300.0, service_from: Some(0), ..Default::default() });
        c.lines.push(CompanyLine { name: "Linie7".into(), number: "7".into(), km: 100.0, service_from: Some(0), ..Default::default() });
        c
    }

    /// Hire `n` inspectors (another week's market when this one runs out); the first day all of
    /// them are at work.
    fn with_inspectors(c: &mut Company, n: usize) -> String {
        c.research.done.push(keys::TICKET_INSPECTORS.to_string());
        let (id0, mut round) = (c.id.clone(), 0);
        let mut hired = 0;
        while hired < n {
            for a in staff::applicants_of(c, Role::Inspector) {
                if hired < n {
                    staff::hire(c, &a).unwrap();
                    hired += 1;
                }
            }
            round += 1;
            c.id = format!("{id0}{round}");
            c.taken = Default::default();
        }
        c.id = id0;
        for e in c.staff.iter_mut() {
            (e.sick_until, e.holiday_until) = (None, None);
        }
        let ids: Vec<u32> = c.staff.iter().filter(|e| e.role == Role::Inspector).map(|e| e.id).collect();
        (0..14).map(|k| dates::add(&c.date, k)).find(|d| ids.iter().all(|id| staff::at_work(c.employee(*id).unwrap(), d))).expect("a day all of them work")
    }

    /// A day's trips of a line: every 20 minutes from 05:00 to 23:40, 40 riders each, half an hour long.
    fn trips(line: &str, number: &str, share: f64) -> Vec<Seen> {
        (0..57).map(|k| 5 * 60 + 20 * k).map(|dep| Seen { line: line.into(), number: number.into(), dep, arr: dep + 30, from: "Bahnhof".into(), to: "Markt".into(), pax: 40, share, crowded: false, stops: 0 }).collect()
    }

    #[test]
    fn fines_follow_the_country_and_else_the_currency() {
        assert_eq!(fine(&company("dk")), Currency::Dkk.cents(750.0));
        assert_eq!(fine(&company("de")), 60_00);
        assert_eq!(fine(&company("at")), 105_00);
        assert_eq!(fine(&company("gb")), Currency::Gbp.cents(80.0));
        assert_eq!(fine(&company("it")), 60_00);
        let mut c = company("intl");
        c.currency = Currency::Dkk;
        assert_eq!(fine(&c), Currency::Dkk.cents(750.0));
        // (750 kr. is about 100 €)
        assert!((10_000..10_100).contains(&fine(&company("dk"))));
    }

    #[test]
    fn teams_of_two_check_their_line_catch_dodgers_and_book_the_paid_fines() {
        let mut c = company("dk");
        let date = with_inspectors(&mut c, 3);
        c.date = date.clone();
        let p = staff::patrols(&c, &date);
        // (a pair on the busiest line, the third alone on the next)
        assert_eq!(p.iter().filter(|x| x.ids.len() == 2).count(), 1);
        assert!(p.iter().any(|x| x.ids.len() == 2 && x.line == "Linie5") && p.iter().any(|x| x.ids.len() == 1 && x.line == "Linie7"));
        let pair = p.iter().find(|x| x.ids.len() == 2).unwrap().share;
        let lone = p.iter().find(|x| x.ids.len() == 1 && x.from == p[0].from).map(|x| x.share).unwrap();
        assert!(lone < pair);
        let mut seen = trips("Linie5", "5", 0.05);
        seen.extend(trips("Linie7", "7", 0.05));
        let lines = vec![LineDay { line: "Linie5".into(), passengers: 2280, dodgers: 114.0, ..Default::default() }];
        let day = close(&mut c, &date, lines, &seen);
        assert!(!day.checks.is_empty());
        // (one boarding after the other, within their hours, at most 45 a boarding for the pair and 22 alone)
        for k in &day.checks {
            let pt = p.iter().find(|x| x.ids == k.team && k.at >= x.from && k.at < x.to).expect("its patrol");
            assert!(k.at >= pt.from && k.at < pt.to, "{k:?}");
            assert!(k.checked as f64 <= if k.team.len() == 2 { PAIR_CHECKS } else { LONE_CHECKS } + 0.5);
            assert!(k.caught <= k.checked && k.fine == fine(&c) && !k.names.is_empty());
        }
        let caught: u32 = day.checks.iter().map(|k| k.caught).sum();
        assert!(caught > 0, "{:?}", day.checks.len());
        assert_eq!(day.written, caught as Cents * fine(&c));
        let booked: Cents = c.ledger.iter().filter(|b| b.kind == BookingKind::InspectionFines).map(|b| b.amount).sum();
        assert_eq!(booked, day.paid);
        assert!((day.paid as f64 - day.written as f64 * PAID).abs() <= 2.0);
        // (the line is kept as checked, and the message told)
        assert!(c.inspection.days[0].lines[0].checked > 0.0);
        assert!(c.inbox.messages.iter().any(|m| m.key == "inspection"));
        assert_eq!(c.inspection.months.len(), 1);
        assert_eq!(c.inspection.months[0].caught, caught);
    }

    #[test]
    fn the_same_day_draws_the_same_checks() {
        let mut a = company("de");
        let date = with_inspectors(&mut a, 2);
        a.date = date.clone();
        let mut b = a.clone();
        let seen = trips("Linie5", "5", 0.08);
        let (x, y) = (close(&mut a, &date, Vec::new(), &seen), close(&mut b, &date, Vec::new(), &seen));
        assert_eq!(x, y);
        // (another day draws others)
        let mut c = a.clone();
        let other = dates::add(&date, 7);
        c.date = other.clone();
        let z = close(&mut c, &other, Vec::new(), &seen);
        assert!(z.checks.is_empty() || z.checks.iter().map(|k| (k.checked, k.caught)).collect::<Vec<_>>() != x.checks.iter().map(|k| (k.checked, k.caught)).collect::<Vec<_>>());
    }

    #[test]
    fn days_are_kept_sixty_days_and_months_longer() {
        let mut c = company("de");
        let start = c.date.clone();
        for k in 0..70 {
            let d = dates::add(&start, k);
            close(&mut c, &d, vec![LineDay { line: "Linie5".into(), passengers: 100, dodgers: 5.0, ..Default::default() }], &[]);
        }
        assert_eq!(c.inspection.days.len(), KEPT_DAYS);
        assert_eq!(c.inspection.days.last().unwrap().date, dates::add(&start, 69));
        assert_eq!(c.inspection.days[0].date, dates::add(&start, 10));
        // (no checks without inspectors: nothing booked, no months)
        assert!(c.ledger.iter().all(|b| b.kind != BookingKind::InspectionFines));
        assert!(c.inspection.months.is_empty());
        let (n, ..) = totals(&c, &dates::add(&start, 69), 30);
        assert_eq!(n, 0);
        // (an old file without any of it reads)
        let mut v = serde_json::to_value(&c).unwrap();
        v.as_object_mut().unwrap().remove("inspection");
        let back: Company = serde_json::from_value(v).unwrap();
        assert!(back.inspection.days.is_empty());
    }

    #[test]
    fn a_line_checked_keeps_fewer_dodgers_for_two_weeks() {
        use super::super::dodgers;
        let mut c = company("de");
        let date = c.date.clone();
        // (nobody checked it: the base share, the fares as they always were)
        assert_eq!(dodgers::deterrence(&c, &date, "Linie5"), 0.0);
        assert!((dodgers::fare_factor(dodgers::share(dodgers::raw(10 * 60, false), 0.0, false)) - 1.0).abs() < 1e-12);
        // (checked all day yesterday: fewer today, fewer still the day after it was checked again,
        // and it fades)
        let yesterday = dates::add(&date, -1);
        c.inspection.days.push(Day { date: yesterday.clone(), lines: vec![LineDay { line: "Linie5".into(), checked: 1.0, ..Default::default() }], ..Default::default() });
        let d1 = dodgers::deterrence(&c, &date, "Linie5");
        let d7 = dodgers::deterrence(&c, &dates::add(&date, 6), "Linie5");
        let d15 = dodgers::deterrence(&c, &dates::add(&date, 14), "Linie5");
        assert!(d1 > d7 && d7 > 0.0 && d15 == 0.0, "{d1} {d7} {d15}");
        assert_eq!(dodgers::deterrence(&c, &date, "Linie7"), 0.0);
        assert!(dodgers::share(dodgers::BASE, d1, false) < dodgers::BASE);
        // (the researched working method keeps more off)
        c.research.done.push("inspection_1".into());
        let more = dodgers::deterrence(&c, &date, "Linie5");
        assert!(more >= d1);
        // (inspectors at work today: more again)
        let mut w = company("de");
        let day = with_inspectors(&mut w, 2);
        w.inspection = c.inspection.clone();
        assert!(dodgers::deterrence(&w, &day, "Linie5") > dodgers::deterrence_with(&w, &day, "Linie5", 0.0));
        // (a night trip has more: its fares a little less than the base's)
        assert!(dodgers::fare_factor(dodgers::share(dodgers::raw(23 * 60, false), 0.0, false)) < 1.0);
    }

    /// Two directions of a line, a bus every 10 minutes each way from 05:00, 28 minutes, 14 stops.
    fn both_ways(line: &str, share: f64) -> Vec<Seen> {
        (0..112)
            .map(|k| {
                let dep = 5 * 60 + 10 * (k / 2);
                let (a, b) = if k % 2 == 0 { ("Bahnhof", "Markt") } else { ("Markt", "Bahnhof") };
                Seen { line: line.into(), number: line.trim_start_matches("Linie").into(), dep, arr: dep + 28, from: a.into(), to: b.into(), pax: 40, share, crowded: false, stops: 14 }
            })
            .collect()
    }

    /// (round T) A team rides a few stops, gets off and takes the next bus passing there after a
    /// short wait - the earliest either way, the other direction when they come together.
    #[test]
    fn a_team_rides_a_few_stops_and_takes_the_next_bus() {
        let mut c = company("de");
        let date = with_inspectors(&mut c, 2);
        c.date = date.clone();
        let seen = both_ways("Linie5", 0.08);
        let day = close(&mut c, &date, Vec::new(), &seen);
        let mut mine: Vec<&Check> = day.checks.iter().filter(|k| k.line == "Linie5").collect();
        mine.sort_by_key(|k| k.at);
        assert!(mine.len() >= 20, "{}", mine.len());
        for k in &mine {
            let rode = k.alight - k.board;
            // (3 to 8 stops, fewer only where the trip ends)
            assert!((1..=8).contains(&rode) && (rode >= 3 || k.alight == k.stops - 1), "{k:?}");
            assert!(k.off > k.at && k.off - k.at <= 20, "{k:?}");
            assert!(k.checked as f64 <= PAIR_CHECKS + 0.5);
        }
        for w in mine.windows(2) {
            let (a, b) = (w[0], w[1]);
            // (after the wait, at the stop they got off at: the same place either way)
            assert!(b.at >= a.off + TRANSFER.0 as i32, "{a:?} {b:?}");
            let place = |k: &Check, i: u32| if k.from == a.from { i } else { k.stops - 1 - i };
            if b.board != 0 || b.at != b.dep {
                assert_eq!(place(b, b.board), place(a, a.alight), "{a:?} {b:?}");
            }
        }
        // (the earliest bus either way; the other direction when they come together)
        let trips: Vec<&Seen> = seen.iter().collect();
        let here = trips.iter().find(|s| s.from == "Bahnhof" && s.dep == 8 * 60).unwrap();
        let (bus, i, when) = next_bus(&trips, here, 7, at_stop(here, 7) + 3, 24 * 60).unwrap();
        let (_, j, other) = next_bus(&trips, here, 7, when + 1, 24 * 60).unwrap();
        assert!(when <= other);
        let _ = (bus, i, j);
        let same_minute = vec![Seen { line: "L".into(), dep: 600, arr: 626, from: "A".into(), to: "B".into(), stops: 14, ..Default::default() }, Seen { line: "L".into(), dep: 598, arr: 624, from: "B".into(), to: "A".into(), stops: 14, ..Default::default() }];
        let refs: Vec<&Seen> = same_minute.iter().collect();
        let cur = Seen { line: "L".into(), dep: 560, arr: 586, from: "A".into(), to: "B".into(), stops: 14, ..Default::default() };
        // (stop 6 of 14 by A→B is stop 7 of B→A: both buses there at 612 - the other direction is taken)
        let (pick, at, _) = next_bus(&refs, &cur, 6, 600, 24 * 60).unwrap();
        assert_eq!((pick.from.as_str(), at), ("B", 7));
    }

    /// (round T) Boardings check more buses with fewer each: a day's checks and catches per
    /// inspector-hour stay where round S had them (within a fifth), and the same day draws the same.
    #[test]
    fn boardings_keep_the_days_totals() {
        let mut seen = both_ways("Linie5", 0.05);
        seen.extend(both_ways("Linie7", 0.05));
        // (round S's whole trips, measured on the same buses: caught an inspector-hour)
        for (n, before) in [(2usize, 0.645), (3, 0.818), (4, 0.716)] {
            let mut c = company("de");
            let first = with_inspectors(&mut c, n);
            let ids: Vec<u32> = c.staff.iter().filter(|e| e.role == Role::Inspector).map(|e| e.id).collect();
            let (mut caught, mut hours, mut buses, mut days) = (0u64, 0.0f64, 0usize, 0);
            for k in 0..120 {
                let d = dates::add(&first, k);
                if !ids.iter().all(|id| staff::at_work(c.employee(*id).unwrap(), &d)) {
                    continue;
                }
                c.date = d.clone();
                c.inspection = Default::default();
                hours += staff::patrols(&c, &d).iter().map(|x| (x.to - x.from) as f64 / 60.0 * x.ids.len() as f64).sum::<f64>();
                let mut again = c.clone();
                let day = close(&mut c, &d, Vec::new(), &seen);
                assert_eq!(day, close(&mut again, &d, Vec::new(), &seen));
                caught += day.checks.iter().map(|x| x.caught as u64).sum::<u64>();
                buses += day.checks.len();
                days += 1;
            }
            let rate = caught as f64 / hours;
            assert!((0.8 * before..=1.2 * before).contains(&rate), "{n} inspectors: {rate:.3} caught an hour against {before}");
            // (more buses than whole trips: round S rode about 11 a pair a day)
            assert!(buses as f64 / days as f64 > 20.0 * (n / 2) as f64, "{n}: {}", buses as f64 / days as f64);
            eprintln!("BOARDINGS n={n} caught/insp_h={rate:.3} (before {before}) buses/day={:.1}", buses as f64 / days as f64);
        }
    }
}

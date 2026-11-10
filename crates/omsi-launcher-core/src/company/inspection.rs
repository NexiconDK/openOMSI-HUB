//! (NexiconDK, ticket inspection round S, 9 November 2026) The ticket inspectors' day, as it was:
//! their patrols (`staff::patrols` - in teams of two, a lone one where the number is odd or for
//! the hours one of them stays on) ride the trips of their line one after the other within their
//! hours, check the passengers on board (`ON_BOARD` of a trip's riders are aboard at once; a pair
//! checks at most `PAIR_CHECKS` of them a trip, a lone inspector `LONE_CHECKS`, more with the
//! researched working method) and catch those without a ticket - the trip's share of dodgers
//! (`dodgers`), less the few who get off before they are reached (`ESCAPE`). Each caught pays the
//! country's fine (`fine`: 750 kr. in Denmark, 60 € in Germany, …); about two thirds of it is
//! paid (`PAID`), the rest is written off. The paid fines are booked as their own income
//! (`BookingKind::InspectionFines`), a line a day.
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
/// The most passengers a trip a pair checks, and a lone inspector.
pub const PAIR_CHECKS: f64 = 45.0;
pub const LONE_CHECKS: f64 = 22.0;
/// Minutes between getting off one trip and boarding the next.
pub const CHANGE: (i64, i64) = (6, 18);

/// What the inspectors saw, kept.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Book {
    #[serde(default)]
    pub days: Vec<Day>,
    #[serde(default)]
    pub months: Vec<MonthTotal>,
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
        let most = if p.ids.len() >= 2 { PAIR_CHECKS } else { LONE_CHECKS } * factor;
        let mut trips: Vec<&Seen> = seen.iter().filter(|s| s.line.eq_ignore_ascii_case(&p.line) && s.dep >= p.from && s.dep < p.to).collect();
        trips.sort_by_key(|s| (s.dep, s.arr));
        let mut free = p.from;
        for s in trips {
            if s.dep < free {
                continue;
            }
            let aboard = s.pax as f64 * rng.range(ON_BOARD.0, ON_BOARD.1);
            let checked = aboard.min(most).round() as u32;
            let caught = (0..checked).filter(|_| rng.chance(s.share * (1.0 - ESCAPE))).count() as u32;
            checks.push(Check { line: s.line.clone(), number: s.number.clone(), dep: s.dep, arr: s.arr, from: s.from.clone(), to: s.to.clone(), team: p.ids.clone(), names: names.clone(), checked, caught, fine: each });
            free = s.arr + rng.int(CHANGE.0, CHANGE.1) as i32;
        }
    }
    checks.sort_by(|a, b| a.dep.cmp(&b.dep).then(a.number.cmp(&b.number)));
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
    let day = Day { date: date.to_string(), lines, checks, written, paid };
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
        (0..57).map(|k| 5 * 60 + 20 * k).map(|dep| Seen { line: line.into(), number: number.into(), dep, arr: dep + 30, from: "Bahnhof".into(), to: "Markt".into(), pax: 40, share, crowded: false }).collect()
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
        // (one trip after the other, within their hours, at most 45 a trip for the pair and 22 alone)
        for k in &day.checks {
            let pt = p.iter().find(|x| x.ids == k.team && k.dep >= x.from && k.dep < x.to).expect("its patrol");
            assert!(k.dep >= pt.from && k.dep < pt.to, "{k:?}");
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
}

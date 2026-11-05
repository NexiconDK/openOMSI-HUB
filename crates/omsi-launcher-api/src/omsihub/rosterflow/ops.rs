//! RosterFlow Pro's tabs "4 Daglig drift" and "5 Kalender" (step 4, NexiconDK 4 November 2026,
//! made clearer after the player's sketches the same day; the core's `company::pro_ops` and
//! `company::pro_cut`):
//!
//! - the calendar (`rf_cal`): the drivers grouped - permanent, reserves with a frame, hourly paid -
//!   each with his number ("chf.") and his week or frame; every day of the week from Sunday his
//!   duty and its working time (a part of a cut duty by its letter, "1002C"), a day off, ill, on
//!   holiday or a frame, and the hours of his week against his contract. Above them each day's
//!   duties without a driver. A week, 14 days or a month.
//! - the box beside it, and the daily operations of a date (`rf_ops`): step 1 the date's duties
//!   without a driver and why; step 2 the duty chosen - who can drive it, permanent drivers first,
//!   then the reserves, then the hourly paid, the cheapest first, each with one thing to do by the
//!   rules of asking (`pro_ops::Rule`: put on, ask, waiting for his answer, give it after a yes) and
//!   his answer in a box of its own; those who cannot, folded. When nobody can or will: cut it
//!   (`pro_cut`) - its parts, each with one driver suggested.
//! - their acts (`rf_ops_act`): ask, give (put on), take a question back, take the duty back, cut,
//!   join again, an agency driver (only when the agency setting is on).
//!
//! A date outside the company's week is planned with the timetable of its weekday in the
//! company's week (`plan_on`).

use super::super::company::words::{day_label, eur, num, short_date, tr, trf};
use super::super::company::{self, Session};
use super::{str_arg, usize_arg, Rf};
use crate::Answer;
use omsi_launcher_lib::company::model::{Cents, Employee, Employment};
use omsi_launcher_lib::company::plan::{self as plan, Cannot, DayPlan, Problem};
use omsi_launcher_lib::company::pro_duties as pd;
use omsi_launcher_lib::company::pro_cut::{self as pc, How};
use omsi_launcher_lib::company::pro_ops::{self as po, Asked, DayIs, OpenDuty, Price, Reply, Rule};
use omsi_launcher_lib::company::pro_rest::{self as pr, Breach};
use omsi_launcher_lib::company::pro_turnus::{self as pt, ProDuty, Week};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};
use std::collections::HashMap;

/// The days of a week, Sunday first (translated when shown).
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The days of a week in full, Sunday first (translated when shown).
const FULL_DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// The answers a driver can give, as the page shows them: their key and words.
const ANSWERS: [(&str, &str); 7] = [("yes", "Yes"), ("no", "No"), ("early", "Too early"), ("late", "Too late"), ("swap", "Swap with a colleague"), ("text", "In his own words"), ("silent", "No answer")];


fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// "05:05".
fn clock(m: i32) -> String {
    format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(60))
}

/// "2 t 40" / "3 t": a length as the page writes it.
fn length(m: i32) -> String {
    let m = m.max(0);
    if m % 60 == 0 {
        trf("%{h} h", &[("h", &(m / 60).to_string())])
    } else {
        trf("%{h} h %{m}", &[("h", &(m / 60).to_string()), ("m", &format!("{:02}", m % 60))])
    }
}

/// One decimal, the language's way ("31,8").
fn hours(minutes: f64) -> String {
    num(minutes / 60.0, 1)
}

/// The plan of `date` as the daily operations see it: one of the company's week its own
/// (`company::plan_of`), another date's made from the timetable of its weekday in the company's
/// week - the rotas, exceptions and duties given of that date, its drivers' illnesses and
/// holidays. None while the timetable is being read.
fn plan_on(s: &mut Session, r: &mut Rf, date: &str) -> Option<Result<DayPlan, String>> {
    let c = s.company.as_ref()?;
    let k = co::dates::between(&c.date, date);
    if (0..7).contains(&k) {
        return company::plan_of(s, date);
    }
    let key = (s.generation, s.plan_gen);
    if let Some(p) = r.ops_plans.iter().find(|x| x.0 == key && x.1 == date) {
        return Some(Ok(p.2.clone()));
    }
    let model = co::dates::add(&c.date, k.rem_euclid(7));
    let Some(lines) = company::lines_of(s, &model) else {
        return match company::plan_of(s, &model) {
            Some(Err(e)) => Some(Err(e)),
            _ => None,
        };
    };
    let c = s.company.as_ref()?;
    let tours = co::network::tours_of_day(c, &lines, date);
    let p = plan::day_plan(c, date, tours, &[], &[], false);
    r.ops_plans.retain(|x| x.0 == key);
    r.ops_plans.push((key, date.to_string(), p.clone()));
    Some(Ok(p))
}

/// The plans of `dates`, or what to say while they are being read.
fn plans_of(s: &mut Session, r: &mut Rf, dates: &[String]) -> Result<HashMap<String, DayPlan>, String> {
    company::week_work(s);
    let mut out = HashMap::new();
    for d in dates {
        match plan_on(s, r, d) {
            Some(Ok(p)) => {
                out.insert(d.clone(), p);
            }
            Some(Err(e)) => return Err(e),
            None => return Err(tr("Reading the timetable…")),
        }
    }
    Ok(out)
}

/// The duties of `date` as the Vagter tab numbers them (those of its weekday).
fn duties_on<'a>(week: &'a Week, date: &str) -> &'a [ProDuty] {
    week.get(pt::day_of(date)).map(Vec::as_slice).unwrap_or(&[])
}

/// The duties of the plan `p`'s date: its weekday's, or - a duty cut for that date - its own
/// (`pro_cut`: the parts cut off are duties of their own that day).
fn duties_for(c: &Company, week: &Week, p: &DayPlan) -> Vec<ProDuty> {
    if pc::splits_on(c, &p.date).next().is_some() {
        pt::day_duties(c, &p.date, p.tours.iter().map(|t| t.tour.clone()).collect())
    } else {
        duties_on(week, &p.date).to_vec()
    }
}

/// A driver's employment as the chip says it: its key, its word and its week or frame.
fn kind_of(c: &Company, e: &Employee) -> (&'static str, String, String) {
    match e.employment {
        Employment::FullTime => ("full", tr("Permanent"), trf("%{h} h", &[("h", &num(co::econ_uk::contract_week_hours(c), 0))])),
        Employment::PartTime { hours } => ("part", tr("Permanent"), trf("%{h} h", &[("h", &num(hours, if hours.fract() == 0.0 { 0 } else { 1 }))])),
        Employment::Reserve(f) => ("reserve", tr("Reserve frame"), trf("frame %{from}–%{to}", &[("from", &clock(f.from)), ("to", &clock(f.to))])),
        Employment::Hourly => ("hourly", tr("Hourly"), tr("hourly paid")),
    }
}

/// The order the calendar lists the kinds in: permanent, frame, hourly.
fn kind_rank(e: &Employee) -> u8 {
    match e.employment {
        Employment::FullTime | Employment::PartTime { .. } => 0,
        Employment::Reserve(_) => 1,
        Employment::Hourly => 2,
    }
}

/// A duty's colour: early, day, late, split (the Turnus tab's).
fn cls_of(start: i32, split: bool) -> &'static str {
    if split {
        "split"
    } else if start < 9 * 60 {
        "early"
    } else if start < 12 * 60 {
        "day"
    } else {
        "late"
    }
}

/// The name of `date`'s weekday in a sentence ("Monday"; lower case in the languages that write
/// it so: "mandag").
fn day_name(date: &str) -> String {
    let n = tr(FULL_DAYS[pt::day_of(date)]);
    if ["", "en", "de"].contains(&company::words::language().as_str()) {
        n
    } else {
        n.to_lowercase()
    }
}

/// "06:15 / 14:30": when a split duty's two halves begin (the pieces of the driver's day `pieces`,
/// a pause of 1 h 30 or more between them), or None.
fn halves(p: &DayPlan, pieces: &[(usize, usize)], sign_on: i32) -> Option<String> {
    let mut v: Vec<(i32, i32)> = pieces.iter().map(|&(ti, k)| (p.tours[ti].duties[k].from, p.tours[ti].duties[k].to)).collect();
    v.sort_unstable();
    let gap = v.windows(2).position(|w| w[1].0 - w[0].1 >= 90)?;
    Some(format!("{} / {}", clock(v[0].0 - sign_on), clock(v[gap + 1].0 - sign_on)))
}

/// A driver's day as a cell of the calendar (`pieces` his pieces that day, `for_whom` whose duty
/// he drives for him).
fn cell_json(day: &po::DriverDay, duties: &[ProDuty], split_times: Option<String>, for_whom: Option<String>) -> Value {
    match &day.is {
        DayIs::Duties { numbers, start, end, frame, labels } => {
            let split = numbers.iter().any(|n| duties.iter().any(|d| d.number == *n && d.kind == pd::Kind::Split));
            let time = split_times.filter(|_| split).unwrap_or_else(|| format!("{}–{}", clock(*start), clock(*end)));
            let sub = match &for_whom {
                Some(w) => format!("{} · {}", clock(*start), trf("for %{name}", &[("name", w)])),
                None => time.clone(),
            };
            json!({ "kind": "duty", "label": labels.join(" + "), "sub": sub, "tip": format!("{}–{}", clock(*start), clock(*end)), "cls": cls_of(*start, split), "frame": frame.is_some(), "numbers": numbers })
        }
        DayIs::Frame { from, to } => json!({ "kind": "frame", "label": tr("Reserve frame"), "sub": format!("{}–{}", clock(*from), clock(*to)), "cls": "reserve" }),
        DayIs::Off => json!({ "kind": "off", "label": "–", "cls": "off" }),
        DayIs::Ill => json!({ "kind": "ill", "label": tr("Ill"), "cls": "ill" }),
        DayIs::Holiday => json!({ "kind": "holiday", "label": tr("Holiday"), "cls": "holiday" }),
        DayIs::Away => json!({ "kind": "away", "label": "–", "cls": "none" }),
    }
}

/// The drivers of the company, as the calendar and the daily operations list them.
fn drivers(c: &Company) -> Vec<&Employee> {
    let mut v: Vec<&Employee> = c.staff.iter().filter(|e| co::staff::drives(e)).collect();
    v.sort_by_key(|e| (kind_rank(e), e.id));
    v
}

fn off(c: &Company) -> Option<Value> {
    (!pd::on(c)).then(|| json!({ "off": true, "reading": tr("RosterFlow Pro is not the planner of this company: choose it on the planning page.") }))
}

/// "Week 45".
fn week_label(sunday: &str) -> String {
    trf("Week %{n}", &[("n", &pt::week_number(&co::dates::add(sunday, 1)).to_string())])
}

/// The first name of driver `id`.
fn first_name(c: &Company, id: u32) -> String {
    c.employee(id).map(|e| e.name.split_whitespace().next().unwrap_or(&e.name).to_string()).unwrap_or_default()
}

/// Whose duty driver `id` drives on `date` for him: a duty given him for the date that another's
/// rota row has (`pro_ops::Call`).
fn for_whom(c: &Company, id: u32, date: &str) -> Option<String> {
    let call = po::calls_on(c, date).find(|x| x.driver == id && x.part.is_none())?;
    c.staff.iter().find(|e| e.id != id && pt::decided(c, e.id, date).is_some_and(|d| d.planned == pt::Cell::Duty { n: call.n })).map(|e| first_name(c, e.id))
}

/// The calendar (`rf_cal`: `week` the weeks on from the company's, `span` "week" | "two" |
/// "month", `kind` "all" | "permanent" | "reserve" | "hourly").
fn calendar(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({ "none": true })) };
    if let Some(v) = off(&c) {
        return Ok(v);
    }
    let offset = usize_arg(args, "week").min(104) as i64;
    let span = str_arg(args, "span");
    let weeks = match span.as_str() {
        "two" => 2,
        "month" => 5,
        _ => 1,
    };
    let first = co::dates::add(&pt::sunday_of(&c.date), 7 * offset);
    let dates: Vec<String> = (0..7 * weeks).map(|k| co::dates::add(&first, k)).collect();
    let last = dates.last().cloned().unwrap_or_else(|| first.clone());
    let mut out = json!({
        "week": { "label": week_label(&first), "from": day_label(&first), "to": day_label(&last), "span_text": trf("%{from} – %{to}", &[("from", &format!("{} {}", day_name(&first), short_date(&first))), ("to", &format!("{} {}", day_name(&last), short_date(&last)))]), "offset": offset, "span": if weeks == 1 { "week" } else if weeks == 2 { "two" } else { "month" } },
        "now": trf("now: %{day} %{time}", &[("day", &day_name(&c.date)), ("time", &co::clock::hhmm(co::clock::now(&c)))]),
    });
    let week = match super::turnus::week_duties(s, r) {
        None => {
            out["reading"] = json!(tr("Reading the timetable…"));
            return Ok(out);
        }
        Some(Err(e)) => {
            out["reading"] = json!(e);
            return Ok(out);
        }
        Some(Ok(w)) => w,
    };
    let plans = match plans_of(s, r, &dates) {
        Ok(p) => p,
        Err(e) => {
            out["reading"] = json!(e);
            return Ok(out);
        }
    };
    let duties: HashMap<&str, Vec<ProDuty>> = dates.iter().filter_map(|d| plans.get(d).map(|p| (d.as_str(), duties_for(&c, &week, p)))).collect();
    let days: Vec<Value> = dates
        .iter()
        .map(|d| {
            let missing = plans.get(d).map_or(0, |p| open_items(&c, p, &duties[d.as_str()]).len());
            json!({ "date": d, "wd": tr(DAYS[pt::day_of(d)]), "name": day_name(d), "day": short_date(d), "today": *d == c.date, "past": co::dates::between(&c.date, d) < 0, "missing": missing, "label": day_label(d),
                "missing_text": match missing { 0 => tr("all filled"), 1 => tr("duty missing"), _ => tr("duties missing") } })
        })
        .collect();
    let want = str_arg(args, "kind");
    let sign_on = c.planning.flow.prefs.sign_on;
    let list: Vec<Value> = drivers(&c)
        .into_iter()
        .filter(|e| match want.as_str() {
            "permanent" => kind_rank(e) == 0,
            "reserve" => kind_rank(e) == 1,
            "hourly" => kind_rank(e) == 2,
            _ => true,
        })
        .map(|e| {
            let mut minutes: HashMap<&str, f64> = HashMap::new();
            let cells: Vec<Value> = dates
                .iter()
                .map(|d| {
                    let Some(p) = plans.get(d) else { return json!({ "kind": "away", "label": "–", "cls": "none" }) };
                    let ds = &duties[d.as_str()];
                    let day = po::driver_day(&c, e, p, ds);
                    minutes.insert(d.as_str(), day.minutes);
                    let mine = p.duties_of(plan::Who::Staff(e.id));
                    let mut v = cell_json(&day, ds, halves(p, &mine, sign_on), for_whom(&c, e.id, d));
                    v["date"] = json!(d);
                    v
                })
                .collect();
            let (key, chip, sub) = kind_of(&c, e);
            let contract = po::contract_minutes(&c, e).filter(|_| kind_rank(e) == 0);
            let hours_of: Vec<Value> = (0..weeks)
                .map(|w| {
                    let sunday = co::dates::add(&first, 7 * w);
                    let m = po::week_minutes(&c, e, &sunday, &|d| minutes.get(d).copied().unwrap_or(0.0));
                    let over = contract.is_some_and(|k| m > k + 0.5);
                    let pct = contract.map(|k| (m / k * 100.0).clamp(0.0, 100.0).round());
                    json!({ "h": hours(m), "of": contract.map(|k| num(k / 60.0, if (k / 60.0).fract() == 0.0 { 0 } else { 1 })), "over": over, "pct": pct, "label": week_label(&sunday) })
                })
                .collect();
            let what = match e.employment {
                Employment::FullTime => trf("full time %{h} h", &[("h", &num(co::econ_uk::contract_week_hours(&c), 0))]),
                Employment::PartTime { hours } => trf("part time %{h} h", &[("h", &num(hours, if hours.fract() == 0.0 { 0 } else { 1 }))]),
                _ => sub.clone(),
            };
            let group = ["permanent", "reserve", "hourly"][kind_rank(e) as usize];
            json!({ "id": e.id, "nr": e.id, "name": e.name, "kind": key, "group": group, "chip": chip, "sub": sub, "what": what, "cells": cells, "hours": hours_of })
        })
        .collect();
    out["days"] = json!(days);
    out["drivers"] = json!(list);
    out["groups"] = json!({ "permanent": tr("Permanent drivers"), "reserve": tr("Reserves with a frame"), "hourly": tr("Hourly paid") });
    out["kinds"] = json!([["all", tr("All")], ["permanent", tr("Permanent")], ["reserve", tr("Reserve frame")], ["hourly", tr("Hourly")]]);
    out["legend"] = json!([["early", tr("Early duty")], ["day", tr("Day duty")], ["late", tr("Late duty")], ["split", tr("Split duty")], ["reserve", tr("Reserve frame")], ["ill", tr("Ill")], ["holiday", tr("Holiday")], ["off", tr("Day off (–)")]]);
    Ok(out)
}

/// Why a duty is open, in words.
fn why_words(p: Option<Problem>) -> String {
    match p {
        Some(Problem::Unassigned) | None => tr("No driver planned"),
        Some(p) => tr(p.label()),
    }
}

/// Why a driver cannot take a duty, in words.
fn cannot_words(x: &Cannot) -> String {
    match x {
        Cannot::NotDriver => tr("Not a driver"),
        Cannot::NotEmployed => tr("Not employed that day"),
        Cannot::Ill { .. } => tr("Ill"),
        Cannot::Holiday { .. } => tr("On holiday"),
        Cannot::WeekDone { .. } => tr("Has worked his days this week"),
        Cannot::Lacks(_) => tr("No licence for this bus"),
        Cannot::OnDuty { from, to, .. } => trf("drives another duty %{from}–%{to}", &[("from", &clock(*from)), ("to", &clock(*to))]),
        Cannot::Rest { minutes } => trf("only %{m} min rest to his other duty", &[("m", &minutes.to_string())]),
        Cannot::NightRest { from } => trf("his night's rest: free from %{time}", &[("time", &clock(*from))]),
        Cannot::TooLong { minutes } => trf("his day would be %{t}", &[("t", &length(*minutes))]),
    }
}

/// What a driver costs, in short words ("overtime 2 h 40 = 1.070 kr."), and whether it is
/// overtime (shown orange).
fn price_short(p: &Price, cost: Cents) -> (String, bool) {
    let money = eur(cost);
    match p {
        Price::Frame | Price::Within => (tr("nothing extra"), false),
        Price::Overtime { minutes, .. } => (trf("overtime %{t} = %{cost}", &[("t", &length(*minutes)), ("cost", &money)]), true),
        Price::Hourly { .. } => (money, false),
        Price::Extra { minutes } => (trf("%{t} extra = %{cost}", &[("t", &length(*minutes)), ("cost", &money)]), false),
        Price::Agency { minutes } => (trf("agency %{t} = %{cost}", &[("t", &length(*minutes)), ("cost", &money)]), false),
    }
}

/// A name for an answer's places: a driver's.
fn name_in(c: &Company) -> impl Fn(u32) -> String + '_ {
    |id| c.employee(id).map(|e| e.name.clone()).unwrap_or_default()
}

/// An answer as the page shows it: its kind, its word, its words (translated, places filled).
fn reply_json(c: &Company, r: &Reply) -> Value {
    let (text, args) = po::words(r, &name_in(c));
    let words = match r {
        Reply::Text { text, .. } => format!("\"{text}\""),
        Reply::Silent => String::new(),
        _ => {
            let a: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
            format!("\"{}\"", trf(&text, &a))
        }
    };
    let chip = ANSWERS.iter().find(|x| x.0 == r.key()).map(|x| tr(x.1)).unwrap_or_default();
    json!({ "key": r.key(), "chip": chip, "words": words, "yes": r.yes(), "swap": match r { Reply::Swap { with, theirs, .. } => json!({ "with": with, "theirs": theirs }), _ => Value::Null } })
}

/// The week's minutes of every driver without the duties open (`po::week_minutes`), of the week
/// `date` is in.
fn week_of_drivers(c: &Company, week: &Week, plans: &HashMap<String, DayPlan>, date: &str) -> HashMap<u32, f64> {
    let sunday = pt::sunday_of(date);
    drivers(c)
        .into_iter()
        .chain(co::agency::drivers_on(c, date))
        .map(|e| {
            let m = po::week_minutes(c, e, &sunday, &|d| plans.get(d).map_or(0.0, |p| po::driver_day(c, e, p, &duties_for(c, week, p)).minutes));
            (e.id, m)
        })
        .collect()
}

/// Driver `e`'s working day on `date` for the rest rules (`pro_rest::Span`), as the plans have it.
fn span_on(c: &Company, week: &Week, plans: &HashMap<String, DayPlan>, e: &Employee, date: &str) -> pr::Span {
    let p = plans.get(date)?;
    match po::driver_day(c, e, p, &duties_for(c, week, p)).is {
        DayIs::Duties { numbers, start, end, .. } => Some((start, end, numbers.first().copied().unwrap_or(0))),
        _ => None,
    }
}

/// The rest rules open duty `d` of `date` would break for driver `e` (`pro_rest::breaches`):
/// against his days before and after and his week (`weekly` its minutes without it).
fn breaches_of(c: &Company, week: &Week, plans: &HashMap<String, DayPlan>, e: &Employee, date: &str, d: &OpenDuty, weekly: &HashMap<u32, f64>) -> Vec<Breach> {
    // (the company's day: the night's rest since his last duty is the game's own rule)
    let prev = if date == c.date { None } else { span_on(c, week, plans, e, &co::dates::add(date, -1)) };
    let next = span_on(c, week, plans, e, &co::dates::add(date, 1));
    let own = span_on(c, week, plans, e, date);
    let days: Vec<pr::Span> = po::week_dates(date).iter().map(|x| span_on(c, week, plans, e, x)).collect();
    // (within a reserve's frame the duty adds no working time to his week: the frame is paid)
    let p = plans.get(date);
    let from = p.and_then(|p| d.pieces.iter().map(|&(ti, k)| p.tours[ti].duties[k].from).min()).unwrap_or(d.start);
    let to = p.and_then(|p| d.pieces.iter().map(|&(ti, k)| p.tours[ti].duties[k].to).max()).unwrap_or(d.end);
    let in_frame = pt::frame_on(c, e, date).is_some_and(|f| f.holds(from, to));
    let paid = if in_frame { 0 } else { d.paid };
    let week_now = if in_frame { 0.0 } else { weekly.get(&e.id).copied().unwrap_or(0.0) };
    pr::breaches(d.start, d.end, prev, next, own, pr::reduced_in(&days), week_now, paid)
}

/// A rule broken, in words ("⚠ Only 9 h rest after duty 1005").
fn breach_words(b: &Breach) -> String {
    match b {
        Breach::RestAfter { minutes, n } => trf("⚠ Only %{t} rest after duty %{n}", &[("t", &length(*minutes)), ("n", &n.to_string())]),
        Breach::RestBefore { minutes, n } => trf("⚠ Only %{t} rest before duty %{n}", &[("t", &length(*minutes)), ("n", &n.to_string())]),
        Breach::LongDay { .. } => tr("⚠ Over 13 h on the day"),
        Breach::LongWeek { .. } => tr("⚠ Over 48 h in the week"),
    }
}

/// A row of the box with the rules its driver would break: said in red, the thing to do "anyway"
/// with what it costs (asked first in the popup).
fn with_breaches(c: &Company, row: &mut Value, name: &str, breaches: &[Breach]) {
    if breaches.is_empty() {
        return;
    }
    row["breach"] = json!(breaches.iter().map(breach_words).collect::<Vec<_>>());
    let fine = pr::fine(c);
    let mut cost = trf("%{name} breaks the rest rules: less content and more stressed.", &[("name", name)]);
    if fine > 0 {
        cost = format!("{cost} {}", trf("The fine: %{amount}", &[("amount", &eur(fine))]));
    }
    row["anyway"] = json!(cost);
    if row["action"] == "set" {
        row["action"] = json!("set_anyway");
    } else if row["action"] == "give" {
        row["action"] = json!("give_anyway");
    }
}

/// The extra duties a driver was given this month, in words.
fn extra_words(c: &Company, id: u32, date: &str) -> (usize, String) {
    let n = pr::extras_in_month(c, id, date);
    (n, trf("%{n} extra this month", &[("n", &n.to_string())]))
}

/// What the box lists as missing on the plan `p`'s date: the open duties no cut covers, and the
/// duties cut for the date with a part still without a driver (n, is it cut).
fn open_items(c: &Company, p: &DayPlan, ds: &[ProDuty]) -> Vec<(u32, bool)> {
    let covered = pc::covered(c, p);
    let mut v: Vec<(i32, u32, bool)> = po::open_duties(p, ds).into_iter().filter(|o| !o.pieces.iter().all(|x| covered.contains(x))).map(|o| (o.start, o.n, false)).collect();
    for sp in pc::splits_on(c, &p.date) {
        let open = sp.parts.iter().any(|part| pc::part_pieces(p, part).iter().any(|&(ti, k)| p.tours[ti].duties[k].who.is_none()));
        if open {
            v.push((sp.parts.first().map_or(0, |x| x.from), sp.n, true));
        }
    }
    v.sort();
    v.into_iter().map(|x| (x.1, x.2)).collect()
}

/// A part of a cut duty as an open duty of its own (its pieces, when it signs on and off).
fn part_duty(c: &Company, p: &DayPlan, n: u32, part: &pc::Part) -> OpenDuty {
    let pieces = pc::part_pieces(p, part);
    let pr = &c.planning.flow.prefs;
    let mut lines: Vec<String> = Vec::new();
    for (ti, _) in &pieces {
        if !lines.contains(&p.tours[*ti].tour.number) {
            lines.push(p.tours[*ti].tour.number.clone());
        }
    }
    OpenDuty { n, start: part.from - pr.sign_on, end: part.to + pr.sign_off, paid: part.to - part.from + pr.sign_on + pr.sign_off, kind: pd::Kind::Whole, pieces, why: None, part: false, lines }
}

/// The trips that carry passengers of the pieces of `d` without a driver.
fn trips_open(p: &DayPlan, d: &OpenDuty) -> usize {
    pc::segs_of(p, &d.pieces.iter().copied().filter(|&(ti, k)| p.tours[ti].duties[k].who.is_none()).collect::<Vec<_>>()).iter().map(|x| x.trips(p)).sum()
}

/// What happens to a duty (or part) nobody drives: an agency driver at his price when the agency
/// setting is on - the server's admin's on the server (`live_pay::agency_runs_open`) -, else its
/// trips are dropped.
fn uncovered(c: &Company, p: &DayPlan, d: &OpenDuty) -> Value {
    if co::live_pay::agency_runs_open(c) {
        let cost = if p.date == c.date { po::agency_on_day(c, p, d) } else { co::agency::quote(c, 1, (d.end - d.start).max(60)) };
        json!({ "agency": trf("Agency driver %{cost}", &[("cost", &eur(cost))]) })
    } else {
        let n = trips_open(p, d);
        json!({ "drop": if n == 1 { tr("Without a driver 1 trip is dropped") } else { trf("Without a driver %{n} trips are dropped", &[("n", &n.to_string())]) } })
    }
}

/// The answers given for duty `n` of `date` (`part`), as known now: (driver, answer).
fn said_of(c: &Company, date: &str, n: u32, part: Option<u8>) -> Vec<(u32, Reply)> {
    let (game, now) = (co::clock::now(c), now());
    c.planning.ops.asks.iter().filter(|a| a.date == date && a.n == n && (a.part == part || a.part.is_none())).filter_map(|a| Some((a.of_staff()?, a.shown_at(game, now)?.clone()))).collect()
}

/// One driver of the box for duty `n` (`part`) of the plan `p`'s date: his kind and price, and the
/// one thing to do by the rules of asking - put on, ask (or wait for the colleague asked), waiting
/// for his answer, give it after a yes - with his answer in a box of its own.
fn row_json(c: &Company, p: &DayPlan, n: u32, part: Option<u8>, e: &Employee, tier: u8, price: (&Price, Cents)) -> Value {
    let (game, now) = (co::clock::now(c), now());
    let date = p.date.as_str();
    let (_, chip, _) = kind_of(c, e);
    let (words, over) = price_short(price.0, price.1);
    let rule = po::rule_of(c, e, date, tier);
    let ask = c.planning.ops.asks.iter().filter(|a| a.date == date && a.n == n && a.part == part && !a.closed && a.of_staff() == Some(e.id)).max_by_key(|a| a.id);
    let shown = ask.and_then(|a| a.shown_at(game, now));
    let mut v = json!({ "id": e.id, "nr": e.id, "name": e.name, "kind": kind_of(c, e).0, "sub": format!("{chip} · {words}"), "over": over });
    match (ask, shown) {
        (Some(a), None) => {
            v["action"] = json!("waiting");
            v["ask"] = json!(a.id);
        }
        (Some(_), Some(r)) => {
            v["answer"] = reply_json(c, r);
            v["action"] = json!(if r.yes() { "give" } else { "none" });
        }
        (None, _) if matches!(po::mood_of(c, e.id, game), po::Mood::Done { .. }) => {
            v["action"] = json!("done");
            v["hint"] = json!(tr("Answers no more questions today"));
        }
        (None, _) => match rule {
            Rule::Set => v["action"] = json!("set"),
            Rule::Many => v["action"] = json!("ask"),
            Rule::One => match po::open_one(c, date, n, part, game, now).and_then(|a| a.of_staff()) {
                Some(other) => {
                    v["action"] = json!("ask_off");
                    v["hint"] = json!(trf("ask him when %{name} has answered", &[("name", &first_name(c, other))]));
                }
                None => v["action"] = json!("ask"),
            },
        },
    }
    // (said no today already: irritated, angry - shown before he is asked again)
    match po::mood_of(c, e.id, game) {
        po::Mood::Calm => {}
        po::Mood::Irritated { at } => {
            v["mood"] = json!({ "key": "irritated", "text": trf("😒 Irritated · said no at %{time}", &[("time", &clock(at.rem_euclid(co::clock::DAY) as i32))]) });
            if v["action"] == "ask" {
                v["hint"] = json!(tr("Has already said no today"));
            }
        }
        po::Mood::Angry { .. } | po::Mood::Done { .. } => {
            v["mood"] = json!({ "key": "angry", "text": tr("😠 Angry") });
            if v["action"] == "ask" {
                v["hint"] = json!(tr("Has already said no twice today"));
            }
        }
    }
    v
}

/// "Thursday 05:30–13:45 · line 2".
fn duty_sub(d: &OpenDuty, date: &str) -> String {
    format!("{} {}–{} · {}", tr(FULL_DAYS[pt::day_of(date)]), clock(d.start), clock(d.end), trf("line %{l}", &[("l", &d.lines.join(", "))]))
}

/// The box's step 2 for open duty `d`: who can drive it (`rest`: the rest rules a driver would
/// break; `fair`: those given the fewest extra duties this month first, else the cheapest).
fn who_json(c: &Company, p: &DayPlan, d: &OpenDuty, weekly: &HashMap<u32, f64>, rest: &dyn Fn(&Employee, &OpenDuty) -> Vec<Breach>, fair: bool) -> Value {
    let date = p.date.as_str();
    let cands = po::candidates(c, p, d, &|id| weekly.get(&id).copied().unwrap_or(0.0));
    let mut groups: Vec<(&str, String, Vec<((bool, usize, Cents), Value)>)> = vec![("permanent", tr("Permanent"), Vec::new()), ("reserve", tr("Reserve frame"), Vec::new()), ("hourly", tr("Hourly paid"), Vec::new())];
    let mut cant = Vec::new();
    for x in &cands {
        let Some(e) = c.employee(x.id) else { continue };
        match &x.fit {
            Ok(()) => {
                let g = if x.tier == plan::TIER_RESERVE || matches!(e.employment, Employment::Reserve(_)) {
                    1
                } else if e.employment == Employment::Hourly {
                    2
                } else {
                    0
                };
                let mut row = row_json(c, p, d.n, None, e, x.tier, (&x.price, x.cost));
                let breaks = rest(e, d);
                with_breaches(c, &mut row, &e.name, &breaks);
                let (extras, words) = extra_words(c, e.id, date);
                row["extra"] = json!(words);
                // (those who would break the rest rules after the others; the fewest extra duties
                // first when the player sorts so)
                groups[g].2.push(((!breaks.is_empty(), if fair { extras } else { 0 }, x.cost), row));
            }
            Err(why) => cant.push(json!({ "name": e.name, "nr": e.id, "why": cannot_words(why) })),
        }
    }
    let groups: Vec<Value> = groups
        .into_iter()
        .filter(|g| !g.2.is_empty())
        .map(|(key, label, mut rows)| {
            rows.sort_by_key(|r| r.0);
            json!({ "key": key, "label": label, "rows": rows.into_iter().map(|r| r.1).collect::<Vec<_>>() })
        })
        .collect();
    let live = groups.iter().flat_map(|g| g["rows"].as_array().cloned().unwrap_or_default()).any(|r| r["action"] != "none");
    // (the players of "Play together" who drive, asked for a duty of the company's day only)
    let (game, now) = (co::clock::now(c), now());
    let players: Vec<Value> = if date == c.date {
        c.together
            .guests
            .iter()
            .filter(|g| !g.banned && g.perms().contains(&omsi_launcher_lib::company::together::Perm::Drive))
            .map(|g| {
                let a = c.planning.ops.asks.iter().filter(|a| a.date == date && a.n == d.n && a.part.is_none() && !a.closed && a.who == (Asked::Guest { no: g.no })).max_by_key(|a| a.id);
                let r = a.and_then(|a| a.shown_at(game, now));
                json!({ "no": g.no, "name": g.name, "action": match (a, r) { (None, _) => "ask", (Some(_), None) => "waiting", (Some(_), Some(r)) if r.yes() || r.key() == "text" => "give", _ => "none" }, "ask": a.map(|a| a.id), "answer": r.map(|r| reply_json(c, r)) })
            })
            .collect()
    } else {
        Vec::new()
    };
    let nobody = !live && !players.iter().any(|p| p["action"] != "none");
    let given = c.planning.ops.calls.iter().find(|x| x.date == date && x.n == d.n && x.part.is_none()).and_then(|x| c.employee(x.driver)).map(|e| e.name.clone());
    let mut v = json!({
        "n": d.n, "title": trf("Duty %{n}", &[("n", &d.n.to_string())]), "sub": duty_sub(d, date), "why": why_words(d.why), "kind": tr(d.kind.label()),
        "groups": groups, "cant": cant, "cant_label": trf("%{n} others cannot drive it", &[("n", &cant.len().to_string())]),
        "players": players, "given": given, "nobody": nobody, "can_cut": nobody && pc::can_cut_duty(c, p, d),
        "sort": { "value": if fair { "fair" } else { "cheap" }, "options": [["cheap", tr("Cheapest first")], ["fair", tr("Fewest extra first")]] },
    });
    if let Value::Object(m) = uncovered(c, p, d) {
        for (k, x) in m {
            v[k] = x;
        }
    }
    v
}

/// How a driver comes to a part, in words, and the note under it.
fn how_words(c: &Company, s: &pc::Sugg) -> (String, Option<String>) {
    let (price, _) = price_short(&s.price, s.cost);
    let e = c.employee(s.driver);
    let kind = e.map(|e| kind_of(c, e).1).unwrap_or_default();
    match &s.how {
        How::Continues { own, at, stop } => (
            format!("{} · {}", trf("goes on after %{own}", &[("own", &own.to_string())]), price),
            Some(trf("%{own} ends %{time} at %{stop}.", &[("own", &own.to_string()), ("time", &clock(*at)), ("stop", stop)])),
        ),
        How::Earlier { own, at, stop } => (
            format!("{} · {}", trf("begins before %{own}", &[("own", &own.to_string())]), price),
            Some(trf("%{own} begins %{time} at %{stop}.", &[("own", &own.to_string()), ("time", &clock(*at)), ("stop", stop)])),
        ),
        How::Said { late, time } => (format!("{kind} · {price}"), Some(if *late { trf("Said: can until %{time}.", &[("time", &clock(*time))]) } else { trf("Said: can from %{time}.", &[("time", &clock(*time))]) })),
        _ => (format!("{kind} · {price}"), None),
    }
}

/// The box's cut view for duty `n` cut for the plan `p`'s date: its parts, each with its driver
/// or one suggested.
fn cut_json(c: &Company, p: &DayPlan, ds: &[ProDuty], sp: &pc::Split, weekly: &HashMap<u32, f64>, rest: &dyn Fn(&Employee, &OpenDuty) -> Vec<Breach>) -> Value {
    let date = p.date.as_str();
    let mut skip: Vec<u32> = Vec::new();
    // (the drivers given parts first: not suggested for another)
    for part in &sp.parts {
        for (ti, k) in pc::part_pieces(p, part) {
            if let Some(plan::Who::Staff(id)) = p.tours[ti].duties[k].who {
                skip.push(id);
            }
        }
    }
    // (each part one driver: the parts a driver goes on or begins earlier for first, then the
    // rest in order)
    let weeks = |id: u32| weekly.get(&id).copied().unwrap_or(0.0);
    let segs_of_part = |part: &pc::Part| pc::segs_of(p, &pc::part_pieces(p, part));
    let mut order: Vec<(u8, usize)> = sp.parts.iter().enumerate().map(|(i, part)| (pc::suggest(c, p, &segs_of_part(part), ds, &weeks, &said_of(c, date, sp.n, Some(i as u8)), &skip).map_or(9, |s| s.how.rank()), i)).collect();
    order.sort();
    let mut chosen: HashMap<usize, pc::Sugg> = HashMap::new();
    for (_, i) in order {
        let part = &sp.parts[i];
        if pc::part_pieces(p, part).iter().any(|&(ti, k)| p.tours[ti].duties[k].who.is_some()) {
            continue;
        }
        if let Some(s) = pc::suggest(c, p, &segs_of_part(part), ds, &weeks, &said_of(c, date, sp.n, Some(i as u8)), &skip) {
            skip.push(s.driver);
            chosen.insert(i, s);
        }
    }
    let total: i32 = sp.parts.iter().map(|x| x.to - x.from).sum();
    let mut driven = 0;
    let parts: Vec<Value> = sp
        .parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            let od = part_duty(c, p, sp.n, part);
            let letter = pc::letter(i);
            let mut v = json!({ "i": i, "letter": letter.to_string(), "title": format!("{letter} · {}–{}", clock(part.from), clock(part.to)), "from": part.from, "to": part.to });
            let who = od.pieces.iter().find_map(|&(ti, k)| p.tours[ti].duties[k].who);
            if let Some(w) = who {
                driven += part.to - part.from;
                let name = match w {
                    plan::Who::Staff(id) => c.employee(id).map(|e| e.name.clone()).unwrap_or_default(),
                    plan::Who::Guest(no) => c.together.name_of(no),
                    _ => tr("Agency driver"),
                };
                v["given"] = json!(name);
                return v;
            }
            match chosen.get(&i) {
                Some(s) => {
                    let Some(e) = c.employee(s.driver) else { return v };
                    let (how, note) = how_words(c, s);
                    let mut row = row_json(c, p, sp.n, Some(i as u8), e, s.tier, (&s.price, s.cost));
                    row["sub"] = json!(how);
                    row["note"] = json!(note);
                    with_breaches(c, &mut row, &e.name, &rest(e, &od));
                    row["extra"] = json!(extra_words(c, e.id, date).1);
                    v["row"] = row;
                }
                None => {
                    v["none"] = json!(tr("Nobody free"));
                    if let Value::Object(m) = uncovered(c, p, &od) {
                        for (k, x) in m {
                            v[k] = x;
                        }
                    }
                }
            }
            v
        })
        .collect();
    let at: Vec<String> = sp
        .parts
        .iter()
        .skip(1)
        .filter_map(|part| {
            let (ti, k) = *pc::part_pieces(p, part).first()?;
            let t = &p.tours[ti];
            Some(format!("{} {}", t.tour.trips[t.duties[k].start].from.trim(), clock(part.from)))
        })
        .collect();
    let first = sp.parts.first().map_or(0, |x| x.from);
    let last = sp.parts.last().map_or(0, |x| x.to);
    json!({
        "n": sp.n, "title": trf("Cut duty %{n}", &[("n", &sp.n.to_string())]),
        "sub": trf("%{time} · cut at %{at}", &[("time", &format!("{}–{}", clock(first), clock(last))), ("at", &at.join(&format!(" {} ", tr("and"))))]),
        "parts": parts, "first": first, "last": last,
        "driven": trf("Driven: %{a} of %{b}", &[("a", &length(driven)), ("b", &length(total))]),
    })
}

/// The box and the daily operations of `date` (`rf_ops`: `date`, the company's day when none; `n`
/// the duty chosen - step 2 -, none: step 1).
fn daily(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({ "none": true })) };
    if let Some(v) = off(&c) {
        return Ok(v);
    }
    let date = Some(str_arg(args, "date")).filter(|d| co::dates::parse(d).is_some() && co::dates::between(&c.date, d) >= 0).unwrap_or_else(|| c.date.clone());
    let mut out = json!({
        "date": date, "label": day_label(&date), "heading": format!("{} {}", tr(FULL_DAYS[pt::day_of(&date)]), short_date(&date)), "today": date == c.date,
        "prev": (co::dates::between(&c.date, &date) > 0).then(|| co::dates::add(&date, -1)), "next": co::dates::add(&date, 1),
    });
    let week = match super::turnus::week_duties(s, r) {
        None => {
            out["reading"] = json!(tr("Reading the timetable…"));
            return Ok(out);
        }
        Some(Err(e)) => {
            out["reading"] = json!(e);
            return Ok(out);
        }
        Some(Ok(w)) => w,
    };
    let mut need = po::week_dates(&date);
    // (the days before and after: the rest rules)
    for x in [co::dates::add(&date, -1), date.clone(), co::dates::add(&date, 1)] {
        if !need.contains(&x) {
            need.push(x);
        }
    }
    let plans = match plans_of(s, r, &need) {
        Ok(p) => p,
        Err(e) => {
            out["reading"] = json!(e);
            return Ok(out);
        }
    };
    let Some(p) = plans.get(&date) else { return Ok(out) };
    let ds = duties_for(&c, &week, p);
    let open = po::open_duties(p, &ds);
    let items = open_items(&c, p, &ds);
    out["open"] = json!(items
        .iter()
        .filter_map(|&(n, cut)| {
            if cut {
                let sp = pc::split_of(&c, &date, n)?;
                let left = sp.parts.iter().filter(|part| pc::part_pieces(p, part).iter().any(|&(ti, k)| p.tours[ti].duties[k].who.is_none())).count();
                let (from, to) = (sp.parts.first()?.from, sp.parts.last()?.to);
                return Some(json!({ "n": n, "cut": true, "label": n.to_string(), "time": format!("{}–{}", clock(from), clock(to)), "why": trf("cut in %{n} parts · %{left} without a driver", &[("n", &sp.parts.len().to_string()), ("left", &left.to_string())]) }));
            }
            let o = open.iter().find(|o| o.n == n)?;
            let mut why = format!("{} · {}", trf("line %{l}", &[("l", &o.lines.join(", "))]), why_words(o.why));
            if o.kind == pd::Kind::Split {
                why = format!("{} · {}", tr("split duty"), why);
            }
            let time = if o.kind == pd::Kind::Split { halves(p, &o.pieces, c.planning.flow.prefs.sign_on).unwrap_or_else(|| format!("{}–{}", clock(o.start), clock(o.end))).replace(" / ", " + ") } else { format!("{}–{}", clock(o.start), clock(o.end)) };
            Some(json!({ "n": n, "cut": false, "label": n.to_string(), "time": time, "why": why, "split": o.kind == pd::Kind::Split }))
        })
        .collect::<Vec<_>>());
    out["count"] = json!({ "duties": ds.len(), "open": items.len(), "text": match items.len() { 0 => tr("Every duty has a driver."), 1 => tr("1 duty is missing a driver"), n => trf("%{n} duties are missing a driver", &[("n", &n.to_string())]) } });
    // the frames of the day, and who is free all day
    out["frames"] = json!(plan::frames_on(&c, &date)
        .iter()
        .map(|(e, f)| {
            let day = po::driver_day(&c, e, p, &ds);
            let used = matches!(day.is, DayIs::Duties { .. });
            json!({ "id": e.id, "nr": e.id, "name": e.name, "time": format!("{}–{}", clock(f.from), clock(f.to)), "used": used, "what": cell_json(&day, &ds, None, None)["label"] })
        })
        .collect::<Vec<_>>());
    out["free"] = json!(drivers(&c)
        .into_iter()
        .filter(|e| po::driver_day(&c, e, p, &ds).is == DayIs::Off)
        .map(|e| {
            let (key, chip, _) = kind_of(&c, e);
            json!({ "id": e.id, "nr": e.id, "name": e.name, "kind": key, "chip": chip })
        })
        .collect::<Vec<_>>());
    // step 2: the duty chosen - who can drive it, or its cut
    let Some(n) = args.get("n").and_then(Value::as_u64).map(|x| x as u32) else { return Ok(out) };
    let weekly = week_of_drivers(&c, &week, &plans, &date);
    let rest = |e: &Employee, d: &OpenDuty| breaches_of(&c, &week, &plans, e, &date, d, &weekly);
    if let Some(sp) = pc::split_of(&c, &date, n) {
        out["cut"] = cut_json(&c, p, &ds, sp, &weekly, &rest);
        out["waiting"] = json!(c.planning.ops.asks.iter().any(|a| a.date == date && a.n == n && !a.closed && a.reply.is_some() && a.shown_at(co::clock::now(&c), now()).is_none()));
        return Ok(out);
    }
    let Some(d) = open.iter().find(|o| o.n == n) else { return Ok(out) };
    out["duty"] = who_json(&c, p, d, &weekly, &rest, str_arg(args, "sort") == "fair");
    out["waiting"] = json!(po::asks_of(&c, &date, d.n).any(|a| a.reply.is_some() && a.shown_at(co::clock::now(&c), now()).is_none()));
    Ok(out)
}

fn said(text: impl Into<String>, error: bool) -> Value {
    json!({ "say": { "text": text.into(), "error": error } })
}

/// What an AI driver's answer is decided from: his day in the plan `p` against open duty `d`.
fn situation(c: &Company, p: &DayPlan, ds: &[ProDuty], d: &OpenDuty, e: &Employee, cand: Option<&po::Cand>) -> po::Situation {
    let fit = cand.map_or_else(|| po::fit_of(c, p, e, &d.pieces), |x| x.fit.clone());
    let in_frame = cand.is_some_and(|x| x.tier == plan::TIER_RESERVE);
    // (his own duty that day, and a free colleague who could take it)
    let own = match po::driver_day(c, e, p, ds).is {
        DayIs::Duties { numbers, start, end, .. } => numbers.first().copied().filter(|n| *n > 0).map(|n| (n, start, end)),
        _ => None,
    };
    let swap_with = own.and_then(|(n, _, _)| {
        let mine = ds.iter().find(|x| x.number == n)?;
        let pieces = po::pieces_of(p, mine);
        drivers(c).into_iter().filter(|x| x.id != e.id && p.duties_of(plan::Who::Staff(x.id)).is_empty()).find(|x| po::fit_of(c, p, x, &pieces).is_ok()).map(|x| x.id)
    });
    let from = d.pieces.iter().map(|&(ti, k)| p.tours[ti].duties[k].from).min().unwrap_or(d.start);
    let to = d.pieces.iter().map(|&(ti, k)| p.tours[ti].duties[k].to).max().unwrap_or(d.end);
    let (can_from, can_until) = po::could(p, e, &d.pieces, from, to);
    po::Situation { fit, in_frame, week: cand.map_or(0.0, |x| x.week), contract: po::contract_minutes(c, e), own, swap_with, can_from, can_until }
}

/// The duty (or part of a cut duty) of an act: the plans, the date's duties, it as an open duty,
/// the drivers' weeks, and the rest rules it would break for each - or what to say.
#[allow(clippy::type_complexity)]
fn target(s: &mut Session, r: &mut Rf, c: &Company, date: &str, n: u32, part: Option<u8>) -> Result<(DayPlan, Vec<ProDuty>, OpenDuty, HashMap<u32, f64>, HashMap<u32, Vec<Breach>>), Value> {
    let week = match super::turnus::week_duties(s, r) {
        Some(Ok(w)) => w,
        Some(Err(e)) => return Err(said(e, true)),
        None => return Err(said(tr("Reading the timetable…"), false)),
    };
    let mut need = po::week_dates(date);
    for x in [co::dates::add(date, -1), date.to_string(), co::dates::add(date, 1)] {
        if !need.contains(&x) {
            need.push(x);
        }
    }
    let plans = plans_of(s, r, &need).map_err(|e| said(e, true))?;
    let p = plans.get(date).cloned().ok_or_else(|| json!({}))?;
    let ds = duties_for(c, &week, &p);
    let d = match part {
        Some(i) => {
            let sp = pc::split_of(c, date, n).ok_or_else(|| said(tr(pc::NO_SUCH_PART), true))?;
            let part = sp.parts.get(i as usize).ok_or_else(|| said(tr(pc::NO_SUCH_PART), true))?;
            part_duty(c, &p, n, part)
        }
        None => po::open_duties(&p, &ds).into_iter().find(|o| o.n == n).ok_or_else(|| said(tr("That duty has a driver now."), true))?,
    };
    let weekly = week_of_drivers(c, &week, &plans, date);
    let breaks = drivers(c).into_iter().map(|e| (e.id, breaches_of(c, &week, &plans, e, date, &d, &weekly))).collect();
    Ok((p, ds, d, weekly, breaks))
}

/// What a click of the box or the daily operations does (`rf_ops_act`: `act` "ask" | "give" |
/// "give_guest" | "withdraw_ask" | "withdraw" | "cut" | "uncut" | "agency"; `date`, `n`, `part`
/// for a part of a cut duty, and what it needs).
fn act(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let what = str_arg(args, "act");
    let date = str_arg(args, "date");
    let n = args.get("n").and_then(Value::as_u64).unwrap_or(0) as u32;
    let part = args.get("part").and_then(Value::as_u64).map(|x| x as u8);
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    if let Some(v) = off(&c) {
        return Ok(v);
    }
    if co::dates::parse(&date).is_none() || co::dates::between(&c.date, &date) < 0 {
        return Ok(said(tr("That day has gone."), true));
    }
    let edit = |s: &mut Session, words: String, f: &mut dyn FnMut(&mut Company) -> Result<(), &'static str>| -> Value {
        let mut refused = None;
        let out = company::act(s, |c| {
            f(c).map_err(|e| {
                refused = Some(e);
                e
            })
        });
        match (out, refused) {
            (Ok(()), _) => {
                // (the day's plans made anew at once, not at the next tick)
                s.plan_gen += 1;
                let mut v = said(words, false);
                v["edited"] = json!(true);
                v
            }
            (Err(_), Some(e)) => said(tr(e), true),
            (Err(_), None) => json!({}),
        }
    };
    Ok(match what.as_str() {
        "ask" => {
            let (p, ds, d, weekly, _) = match target(s, r, &c, &date, n, part) {
                Ok(x) => x,
                Err(v) => return Ok(v),
            };
            let cands = po::candidates(&c, &p, &d, &|id| weekly.get(&id).copied().unwrap_or(0.0));
            let ids: Vec<u32> = args.get("drivers").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as u32).collect()).unwrap_or_default();
            let guests: Vec<u32> = args.get("guests").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as u32).collect()).unwrap_or_default();
            let mut who: Vec<(Asked, Option<Reply>)> = Vec::new();
            for id in &ids {
                let Some(e) = c.employee(*id) else { continue };
                let sit = situation(&c, &p, &ds, &d, e, cands.iter().find(|x| x.id == *id));
                who.push((Asked::Staff { id: *id }, Some(po::ai_reply(&c, e, &date, d.n, d.start, d.end, &sit))));
            }
            if date == c.date {
                who.extend(guests.iter().filter(|g| c.together.guest(**g).is_some_and(|x| !x.banned)).map(|g| (Asked::Guest { no: *g }, None)));
            }
            let words = match ids.as_slice() {
                [one] => trf("%{name} is asked for duty %{duty}: the answer comes in here.", &[("name", &name_in(&c)(*one)), ("duty", &d.n.to_string())]),
                _ => trf("%{n} asked for duty %{duty}: their answers come in here.", &[("n", &who.len().to_string()), ("duty", &d.n.to_string())]),
            };
            let at = now();
            edit(s, words, &mut |c| po::ask_part(c, &date, d.n, part, d.start, d.end, who.clone(), at, &|id| weekly.get(&id).copied().unwrap_or(0.0)).map(|_| ()))
        }
        "give" => {
            let (p, _, d, weekly, breaks) = match target(s, r, &c, &date, n, part) {
                Ok(x) => x,
                Err(v) => return Ok(v),
            };
            let driver = args.get("driver").and_then(Value::as_u64).unwrap_or(0) as u32;
            let swap = args.get("swap").filter(|x| x.is_object()).and_then(|x| Some((x.get("with")?.as_u64()? as u32, x.get("theirs")?.as_u64()? as u32)));
            let Some(e) = c.employee(driver).cloned() else { return Ok(said(tr(po::NOT_A_DRIVER), true)) };
            // (put on without a question: a reserve whose frame holds it, before noon the day before)
            let tier = po::candidates(&c, &p, &d, &|id| weekly.get(&id).copied().unwrap_or(0.0)).iter().find(|x| x.id == driver).map_or(plan::TIER_COLLEAGUE, |x| x.tier);
            let set = po::rule_of(&c, &e, &date, tier) == Rule::Set;
            // (the rest rules broken: only after the player confirmed it - `anyway`)
            let broken = !breaks.get(&driver).map_or(true, Vec::is_empty);
            if broken && args.get("anyway").and_then(Value::as_bool) != Some(true) {
                return Ok(said(tr("This breaks the rest rules: confirm it first."), true));
            }
            // (a reserve within his frame is paid anyway: no extra duty)
            let extra = tier != plan::TIER_RESERVE;
            let label = match part {
                Some(i) => format!("{}{}", n, pc::letter(i as usize)),
                None => n.to_string(),
            };
            let words = match swap {
                Some((with, theirs)) => trf("Duty %{n} is %{name}'s on %{day}, and his %{theirs} %{with}'s.", &[("n", &label), ("name", &e.name), ("day", &day_label(&date)), ("theirs", &theirs.to_string()), ("with", &name_in(&c)(with))]),
                None => trf("Duty %{n} is %{name}'s on %{day}.", &[("n", &label), ("name", &e.name), ("day", &day_label(&date))]),
            };
            let at = now();
            edit(s, words, &mut |c| {
                po::give_asked(c, &date, n, part, driver, swap, set, extra, at)?;
                if broken {
                    pr::broke(c, driver);
                }
                Ok(())
            })
        }
        // (a player who said yes: the duty of the company's day his - `together::book::give`)
        "give_guest" => {
            let guest = args.get("guest").and_then(Value::as_u64).unwrap_or(0) as u32;
            if date != c.date {
                return Ok(said(tr("A player can only be given a duty of the company's day."), true));
            }
            let (p, _, d, _, _) = match target(s, r, &c, &date, n, None) {
                Ok(x) => x,
                Err(v) => return Ok(v),
            };
            let words = trf("Duty %{n} is %{name}'s today: he drives it in his own game.", &[("n", &n.to_string()), ("name", &c.together.name_of(guest))]);
            edit(s, words, &mut |c| {
                let now = co::clock::now(c);
                for &(ti, k) in &d.pieces {
                    if p.tours[ti].duties[k].who.is_some() {
                        continue;
                    }
                    if let Some(x) = co::duty_launch::DutyRef::of(&p, ti, k) {
                        co::together::book::give(c, &p, &x, guest, now)?;
                    }
                }
                po::close(c, &date, n);
                Ok(())
            })
        }
        "withdraw_ask" => {
            let id = args.get("ask").and_then(Value::as_u64).unwrap_or(0) as u32;
            let mut changed = false;
            let v = edit(s, tr("The question is taken back."), &mut |c| {
                changed = po::withdraw_ask(c, id);
                Ok(())
            });
            if changed {
                v
            } else {
                json!({})
            }
        }
        "withdraw" => {
            let mut changed = false;
            let v = edit(s, trf("Duty %{n} runs as planned again.", &[("n", &n.to_string())]), &mut |c| {
                changed = po::withdraw(c, &date, n);
                Ok(())
            });
            if changed {
                v
            } else {
                json!({})
            }
        }
        "cut" => {
            let (p, ds, d, weekly, _) = match target(s, r, &c, &date, n, None) {
                Ok(x) => x,
                Err(v) => return Ok(v),
            };
            let said = said_of(&c, &date, n, None);
            let prop = pc::propose(&c, &p, &d, &ds, &|id| weekly.get(&id).copied().unwrap_or(0.0), &said);
            let words = trf("Duty %{n} is cut in %{parts} parts for %{day}.", &[("n", &n.to_string()), ("parts", &prop.parts.len().to_string()), ("day", &day_label(&date))]);
            edit(s, words, &mut |c| pc::cut(c, &p, &d, &prop))
        }
        "uncut" => {
            let mut changed = false;
            let v = edit(s, trf("Duty %{n} is whole again.", &[("n", &n.to_string())]), &mut |c| {
                changed = pc::uncut(c, &date, n);
                Ok(())
            });
            if changed {
                v
            } else {
                json!({})
            }
        }
        // (an agency driver for it: only when the agency setting is on - today the dispatcher's,
        // on a later day one booked)
        "agency" => {
            if !co::live_pay::agency_runs_open(&c) {
                return Ok(said(tr("Agency drivers are not used here: duties nobody covers are dropped."), true));
            }
            let (p, _, d, _, _) = match target(s, r, &c, &date, n, part) {
                Ok(x) => x,
                Err(v) => return Ok(v),
            };
            let open: Vec<(usize, usize)> = d.pieces.iter().copied().filter(|&(ti, k)| p.tours[ti].duties[k].who.is_none()).collect();
            let tours: Vec<co::network::TourOfDay> = p.tours.iter().map(|t| t.tour.clone()).collect();
            let words = trf("An agency driver drives %{n} on %{day}.", &[("n", &n.to_string()), ("day", &day_label(&date))]);
            edit(s, words, &mut |c| {
                if date == c.date {
                    for &(ti, k) in &open {
                        let t = &p.tours[ti].tour;
                        plan::set_fill(c, &plan::duty_key(&t.line, &t.tour, k), Some(plan::Fill::Agency));
                    }
                    Ok(())
                } else {
                    let pieces: Vec<_> = open.iter().filter_map(|&(ti, k)| Some((co::studio::piece_ref(&tours, ti, k)?, plan::duty_block(&p, ti, k)))).collect();
                    let from = pieces.iter().map(|x| x.1.from).min().unwrap_or(0) - 30;
                    let to = pieces.iter().map(|x| x.1.to).max().unwrap_or(0) + 30;
                    co::agency::book(c, &date, 1, Some((from.max(0), to.max(from + 60))), &pieces).map(|_| ())
                }
            })
        }
        _ => json!({}),
    })
}

/// The commands of the two tabs (`super::run`).
pub(super) fn run(s: &mut Session, r: &mut Rf, cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "rf_cal" => calendar(s, r, args),
        "rf_ops" => daily(s, r, args),
        "rf_ops_act" => act(s, r, args),
        _ => return None,
    })
}

/// Every English text this file hands to `tr`/`trf` besides its literals.
#[cfg(test)]
pub(super) fn texts() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = DAYS.to_vec();
    v.extend(FULL_DAYS);
    v.extend(ANSWERS.iter().map(|a| a.1));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::autopilot;
    use omsi_launcher_lib::company::model::{Frame, Planner};
    use omsi_launcher_lib::company::pro_turnus::Cell;

    /// Line 5's company planning with RosterFlow Pro, its week read, three drivers.
    fn session() -> Session {
        let (mut c, l) = super::super::tests::line5_company();
        assert_eq!(autopilot::set_planner(&mut c, Planner::Pro), Planner::Pro);
        while c.staff.iter().filter(|e| co::staff::drives(e)).count() < 3 {
            for a in co::staff::applicants(&c) {
                if a.role == co::model::Role::Driver && c.staff.iter().filter(|e| co::staff::drives(e)).count() < 3 {
                    let _ = co::staff::hire(&mut c, &a);
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        for e in c.staff.iter_mut() {
            (e.sick_until, e.holiday_until, e.hired) = (None, None, "2024-01-01".into());
        }
        let mut s = Session::default();
        s.company = Some(c);
        company::test_week(&mut s, vec![l]);
        s
    }

    /// A change of the company made here: saved, the day's plans made anew.
    fn changed(s: &mut Session) {
        company::changed(s);
        s.plan_gen += 1;
    }

    #[test]
    fn the_calendar_and_the_box_show_ask_and_give_a_duty() {
        company::words::set_language("en");
        company::test_dry();
        let mut s = session();
        let mut r = Rf::default();
        let ids: Vec<u32> = s.company.as_ref().unwrap().staff.iter().filter(|e| co::staff::drives(e)).map(|e| e.id).collect();
        // the calendar: a week from Sunday, every driver a row with his number and employment
        let v = calendar(&mut s, &mut r, &json!({})).unwrap();
        assert!(v.get("reading").is_none(), "{v}");
        let days = v["days"].as_array().unwrap();
        assert_eq!(days.len(), 7);
        assert_eq!(days[0]["wd"], "Sun");
        assert_eq!(days[0]["name"], "Sunday");
        assert_eq!(days[0]["date"], "2024-03-03");
        assert_eq!(days[1]["today"], true);
        assert_eq!(days[0]["past"], true);
        let rows = v["drivers"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["nr"], ids[0]);
        assert_eq!(rows[0]["group"], "permanent");
        assert_eq!(rows[0]["what"], "full time 39 h");
        assert_eq!(rows[0]["cells"].as_array().map(Vec::len), Some(7));
        assert_eq!(rows[0]["cells"][2]["label"], "–", "a day off");
        assert_eq!(rows[0]["hours"][0]["of"], "39");
        // nobody planned: every duty of a day missing
        let missing = days[2]["missing"].as_u64().unwrap();
        assert!(missing >= 2, "{}", days[2]);
        assert_eq!(days[2]["missing_text"], "duties missing");
        // 14 days, a month
        assert_eq!(calendar(&mut s, &mut r, &json!({ "span": "two" })).unwrap()["days"].as_array().map(Vec::len), Some(14));
        assert_eq!(calendar(&mut s, &mut r, &json!({ "span": "month", "week": 2 })).unwrap()["days"].as_array().map(Vec::len), Some(35));
        // a reserve and an hourly-paid driver
        {
            let c = s.company.as_mut().unwrap();
            c.staff.iter_mut().find(|e| e.id == ids[1]).unwrap().employment = Employment::Reserve(Frame::new(300, 900, 0x1f));
            c.staff.iter_mut().find(|e| e.id == ids[2]).unwrap().employment = Employment::Hourly;
        }
        changed(&mut s);
        let v = calendar(&mut s, &mut r, &json!({ "kind": "hourly" })).unwrap();
        assert_eq!(v["drivers"].as_array().map(Vec::len), Some(1));
        assert_eq!(v["drivers"][0]["group"], "hourly");
        let v = calendar(&mut s, &mut r, &json!({})).unwrap();
        assert_eq!(v["drivers"][1]["chip"], "Reserve frame");
        assert_eq!(v["drivers"][1]["cells"][2]["kind"], "frame", "his frame on Tuesday: {}", v["drivers"][1]["cells"][2]);
        // the box's step 1 for Tuesday: the open duties, the frame, who is free
        let date = "2024-03-05";
        let v = daily(&mut s, &mut r, &json!({ "date": date })).unwrap();
        assert!(v.get("reading").is_none(), "{v}");
        let open = v["open"].as_array().unwrap();
        assert!(open.len() >= 2, "{v}");
        assert!(v.get("duty").is_none(), "step 1 only");
        assert_eq!(v["frames"][0]["id"], ids[1]);
        let n = open[0]["n"].as_u64().unwrap();
        // step 2: who can drive it - the permanent driver first, the reserve, the hourly paid
        let v = daily(&mut s, &mut r, &json!({ "date": date, "n": n })).unwrap();
        let groups = v["duty"]["groups"].as_array().unwrap();
        assert_eq!(groups.iter().map(|g| g["key"].as_str().unwrap()).collect::<Vec<_>>(), ["permanent", "reserve", "hourly"], "{groups:?}");
        // the reserve within his frame, before noon the day before: put on without a question
        let res = &groups[1]["rows"][0];
        assert_eq!((res["id"].as_u64(), res["action"].as_str()), (Some(ids[1] as u64), Some("set")), "{res}");
        assert_eq!(res["sub"], "Reserve frame · nothing extra");
        let hourly = &groups[2]["rows"][0];
        assert_eq!(hourly["action"], "ask");
        assert!(hourly["sub"].as_str().unwrap().starts_with("Hourly · "), "{hourly}");
        // nobody for it: off the server, no agency chosen - its trips are dropped
        assert!(v["duty"]["drop"].as_str().unwrap().starts_with("Without a driver"), "{}", v["duty"]);
        assert!(v["duty"].get("agency").is_none());
        assert_eq!(v["duty"]["nobody"], false);
        // the hourly driver asked: waiting for his answer; given before it: refused
        let out = act(&mut s, &mut r, &json!({ "act": "ask", "date": date, "n": n, "drivers": [ids[2]] })).unwrap();
        assert_eq!(out["edited"], true, "{out}");
        let v = daily(&mut s, &mut r, &json!({ "date": date, "n": n })).unwrap();
        assert_eq!(v["duty"]["groups"][2]["rows"][0]["action"], "waiting");
        assert_eq!(v["waiting"], true);
        let out = act(&mut s, &mut r, &json!({ "act": "give", "date": date, "n": n, "driver": ids[2] })).unwrap();
        assert_eq!(out["say"]["error"], true, "{out}");
        // put on: the reserve's that day, the duty no longer open
        let out = act(&mut s, &mut r, &json!({ "act": "give", "date": date, "n": n, "driver": ids[1] })).unwrap();
        assert_eq!(out["edited"], true, "{out}");
        let v = daily(&mut s, &mut r, &json!({ "date": date })).unwrap();
        assert!(!v["open"].as_array().unwrap().iter().any(|o| o["n"] == n), "{}", v["open"]);
        let v = calendar(&mut s, &mut r, &json!({})).unwrap();
        assert_eq!(v["drivers"][1]["cells"][2]["kind"], "duty", "{}", v["drivers"][1]["cells"][2]);
        assert_eq!(v["drivers"][1]["cells"][2]["numbers"][0], n);
        // taken back: open again
        act(&mut s, &mut r, &json!({ "act": "withdraw", "date": date, "n": n })).unwrap();
        let v = daily(&mut s, &mut r, &json!({ "date": date })).unwrap();
        assert!(v["open"].as_array().unwrap().iter().any(|o| o["n"] == n));
        // the agency only when its setting is on: refused off
        assert_eq!(act(&mut s, &mut r, &json!({ "act": "agency", "date": date, "n": n })).unwrap()["say"]["error"], true);
        // a day gone: refused
        assert_eq!(act(&mut s, &mut r, &json!({ "act": "give", "date": "2024-03-01", "n": n, "driver": ids[1] })).unwrap()["say"]["error"], true);
        // a rota's driver ill that day: his duty open as the illness
        let week = super::super::turnus::week_duties(&mut s, &mut r).unwrap().unwrap();
        {
            let c = s.company.as_mut().unwrap();
            let id = pt::add(c, "Nord", false);
            pt::add_row(c, id).unwrap();
            pt::set_cell(c, id, 0, 2, Cell::Duty { n: week[2][0].number }, &week).unwrap();
            pt::set_driver(c, id, 0, Some(ids[0])).unwrap();
            pt::set_exception(c, ids[0], date, Some((Cell::Off, "sick".into())));
        }
        changed(&mut s);
        let v = daily(&mut s, &mut r, &json!({ "date": date, "n": week[2][0].number })).unwrap();
        assert!(v["duty"]["sub"].as_str().unwrap().starts_with("Tuesday "), "{}", v["duty"]);
        assert_eq!(v["duty"]["why"], "Ill");
        let v = calendar(&mut s, &mut r, &json!({})).unwrap();
        assert_eq!(v["drivers"][0]["cells"][2]["kind"], "ill");
        // without RosterFlow Pro: nothing to show
        autopilot::set_planner(s.company.as_mut().unwrap(), Planner::RosterFlow);
        assert_eq!(calendar(&mut s, &mut r, &json!({})).unwrap()["off"], true);
        assert_eq!(daily(&mut s, &mut r, &json!({})).unwrap()["off"], true);
    }

    #[test]
    fn the_box_warns_of_the_rest_rules_asks_first_and_counts_the_extra_duties() {
        company::words::set_language("en");
        company::test_dry();
        let mut s = session();
        let mut r = Rf::default();
        let ids: Vec<u32> = s.company.as_ref().unwrap().staff.iter().filter(|e| co::staff::drives(e)).map(|e| e.id).collect();
        // a permanent driver who drives the mornings Monday to Friday by his rota
        let week = super::super::turnus::week_duties(&mut s, &mut r).unwrap().unwrap();
        {
            let c = s.company.as_mut().unwrap();
            let id = pt::add(c, "Nord", false);
            pt::add_row(c, id).unwrap();
            for d in 1..=5 {
                pt::set_cell(c, id, 0, d, Cell::Duty { n: week[d][0].number }, &week).unwrap();
            }
            pt::set_driver(c, id, 0, Some(ids[0])).unwrap();
        }
        changed(&mut s);
        // Saturday: one more duty would make his week over 48 hours - said in red, "anyway"
        let date = "2024-03-09";
        let v = daily(&mut s, &mut r, &json!({ "date": date })).unwrap();
        let n = v["open"][0]["n"].as_u64().unwrap();
        let v = daily(&mut s, &mut r, &json!({ "date": date, "n": n })).unwrap();
        let row = v["duty"]["groups"].as_array().unwrap().iter().flat_map(|g| g["rows"].as_array().unwrap().clone()).find(|x| x["id"] == ids[0]).unwrap();
        assert_eq!(row["breach"][0], "⚠ Over 48 h in the week", "{row}");
        assert_eq!(row["action"], "ask", "asked as ever: {row}");
        // (he said yes: "give anyway")
        {
            let c = s.company.as_mut().unwrap();
            po::ask_part(c, date, n as u32, None, 300, 900, vec![(Asked::Staff { id: ids[0] }, Some(Reply::Yes { say: 0 }))], 0, &|_| 0.0).unwrap();
            let game = co::clock::now(c);
            c.planning.ops.asks.last_mut().unwrap().due = Some(game - 1);
        }
        changed(&mut s);
        let v = daily(&mut s, &mut r, &json!({ "date": date, "n": n })).unwrap();
        let row = v["duty"]["groups"].as_array().unwrap().iter().flat_map(|g| g["rows"].as_array().unwrap().clone()).find(|x| x["id"] == ids[0]).unwrap();
        assert_eq!(row["action"], "give_anyway", "{row}");
        assert!(row["anyway"].as_str().unwrap().contains("breaks the rest rules"), "{row}");
        assert_eq!(row["extra"], "0 extra this month");
        assert_eq!(v["duty"]["sort"]["value"], "cheap");
        assert_eq!(daily(&mut s, &mut r, &json!({ "date": date, "n": n, "sort": "fair" })).unwrap()["duty"]["sort"]["value"], "fair");
        // not confirmed: refused; confirmed: his, a little less content and more stressed
        let out = act(&mut s, &mut r, &json!({ "act": "give", "date": date, "n": n, "driver": ids[0] })).unwrap();
        assert_eq!(out["say"]["error"], true, "{out}");
        let before = s.company.as_ref().unwrap().employee(ids[0]).unwrap().clone();
        let out = act(&mut s, &mut r, &json!({ "act": "give", "date": date, "n": n, "driver": ids[0], "anyway": true })).unwrap();
        assert_eq!(out["edited"], true, "{out}");
        let after = s.company.as_ref().unwrap().employee(ids[0]).unwrap().clone();
        assert_eq!(after.satisfaction, (before.satisfaction - pr::BREAK_COST).clamp(0.0, 100.0));
        assert!(after.stress > before.stress);
        // (a day off of his: an extra duty, counted for the month and shown)
        assert_eq!(pr::extras_in_month(s.company.as_ref().unwrap(), ids[0], date), 1);
    }
}

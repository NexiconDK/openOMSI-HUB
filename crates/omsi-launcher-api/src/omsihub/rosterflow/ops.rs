//! RosterFlow Pro's tabs "4 Daglig drift" and "5 Kalender" (step 4, NexiconDK 4 November 2026; the
//! core's `company::pro_ops`):
//!
//! - the calendar (`rf_cal`): every driver a row - his number ("chf."), his employment (permanent,
//!   a reserve's frame, hourly paid) -, every day of the week from Sunday his duty and when he signs
//!   on, a day off, ill, on holiday or a frame, and the hours of his week; above them each day's
//!   duties without a driver. A week, 14 days or a month.
//! - the daily operations of a date (`rf_ops`): its duties without a driver and why, the reserves'
//!   frames and who is free; for the duty chosen every driver in the order it is filled in, whether
//!   he can and what he costs (overtime orange), the drivers asked and their answers.
//! - their acts (`rf_ops_act`): ask the drivers ticked, give the duty to one (a swap with it),
//!   take it back.
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
use omsi_launcher_lib::company::pro_ops::{self as po, Asked, DayIs, OpenDuty, Price, Reply};
use omsi_launcher_lib::company::pro_turnus::{self as pt, ProDuty, Week};
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};
use std::collections::HashMap;

/// The days of a week, Sunday first (translated when shown).
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The answers a driver can give, as the page lists them: their key and words.
const ANSWERS: [(&str, &str); 6] = [("yes", "Yes"), ("no", "No"), ("early", "Too early"), ("late", "Too late"), ("swap", "Swap with a colleague"), ("text", "In his own words")];

/// Where a driver comes in the order an open duty is filled in, in words.
const TIERS: [&str; 4] = ["In his frame", "Colleague", "Hourly paid", "Agency driver"];

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

/// A driver's day as a cell of the calendar.
fn cell_json(day: &po::DriverDay, duties: &[ProDuty]) -> Value {
    match &day.is {
        DayIs::Duties { numbers, start, end, frame } => {
            let label = numbers.iter().map(|n| if *n == 0 { "+".to_string() } else { n.to_string() }).collect::<Vec<_>>().join(" + ");
            let split = numbers.iter().any(|n| duties.iter().any(|d| d.number == *n && d.kind == pd::Kind::Split));
            json!({ "kind": "duty", "label": label, "sub": trf("sign on %{time}", &[("time", &clock(*start))]), "tip": format!("{}–{}", clock(*start), clock(*end)), "cls": cls_of(*start, split), "frame": frame.is_some(), "numbers": numbers })
        }
        DayIs::Frame { from, to } => json!({ "kind": "frame", "label": tr("Reserve frame"), "sub": format!("{}–{}", clock(*from), clock(*to)), "cls": "reserve" }),
        DayIs::Off => json!({ "kind": "off", "label": tr("day off"), "cls": "off" }),
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
    let mut out = json!({
        "week": { "label": week_label(&first), "from": day_label(&first), "to": day_label(dates.last().unwrap_or(&first)), "offset": offset, "span": if weeks == 1 { "week" } else if weeks == 2 { "two" } else { "month" } },
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
    let days: Vec<Value> = dates
        .iter()
        .map(|d| {
            let missing = plans.get(d).map_or(0, |p| po::open_duties(p, duties_on(&week, d)).len());
            json!({ "date": d, "wd": tr(DAYS[pt::day_of(d)]), "day": short_date(d), "today": *d == c.date, "past": co::dates::between(&c.date, d) < 0, "missing": missing, "label": day_label(d) })
        })
        .collect();
    let want = str_arg(args, "kind");
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
                    let ds = duties_on(&week, d);
                    let day = po::driver_day(&c, e, p, ds);
                    minutes.insert(d.as_str(), day.minutes);
                    let mut v = cell_json(&day, ds);
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
                    json!({ "h": hours(m), "of": contract.map(|k| num(k / 60.0, if (k / 60.0).fract() == 0.0 { 0 } else { 1 })), "over": over, "label": week_label(&sunday) })
                })
                .collect();
            json!({ "id": e.id, "nr": e.id, "name": e.name, "kind": key, "chip": chip, "sub": sub, "cells": cells, "hours": hours_of })
        })
        .collect();
    out["days"] = json!(days);
    out["drivers"] = json!(list);
    out["kinds"] = json!([["all", tr("All")], ["permanent", tr("Permanent")], ["reserve", tr("Reserve frame")], ["hourly", tr("Hourly")]]);
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

/// What a driver costs for the duty, in words, and whether it is overtime (shown orange).
fn price_words(c: &Company, e: &Employee, p: &Price, cost: Cents) -> (String, bool) {
    let money = eur(cost);
    match p {
        Price::Frame => (tr("within his frame · nothing more"), false),
        Price::Within => (trf("within his %{h} h · nothing more", &[("h", &po::contract_minutes(c, e).map_or(String::new(), |m| num(m / 60.0, 0)))]), false),
        Price::Overtime { minutes, rate } => (trf("overtime %{t} × %{r} = %{cost}", &[("t", &length(*minutes)), ("r", &num(*rate, 1)), ("cost", &money)]), true),
        Price::Hourly { minutes } => (trf("hourly %{t} = %{cost}", &[("t", &length(*minutes)), ("cost", &money)]), false),
        Price::Extra { minutes } => (trf("outside his frame %{t} = %{cost}", &[("t", &length(*minutes)), ("cost", &money)]), false),
        Price::Agency { minutes } => (trf("agency %{t} = %{cost}", &[("t", &length(*minutes)), ("cost", &money)]), false),
    }
}

/// A name for an answer's places: a driver's.
fn name_in(c: &Company) -> impl Fn(u32) -> String + '_ {
    |id| c.employee(id).map(|e| e.name.clone()).unwrap_or_default()
}

/// An answer as the page shows it: its kind, its chip, its words (translated, places filled).
fn reply_json(c: &Company, r: &Reply) -> Value {
    let (text, args) = po::words(r, &name_in(c));
    let words = match r {
        Reply::Text { text, .. } => format!("\"{text}\""),
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
            let m = po::week_minutes(c, e, &sunday, &|d| plans.get(d).map_or(0.0, |p| po::driver_day(c, e, p, duties_on(week, d)).minutes));
            (e.id, m)
        })
        .collect()
}

/// The daily operations of `date` (`rf_ops`: `date`, the company's day when none; `n` the duty
/// chosen, the first open when none).
fn daily(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({ "none": true })) };
    if let Some(v) = off(&c) {
        return Ok(v);
    }
    let date = Some(str_arg(args, "date")).filter(|d| co::dates::parse(d).is_some() && co::dates::between(&c.date, d) >= 0).unwrap_or_else(|| c.date.clone());
    let mut out = json!({
        "date": date, "label": day_label(&date), "today": date == c.date,
        "prev": (co::dates::between(&c.date, &date) > 0).then(|| co::dates::add(&date, -1)), "next": co::dates::add(&date, 1),
        "answers": ANSWERS.iter().map(|(k, w)| json!({ "key": k, "label": tr(w) })).collect::<Vec<_>>(),
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
    let dates = po::week_dates(&date);
    let mut need = dates.clone();
    if !need.contains(&date) {
        need.push(date.clone());
    }
    let plans = match plans_of(s, r, &need) {
        Ok(p) => p,
        Err(e) => {
            out["reading"] = json!(e);
            return Ok(out);
        }
    };
    let Some(p) = plans.get(&date) else { return Ok(out) };
    let ds = duties_on(&week, &date);
    let open = po::open_duties(p, ds);
    out["open"] = json!(open
        .iter()
        .map(|o| json!({ "n": o.n, "time": format!("{}–{}", clock(o.start), clock(o.end)), "sign_on": trf("sign on %{time}", &[("time", &clock(o.start))]), "why": why_words(o.why), "part": o.part, "lines": o.lines, "kind": tr(o.kind.label()), "asked": po::asks_of(&c, &date, o.n).count() }))
        .collect::<Vec<_>>());
    // the frames of the day, and who is free all day
    out["frames"] = json!(plan::frames_on(&c, &date)
        .iter()
        .map(|(e, f)| {
            let day = po::driver_day(&c, e, p, ds);
            let used = matches!(day.is, DayIs::Duties { .. });
            json!({ "id": e.id, "nr": e.id, "name": e.name, "time": format!("{}–{}", clock(f.from), clock(f.to)), "used": used, "what": cell_json(&day, ds)["label"] })
        })
        .collect::<Vec<_>>());
    out["free"] = json!(drivers(&c)
        .into_iter()
        .filter(|e| po::driver_day(&c, e, p, ds).is == DayIs::Off)
        .map(|e| {
            let (key, chip, _) = kind_of(&c, e);
            json!({ "id": e.id, "nr": e.id, "name": e.name, "kind": key, "chip": chip })
        })
        .collect::<Vec<_>>());
    out["count"] = json!({ "duties": ds.len(), "open": open.len() });
    // the duty chosen: who could take it and what he costs, the drivers asked
    let n = args.get("n").and_then(Value::as_u64).map(|x| x as u32);
    let Some(d) = n.and_then(|n| open.iter().find(|o| o.n == n)).or(open.first()) else { return Ok(out) };
    out["sel"] = json!(d.n);
    let weekly = week_of_drivers(&c, &week, &plans, &date);
    let cands = po::candidates(&c, p, d, &|id| weekly.get(&id).copied().unwrap_or(0.0));
    let asks: Vec<&po::Ask> = po::asks_of(&c, &date, d.n).collect();
    let now = now();
    let list: Vec<Value> = cands
        .iter()
        .filter_map(|x| {
            let e = c.employee(x.id).or_else(|| co::agency::drivers_on(&c, &date).into_iter().find(|a| a.id == x.id))?;
            let (key, chip, sub) = kind_of(&c, e);
            let (price, over) = price_words(&c, e, &x.price, x.cost);
            let booked = co::agency::is_booked(e.id);
            let ask = asks.iter().find(|a| a.who == (Asked::Staff { id: e.id }));
            Some(json!({
                "id": e.id, "nr": if booked { co::agency::number_of(e) } else { e.id }, "name": e.name,
                "kind": if booked { "agency" } else { key }, "chip": if booked { tr("Agency temp") } else { chip }, "sub": sub,
                "tier": tr(TIERS[(x.tier as usize).min(TIERS.len() - 1)]), "standby": x.standby,
                "free": x.fit.is_ok(), "why": x.fit.as_ref().err().map(cannot_words),
                "week": hours(x.week), "price": price, "over": over, "cost": x.cost,
                "note": po::own_hours(e).map(|h| tr(h.2)),
                "asked": ask.is_some(), "askable": !booked,
            }))
        })
        .collect();
    let asked: Vec<Value> = asks
        .iter()
        .map(|a| {
            let (name, nr) = match a.who {
                Asked::Staff { id } => (c.employee(id).map(|e| e.name.clone()).unwrap_or_default(), Some(id)),
                Asked::Guest { no } => (c.together.name_of(no), None),
            };
            let shown = a.shown(now);
            json!({ "id": a.id, "name": name, "nr": nr, "who": a.who, "reply": shown.map(|r| reply_json(&c, r)), "waiting": shown.is_none() })
        })
        .collect();
    // (the players of "Play together" who drive, asked for a duty of the company's day only)
    let players: Vec<Value> = if date == c.date {
        c.together
            .guests
            .iter()
            .filter(|g| !g.banned && g.perms().contains(&omsi_launcher_lib::company::together::Perm::Drive))
            .map(|g| json!({ "no": g.no, "name": g.name, "asked": asks.iter().any(|a| a.who == (Asked::Guest { no: g.no })) }))
            .collect()
    } else {
        Vec::new()
    };
    let given = c.planning.ops.calls.iter().find(|x| x.date == date && x.n == d.n).and_then(|x| c.employee(x.driver)).map(|e| e.name.clone());
    out["duty"] = json!({
        "n": d.n, "time": format!("{}–{}", clock(d.start), clock(d.end)), "sign_on": clock(d.start), "why": why_words(d.why), "lines": d.lines, "kind": tr(d.kind.label()), "paid": length(d.paid),
        "candidates": list, "asks": asked, "players": players, "given": given,
        "agency": trf("An agency driver sent on the day: %{cost}", &[("cost", &eur(po::agency_on_day(&c, p, d)))]),
        "waiting": asks.iter().any(|a| a.shown(now).is_none() && matches!(a.who, Asked::Staff { .. })),
    });
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

/// What a click of the daily operations does (`rf_ops_act`: `act` "ask" | "give" | "give_guest" |
/// "withdraw",
/// `date`, `n`, and what it needs).
fn act(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let what = str_arg(args, "act");
    let date = str_arg(args, "date");
    let n = args.get("n").and_then(Value::as_u64).unwrap_or(0) as u32;
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
            let week = match super::turnus::week_duties(s, r) {
                Some(Ok(w)) => w,
                Some(Err(e)) => return Ok(said(e, true)),
                None => return Ok(said(tr("Reading the timetable…"), false)),
            };
            let mut need = po::week_dates(&date);
            need.push(date.clone());
            let plans = match plans_of(s, r, &need) {
                Ok(p) => p,
                Err(e) => return Ok(said(e, true)),
            };
            let Some(p) = plans.get(&date) else { return Ok(json!({})) };
            let ds = duties_on(&week, &date);
            let Some(d) = po::open_duties(p, ds).into_iter().find(|o| o.n == n) else { return Ok(said(tr("That duty has a driver now."), true)) };
            let weekly = week_of_drivers(&c, &week, &plans, &date);
            let cands = po::candidates(&c, p, &d, &|id| weekly.get(&id).copied().unwrap_or(0.0));
            let ids: Vec<u32> = args.get("drivers").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as u32).collect()).unwrap_or_default();
            let guests: Vec<u32> = args.get("guests").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as u32).collect()).unwrap_or_default();
            let mut who: Vec<(Asked, Option<Reply>)> = Vec::new();
            for id in &ids {
                let Some(e) = c.employee(*id) else { continue };
                let sit = situation(&c, p, ds, &d, e, cands.iter().find(|x| x.id == *id));
                who.push((Asked::Staff { id: *id }, Some(po::ai_reply(&c, e, &date, d.n, d.start, d.end, &sit))));
            }
            if date == c.date {
                who.extend(guests.iter().filter(|g| c.together.guest(**g).is_some_and(|x| !x.banned)).map(|g| (Asked::Guest { no: *g }, None)));
            }
            let count = who.len();
            let at = now();
            edit(s, trf("%{n} asked for duty %{duty}: their answers come in here.", &[("n", &count.to_string()), ("duty", &d.n.to_string())]), &mut |c| po::ask(c, &date, d.n, d.start, d.end, who.clone(), at).map(|_| ()))
        }
        "give" => {
            let driver = args.get("driver").and_then(Value::as_u64).unwrap_or(0) as u32;
            let swap = args.get("swap").filter(|x| x.is_object()).and_then(|x| Some((x.get("with")?.as_u64()? as u32, x.get("theirs")?.as_u64()? as u32)));
            let name = c.employee(driver).map(|e| e.name.clone()).unwrap_or_default();
            let words = match swap {
                Some((with, theirs)) => trf("Duty %{n} is %{name}'s on %{day}, and his %{theirs} %{with}'s.", &[("n", &n.to_string()), ("name", &name), ("day", &day_label(&date)), ("theirs", &theirs.to_string()), ("with", &name_in(&c)(with))]),
                None => trf("Duty %{n} is %{name}'s on %{day}.", &[("n", &n.to_string()), ("name", &name), ("day", &day_label(&date))]),
            };
            edit(s, words, &mut |c| po::give(c, &date, n, driver, swap))
        }
        // (a player who said yes: the duty of the company's day his - `together::book::give`)
        "give_guest" => {
            let guest = args.get("guest").and_then(Value::as_u64).unwrap_or(0) as u32;
            if date != c.date {
                return Ok(said(tr("A player can only be given a duty of the company's day."), true));
            }
            let week = match super::turnus::week_duties(s, r) {
                Some(Ok(w)) => w,
                Some(Err(e)) => return Ok(said(e, true)),
                None => return Ok(said(tr("Reading the timetable…"), false)),
            };
            company::week_work(s);
            let Some(Ok(p)) = company::plan_of(s, &date) else { return Ok(said(tr("Reading the timetable…"), false)) };
            let Some(d) = po::open_duties(&p, duties_on(&week, &date)).into_iter().find(|o| o.n == n) else { return Ok(said(tr("That duty has a driver now."), true)) };
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
    v.extend(ANSWERS.iter().map(|a| a.1));
    v.extend(TIERS);
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
    fn the_calendar_and_the_daily_operations_show_ask_and_give_a_duty() {
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
        assert_eq!(days[0]["date"], "2024-03-03");
        assert_eq!(days[1]["today"], true);
        assert_eq!(days[0]["past"], true);
        let rows = v["drivers"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["nr"], ids[0]);
        assert_eq!(rows[0]["chip"], "Permanent");
        assert_eq!(rows[0]["cells"].as_array().map(Vec::len), Some(7));
        assert_eq!(rows[0]["hours"][0]["of"], "39");
        // nobody planned: every duty of a day missing
        let missing = days[2]["missing"].as_u64().unwrap();
        assert!(missing >= 2, "{}", days[2]);
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
        assert_eq!(v["drivers"][0]["chip"], "Hourly");
        let v = calendar(&mut s, &mut r, &json!({})).unwrap();
        assert_eq!(v["drivers"][1]["chip"], "Reserve frame");
        assert_eq!(v["drivers"][1]["cells"][2]["kind"], "frame", "his frame on Tuesday: {}", v["drivers"][1]["cells"][2]);
        // the daily operations of Tuesday: the open duties, the frame, who is free
        let date = "2024-03-05";
        let v = daily(&mut s, &mut r, &json!({ "date": date })).unwrap();
        assert!(v.get("reading").is_none(), "{v}");
        let open = v["open"].as_array().unwrap();
        assert!(open.len() >= 2, "{v}");
        assert_eq!(v["frames"][0]["id"], ids[1]);
        let n = v["sel"].as_u64().unwrap();
        let cands = v["duty"]["candidates"].as_array().unwrap();
        assert!(!cands.is_empty());
        // the first open duty begins at 06:00: within the reserve's frame - he comes first, free
        assert_eq!(cands[0]["id"], ids[1], "{cands:?}");
        assert_eq!(cands[0]["tier"], "In his frame");
        assert_eq!(cands[0]["price"], "within his frame · nothing more");
        let hourly = cands.iter().find(|x| x["id"] == ids[2]).unwrap();
        assert!(hourly["price"].as_str().unwrap().starts_with("hourly "), "{hourly}");
        // asked: the reserve says yes within his frame, the answers come after a few seconds
        let out = act(&mut s, &mut r, &json!({ "act": "ask", "date": date, "n": n, "drivers": [ids[1], ids[2]] })).unwrap();
        assert_eq!(out["edited"], true, "{out}");
        let asks = s.company.as_ref().unwrap().planning.ops.asks.clone();
        assert_eq!(asks.len(), 2);
        assert_eq!(asks.iter().find(|a| a.who == (Asked::Staff { id: ids[1] })).and_then(|a| a.reply.clone()), Some(Reply::Frame));
        let v = daily(&mut s, &mut r, &json!({ "date": date, "n": n })).unwrap();
        assert_eq!(v["duty"]["asks"].as_array().map(Vec::len), Some(2));
        assert!(v["duty"]["candidates"].as_array().unwrap().iter().any(|x| x["asked"] == true));
        // given to the reserve: his that day, the duty no longer open
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
        let o = v["open"].as_array().unwrap().iter().find(|o| o["n"] == week[2][0].number).unwrap().clone();
        assert_eq!(o["why"], "Ill");
        // without RosterFlow Pro: nothing to show
        autopilot::set_planner(s.company.as_mut().unwrap(), Planner::RosterFlow);
        assert_eq!(calendar(&mut s, &mut r, &json!({})).unwrap()["off"], true);
        assert_eq!(daily(&mut s, &mut r, &json!({})).unwrap()["off"], true);
    }
}

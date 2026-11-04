//! RosterFlow Pro's tab "3 Turnus" (step 3, NexiconDK 4 November 2026; the core's
//! `company::pro_turnus`): the company's rotas, the rows of the one chosen - each day's duty with
//! its times and its kind's colour, the row's hours, the marks of the rules over the whole cycle,
//! the driver on the row in the week shown -, the duties of each day no rota has, the drivers who
//! can go on a row, and a driver's next weeks with their exceptions; and the acts that edit them.
//! A rota is the company's at once (`company::act`), not RosterFlow's draft: it is not a day's
//! blocks and duties but who drives them week after week.

use super::super::company::words::{day_label, tr, trf};
use super::super::company::{self, Session};
use super::{str_arg, usize_arg, Rf};
use crate::Answer;
use omsi_launcher_lib::company::model::Employment;
use omsi_launcher_lib::company::pro_duties as pd;
use omsi_launcher_lib::company::pro_turnus::{self as pt, Cell, Checked, ProDuty, RotaKind, Rule, Week, Weekend};
use omsi_launcher_lib::company::{self as co, rosterflow as rf, Company};
use serde_json::{json, Value};
use std::sync::Arc;

/// The days of a row, Sunday first (translated when shown).
const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// The rules the side panel lists, in its order, and their words: an ordinary rota's, a
/// split-duty rota's, a reserve rota's (NexiconDK, 4 November 2026: the weekends by turns, the
/// split duties in a rota of their own).
const RULES_NORMAL: [(Rule, &str); 7] = [
    (Rule::Rest, "11 hours of rest between duties"),
    (Rule::InARow, "At most 6 days of work in a row"),
    (Rule::TwoOff, "Two days off together - a Saturday and the next week's Sunday count"),
    (Rule::EveryOther, "Every other weekend off"),
    (Rule::Hours, "Hours a week as aimed at"),
    (Rule::Split, "No split duties: they have a rota of their own"),
    (Rule::Duty, "Every duty on its day, in one row"),
];
const RULES_SPLIT: [(Rule, &str); 6] = [
    (Rule::Rest, "11 hours of rest between duties"),
    (Rule::InARow, "At most 6 days of work in a row"),
    (Rule::TwoOff, "Two days off together - a Saturday and the next week's Sunday count"),
    (Rule::Split, "Split duties only, Monday to Friday; the weekend off"),
    (Rule::Hours, "Hours a week as aimed at"),
    (Rule::Duty, "Every duty on its day, in one row"),
];
const RULES_RESERVE: [(Rule, &str); 6] = [
    (Rule::Rest, "11 hours of rest between duties"),
    (Rule::InARow, "At most 6 days of work in a row"),
    (Rule::TwoOff, "Two days off together - a Saturday and the next week's Sunday count"),
    (Rule::Hours, "Hours a week as aimed at"),
    (Rule::Weekend, "A weekend day off in the cycle"),
    (Rule::Duty, "Every duty on its day, in one row"),
];

fn rules_of(kind: RotaKind) -> &'static [(Rule, &'static str)] {
    match kind {
        RotaKind::Normal => &RULES_NORMAL,
        RotaKind::Split => &RULES_SPLIT,
        RotaKind::Reserve => &RULES_RESERVE,
    }
}

/// What a kind of rota is, in a line.
const KIND_NOTES: [&str; 3] = ["Whole and short duties all week. The drivers work every other weekend.", "Split duties only, Monday to Friday. Saturday and Sunday are always off.", "Reserve drivers in their frames."];

/// A weekend's words: off, at work, half - its key, its words, and the cell's word under
/// "Weekend".
fn weekend_words(w: Weekend) -> (&'static str, &'static str, &'static str) {
    match w {
        Weekend::Off => ("off", "Weekend off", "Off duty"),
        Weekend::Work => ("work", "Weekend at work", "Working"),
        Weekend::Half => ("half", "Half weekend", "Half"),
    }
}

/// A duty's colour on the rota (its signature): early, day, late, split.
fn kind_of(d: &ProDuty) -> &'static str {
    if d.kind == pd::Kind::Split {
        "split"
    } else if d.start < 9 * 60 {
        "early"
    } else if d.start < 12 * 60 {
        "day"
    } else {
        "late"
    }
}

fn span(a: i32, b: i32) -> String {
    format!("{}–{}", rf::clock(a), rf::clock(b))
}

/// One decimal, the page's way ("37,9").
fn h1(minutes: i32) -> String {
    format!("{:.1}", minutes as f64 / 60.0)
}

fn mark_words(m: &pt::Mark) -> String {
    let args: Vec<(&str, &str)> = m.args.iter().map(|(k, v)| (*k, v.as_str())).collect();
    trf(m.text, &args)
}

/// The duties of a week of the company (its seven days: each weekday once), Sunday first, as the
/// saved planning runs them; None while a day's timetable is being read.
pub(super) fn week_duties(s: &mut Session, r: &mut Rf) -> Option<Result<Arc<Week>, String>> {
    let generation = s.generation;
    if let Some((_, w)) = r.turnus_week.as_ref().filter(|x| x.0 == generation) {
        return Some(Ok(w.clone()));
    }
    company::week_work(s);
    let mut c = s.company.clone()?;
    // (the week as the Vagter tab has it: a duty cut for one date only is not - `pro_cut`)
    c.planning.ops.splits.clear();
    let mut week: Week = vec![Vec::new(); pt::DAYS];
    for k in 0..pt::DAYS as i64 {
        let date = co::dates::add(&c.date, k);
        let lines = match company::lines_of(s, &date) {
            Some(l) => l,
            None => return company::plan_of(s, &date).and_then(|p| p.err()).map(Err),
        };
        let tours = co::network::tours_of_day(&c, &lines, &date);
        week[pt::day_of(&date)] = pt::day_duties(&c, &date, tours);
    }
    let w = Arc::new(week);
    r.turnus_week = Some((generation, w.clone()));
    Some(Ok(w))
}

/// The Sunday of the week shown: the company's week and `week` weeks on.
fn shown_sunday(c: &Company, args: &Value) -> String {
    let week = usize_arg(args, "week").min(104) as i64;
    co::dates::add(&pt::sunday_of(&c.date), 7 * week)
}

/// "Week 46".
fn week_label(sunday: &str) -> String {
    trf("Week %{n}", &[("n", &pt::week_number(&co::dates::add(sunday, 1)).to_string())])
}

/// A short date: "8/11".
fn short(date: &str) -> String {
    match co::dates::parse(date) {
        Some(d) => {
            let (_, m, day) = co::dates::civil_from_days(d);
            format!("{day}/{m}")
        }
        None => date.to_string(),
    }
}

fn person(c: &Company, id: u32) -> Value {
    match c.employee(id) {
        Some(e) => json!({ "id": e.id, "name": e.name, "nr": e.id }),
        None => Value::Null,
    }
}

/// "rota week 3": a week of a rota (a row of it).
fn week_words(row: usize) -> String {
    trf("rota week %{n}", &[("n", &(row + 1).to_string())])
}

/// Where a driver is in the rotas, in words: "Nord fuldtid · rota week 3".
fn place_words(c: &Company, id: u32) -> Option<String> {
    let (k, row) = pt::place_of(c, id)?;
    Some(format!("{} · {}", c.planning.rotas.list[k].name, week_words(row)))
}

/// A cell as the page draws it.
fn cell_json(week: &Week, day: usize, cell: Cell) -> Value {
    match cell {
        Cell::Off => json!({ "kind": "off", "label": tr("Day off"), "cls": "off" }),
        Cell::Duty { n } => match week.get(day).and_then(|ds| ds.iter().find(|x| x.number == n)) {
            Some(d) => json!({ "kind": "duty", "n": n, "label": n.to_string(), "time": span(d.start, d.end), "cls": kind_of(d), "paid": d.paid }),
            None => json!({ "kind": "duty", "n": n, "label": n.to_string(), "time": "?", "cls": "missing" }),
        },
        Cell::Frame { from, to } => json!({ "kind": "frame", "from": from, "to": to, "label": tr("Reserve"), "time": span(from, to), "cls": "reserve" }),
    }
}

fn level_of<'a>(marks: impl Iterator<Item = &'a pt::Mark>) -> (Option<&'static str>, String) {
    let ms: Vec<&pt::Mark> = marks.collect();
    if ms.is_empty() {
        return (None, String::new());
    }
    let level = if ms.iter().any(|m| m.hard) { "bad" } else { "warn" };
    (Some(level), ms.iter().map(|m| mark_words(m)).collect::<Vec<_>>().join("\n"))
}

/// The rota `k` as the page draws it, the week of `sunday` shown.
fn rota_json(c: &Company, k: usize, week: &Week, sunday: &str, checked: &Checked) -> Value {
    let t = &c.planning.rotas.list[k];
    let rows: Vec<Value> = t
        .rows
        .iter()
        .enumerate()
        .map(|(r, row)| {
            let cells: Vec<Value> = (0..pt::DAYS)
                .map(|d| {
                    let mut v = cell_json(week, d, row.days[d]);
                    let (level, tip) = level_of(checked.of(r, Some(d)));
                    v["level"] = json!(level);
                    v["tip"] = json!(tip);
                    v
                })
                .collect();
            let (level, tip) = level_of(checked.of(r, None));
            let minutes = checked.minutes.get(r).copied().unwrap_or(0);
            json!({
                "cells": cells,
                "hours": h1(minutes),
                "level": level, "tip": tip,
                "driver": pt::driver_on(t, r, sunday).map(|id| person(c, id)),
                "start": row.driver.map(|id| person(c, id)),
            })
        })
        .collect();
    let kind = t.kind();
    let rules: Vec<Value> = rules_of(kind).iter().map(|(rule, words)| json!({ "text": tr(words), "ok": checked.kept(*rule) })).collect();
    let general: Vec<String> = checked.marks.iter().filter(|m| m.row.is_none()).map(mark_words).collect();
    let started = pt::week_of(t, sunday).is_some();
    // the weekend after each week: its Saturday and the next week's Sunday
    let n = t.rows.len();
    let weekends: Vec<Value> = (0..n)
        .filter_map(|k| pt::weekend(t, k).map(|w| (k, w)))
        .map(|(k, w)| {
            let (key, words, short) = weekend_words(w);
            let level = checked.of(k, Some(pt::DAYS - 1)).filter(|m| m.rule == Rule::EveryOther).map(|m| if m.hard { "bad" } else { "warn" }).next();
            let tip = trf("The Saturday of week %{a} and the Sunday of week %{b}", &[("a", &(k + 1).to_string()), ("b", &((k + 1) % n + 1).to_string())]);
            json!({ "state": key, "label": tr(words), "short": tr(short), "tip": tip, "level": level })
        })
        .collect();
    json!({
        "id": t.id, "name": t.name, "reserve": t.reserve,
        "kind": kind.key(), "kind_label": tr(kind.label()), "locked_weekend": kind == RotaKind::Split,
        "weekends": weekends,
        "hours": t.hours, "target": format!("{:.1}", checked.target), "average": format!("{:.1}", checked.average),
        "from": t.from, "from_label": week_label(&t.from), "from_day": day_label(&t.from),
        "started": started,
        "rows": rows, "rules": rules, "general": general,
        "cycle": t.rows.len(),
    })
}

/// The tab (`rf_turnus`: `sel` the rota shown, `week` the weeks on from the company's shown,
/// `driver` whose next weeks the panel shows).
fn view(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({ "none": true })) };
    if !pd::on(&c) {
        return Ok(json!({ "off": true, "reading": tr("RosterFlow Pro is not the planner of this company: choose it on the planning page.") }));
    }
    let sunday = shown_sunday(&c, args);
    let mut out = json!({
        "days": DAYS.iter().map(|d| tr(d)).collect::<Vec<_>>(),
        "week": { "label": week_label(&sunday), "from": day_label(&sunday), "offset": usize_arg(args, "week").min(104) },
        "notes": pt::NOTES.iter().map(|(k, w)| json!({ "key": k, "label": tr(w) })).collect::<Vec<_>>(),
        "frames": pt::RESERVE_FRAMES.iter().map(|(a, b)| json!({ "from": a, "to": b, "time": span(*a, *b) })).collect::<Vec<_>>(),
    });
    let week = match week_duties(s, r) {
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
    // (the rota is of the saved duties, as the days run them: RosterFlow's draft may number them
    // otherwise - said)
    let unsaved = (0..7u8).any(|k| rf::draft_differs(&c, k));
    out["unsaved"] = json!(unsaved);
    let list = &c.planning.rotas.list;
    let sel = args.get("sel").and_then(Value::as_u64).map(|x| x as u32).filter(|id| list.iter().any(|t| t.id == *id)).or_else(|| list.first().map(|t| t.id));
    out["sel"] = json!(sel);
    out["rotas"] = json!(list
        .iter()
        .map(|t| json!({
            "id": t.id, "name": t.name, "reserve": t.reserve, "rows": t.rows.len(),
            "kind": t.kind().key(), "kind_label": tr(t.kind().label()),
            "drivers": t.rows.iter().filter(|r| r.driver.is_some()).count(),
            "hours": format!("{:.1}", pt::target_hours(&c, t)),
        }))
        .collect::<Vec<_>>());
    // the duties of each day, and where they are
    let placed_in = |d: usize, n: u32| -> Option<(u32, usize)> {
        list.iter().find_map(|t| t.rows.iter().position(|r| r.days[d] == Cell::Duty { n }).map(|row| (t.id, row)))
    };
    let duties: Vec<Value> = (0..pt::DAYS)
        .map(|d| {
            json!(week[d]
                .iter()
                .map(|x| {
                    let at = placed_in(d, x.number);
                    let name = at.and_then(|(id, row)| list.iter().find(|t| t.id == id).map(|t| format!("{} · {}", t.name, week_words(row))));
                    json!({ "n": x.number, "time": span(x.start, x.end), "cls": kind_of(x), "split": x.kind == pd::Kind::Split, "hours": h1(x.paid), "rota": at.map(|a| a.0), "row": at.map(|a| a.1), "where": name })
                })
                .collect::<Vec<_>>())
        })
        .collect();
    out["duties"] = json!(duties);
    let unplaced = pt::unplaced(&c, &week);
    let total: usize = week.iter().map(Vec::len).sum();
    let open: usize = unplaced.iter().map(Vec::len).sum();
    // (the split duties of Monday to Friday no rota has: a split-duty rota's)
    let split_open = pt::split_open(&c, &week);
    out["coverage"] = json!({ "placed": total - open, "total": total, "open": unplaced, "split_open": split_open, "split_rota": list.iter().any(|t| t.kind() == RotaKind::Split) });
    // "New rota": the kinds, what each is, the weeks suggested and the drivers without a rota
    let free = |reserve: bool| c.staff.iter().filter(|e| co::staff::drives(e) && pt::place_of(&c, e.id).is_none() && matches!(e.employment, Employment::Reserve(_)) == reserve && !matches!(e.employment, Employment::Hourly)).count();
    let names = [("Rota %{n}", RotaKind::Normal), ("Split duties %{n}", RotaKind::Split), ("Reserve %{n}", RotaKind::Reserve)];
    out["kinds"] = json!(RotaKind::ALL
        .iter()
        .enumerate()
        .map(|(i, k)| {
            let count = list.iter().filter(|t| t.kind() == *k).count() + 1;
            json!({
                "key": k.key(), "label": tr(k.label()), "note": tr(KIND_NOTES[i]),
                "weeks": pt::suggest_weeks(&c, *k, &week),
                "drivers": free(*k == RotaKind::Reserve),
                "name": trf(names[i].0, &[("n", &count.to_string())]),
            })
        })
        .collect::<Vec<_>>());
    // the drivers, and who can go on a row of the rota chosen
    let reserve = sel.and_then(|id| list.iter().find(|t| t.id == id)).is_some_and(|t| t.reserve);
    out["drivers"] = json!(c
        .staff
        .iter()
        .filter(|e| co::staff::drives(e))
        .map(|e| {
            let fits = match e.employment {
                Employment::Hourly => false,
                Employment::Reserve(_) => reserve,
                _ => !reserve,
            };
            json!({ "id": e.id, "name": e.name, "nr": e.id, "kind": tr(e.employment.label()), "hours": co::staff::contract_hours(&c, e).map(|h| format!("{h:.1}")), "where": place_words(&c, e.id), "fits": fits })
        })
        .collect::<Vec<_>>());
    if let Some(k) = sel.and_then(|id| list.iter().position(|t| t.id == id)) {
        let checked = pt::check(&c, k, &week);
        out["rota"] = rota_json(&c, k, &week, &sunday, &checked);
    }
    // a driver's next weeks
    if let Some(id) = args.get("driver").and_then(Value::as_u64).map(|x| x as u32).filter(|id| pt::place_of(&c, *id).is_some()) {
        let weeks: Vec<Value> = (0..4i64)
            .map(|w| {
                let from = co::dates::add(&sunday, 7 * w);
                let days: Vec<Value> = (0..pt::DAYS as i64)
                    .map(|d| {
                        let date = co::dates::add(&from, d);
                        let dd = pt::day_of(&date);
                        match pt::decided(&c, id, &date) {
                            Some(x) => {
                                let mut v = cell_json(&week, dd, x.cell);
                                v["date"] = json!(date);
                                v["short"] = json!(short(&date));
                                v["day"] = json!(dd);
                                v["exception"] = json!(x.note.is_some());
                                if let Some(n) = &x.note {
                                    let words = pt::NOTES.iter().find(|w| w.0 == n).map(|w| tr(w.1)).unwrap_or_default();
                                    v["note"] = json!(words);
                                    v["note_key"] = json!(n);
                                }
                                v
                            }
                            None => json!({ "kind": "none", "date": date, "short": short(&date), "day": dd, "label": "–", "cls": "none" }),
                        }
                    })
                    .collect();
                json!({ "label": week_label(&from), "days": days })
            })
            .collect();
        out["person"] = json!({ "id": id, "name": c.employee(id).map(|e| e.name.clone()).unwrap_or_default(), "where": place_words(&c, id), "weeks": weeks });
    }
    Ok(out)
}

/// A cell of the page's (`{kind: off|duty|frame, n, from, to}`).
fn cell_arg(args: &Value) -> Cell {
    let c = args.get("cell").cloned().unwrap_or(Value::Null);
    let num = |k: &str| c.get(k).and_then(Value::as_i64).unwrap_or(0);
    match c.get("kind").and_then(Value::as_str).unwrap_or("off") {
        "duty" => Cell::Duty { n: num("n").max(0) as u32 },
        "frame" => Cell::Frame { from: num("from") as i32, to: num("to") as i32 },
        _ => Cell::Off,
    }
}

fn said(text: impl Into<String>, error: bool) -> Value {
    json!({ "say": { "text": text.into(), "error": error } })
}

/// An edit of the rotas (`f` on the company, saved at once): `words` said when it is made, its
/// refusal said with its reason.
fn edit<T>(s: &mut Session, words: impl FnOnce(&T) -> String, f: impl FnOnce(&mut Company) -> Result<T, &'static str>) -> Value {
    let mut refused: Option<&'static str> = None;
    let out = company::act(s, |c| {
        f(c).map_err(|e| {
            refused = Some(e);
            e
        })
    });
    match (out, refused) {
        (Ok(v), _) => {
            let mut out = said(words(&v), false);
            out["edited"] = json!(true);
            out
        }
        (Err(_), Some(e)) => said(tr(e), true),
        (Err(_), None) => json!({}),
    }
}

/// What a click on the Turnus tab does (`rf_turnus_act`: `act`, and what it needs).
fn act(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let what = str_arg(args, "act");
    let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
    let row = usize_arg(args, "row");
    let name_of = |s: &Session| s.company.as_ref().and_then(|c| c.planning.rotas.list.iter().find(|t| t.id == id).map(|t| t.name.clone())).unwrap_or_default();
    Ok(match what.as_str() {
        "new" => {
            // (a kind - "normal", "split", "reserve" - and a number of weeks; filled at once with
            // the week's duties when asked: "Make the rota automatically")
            let name = str_arg(args, "name");
            let kind = match args.get("kind").and_then(Value::as_str) {
                Some(k) => RotaKind::of_key(k),
                None if args.get("reserve").and_then(Value::as_bool).unwrap_or(false) => RotaKind::Reserve,
                None => RotaKind::Normal,
            };
            let weeks = usize_arg(args, "weeks").min(pt::MAX_WEEKS);
            let fill = args.get("fill").and_then(Value::as_bool).unwrap_or(false);
            let week = if fill {
                match week_duties(s, r) {
                    Some(Ok(w)) => Some(w),
                    Some(Err(e)) => return Ok(said(e, true)),
                    None => return Ok(said(tr("Reading the timetable…"), false)),
                }
            } else {
                None
            };
            let mut out = edit(
                s,
                |x: &(u32, String, usize, Option<&'static str>)| match x.3 {
                    Some(why) => format!("{} {}", trf("Rota %{name} made with %{n} weeks.", &[("name", &x.1), ("n", &x.2.to_string())]), tr(why)),
                    None if fill => trf("Rota %{name} made: a cycle of %{n} weeks, filled with the week's duties.", &[("name", &x.1), ("n", &x.2.to_string())]),
                    None => trf("Rota %{name} made with %{n} weeks: fill its days, or make it automatically.", &[("name", &x.1), ("n", &x.2.to_string())]),
                },
                |c| {
                    let id = pt::add_kind(c, &name, kind, weeks);
                    let why = match &week {
                        Some(w) => pt::auto(c, id, w).err(),
                        None => None,
                    };
                    let t = c.planning.rotas.list.iter().find(|t| t.id == id).ok_or(pt::NO_ROTA)?;
                    Ok((id, t.name.clone(), t.rows.len(), why))
                },
            );
            if let Some(new) = s.company.as_ref().and_then(|c| c.planning.rotas.list.last()) {
                out["sel"] = json!(new.id);
            }
            out
        }
        "remove" => {
            let name = name_of(s);
            edit(s, |_| trf("Rota %{name} deleted: its drivers drive as the roster has them.", &[("name", &name)]), |c| if pt::remove(c, id) { Ok(()) } else { Err(pt::NO_ROTA) })
        }
        "rename" => {
            let name = str_arg(args, "name");
            edit(s, |_| tr("Renamed."), |c| pt::rename(c, id, &name))
        }
        "hours" => {
            let hours = args.get("hours").and_then(Value::as_f64);
            edit(s, |_| tr("The hours the rows aim at changed."), |c| pt::set_hours(c, id, hours))
        }
        "from" => {
            let Some(c) = s.company.as_ref() else { return Ok(json!({})) };
            let date = shown_sunday(c, args);
            let label = week_label(&date);
            edit(s, |_| trf("The rota begins in %{week}: the driver of rota week 1 is in rota week 1 then.", &[("week", &label)]), |c| pt::set_from(c, id, &date))
        }
        "add_row" => edit(s, |n: &usize| trf("Rota week %{n} added.", &[("n", &n.to_string())]), |c| pt::add_row(c, id)),
        "remove_row" => edit(s, |_| trf("Rota week %{n} removed.", &[("n", &(row + 1).to_string())]), |c| pt::remove_row(c, id, row)),
        "cell" => {
            let week = match week_duties(s, r) {
                Some(Ok(w)) => w,
                Some(Err(e)) => return Ok(said(e, true)),
                None => return Ok(said(tr("Reading the timetable…"), false)),
            };
            let (day, cell) = (usize_arg(args, "day"), cell_arg(args));
            edit(s, |_| String::new(), |c| pt::set_cell(c, id, row, day, cell, &week))
        }
        "driver" => {
            let who = args.get("driver").and_then(Value::as_u64).map(|x| x as u32);
            let name = who.and_then(|w| s.company.as_ref().and_then(|c| c.employee(w)).map(|e| e.name.clone()));
            edit(
                s,
                |_| match &name {
                    Some(n) => trf("%{name} begins in rota week %{n}: a week further down every week.", &[("name", n), ("n", &(row + 1).to_string())]),
                    None => trf("Rota week %{n} has no driver now.", &[("n", &(row + 1).to_string())]),
                },
                |c| pt::set_driver(c, id, row, who),
            )
        }
        "auto" => {
            let week = match week_duties(s, r) {
                Some(Ok(w)) => w,
                Some(Err(e)) => return Ok(said(e, true)),
                None => return Ok(said(tr("Reading the timetable…"), false)),
            };
            edit(
                s,
                |x: &(usize, usize)| {
                    let made = trf("The rota made automatically: a cycle of %{n} weeks.", &[("n", &x.0.to_string())]);
                    if x.1 > 0 {
                        format!("{made} {}", trf("%{n} duties could not be placed: they are under Not in a rota.", &[("n", &x.1.to_string())]))
                    } else {
                        made
                    }
                },
                |c| {
                    let n = pt::auto(c, id, &week)?;
                    let kind = c.planning.rotas.list.iter().find(|t| t.id == id).map(|t| t.kind()).unwrap_or(RotaKind::Normal);
                    Ok((n, pt::left_over(c, kind, &week)))
                },
            )
        }
        "exception" => {
            let driver = args.get("driver").and_then(Value::as_u64).unwrap_or(0) as u32;
            let date = str_arg(args, "date");
            let clear = args.get("clear").and_then(Value::as_bool).unwrap_or(false);
            let cell = (!clear).then(|| (cell_arg(args), str_arg(args, "note")));
            let words = if clear { trf("%{day} as in the rota again.", &[("day", &day_label(&date))]) } else { trf("%{day} changed for that day only: the rota stays as it is.", &[("day", &day_label(&date))]) };
            edit(s, |_| words.clone(), |c| {
                if c.employee(driver).is_none() {
                    return Err(pt::NOT_A_DRIVER);
                }
                pt::set_exception(c, driver, &date, cell);
                Ok(())
            })
        }
        _ => json!({}),
    })
}

/// The commands of the Turnus tab (`super::run`).
pub(super) fn run(s: &mut Session, r: &mut Rf, cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "rf_turnus" => view(s, r, args),
        "rf_turnus_act" => act(s, r, args),
        _ => return None,
    })
}

/// Rota `turnus` as a PDF file (`rf_print` with `turnus`): its file name and bytes, or what to say.
pub(super) fn made(s: &mut Session, r: &mut Rf, args: &Value) -> Result<(String, Vec<u8>), Value> {
    let id = args.get("turnus").and_then(Value::as_u64).unwrap_or(0) as u32;
    let week = match week_duties(s, r) {
        Some(Ok(w)) => w,
        Some(Err(e)) => return Err(said(e, true)),
        None => return Err(said(tr("Reading the timetable…"), false)),
    };
    let c = s.company.as_ref().ok_or_else(|| said(tr(pt::NO_ROTA), true))?;
    let k = c.planning.rotas.list.iter().position(|t| t.id == id).ok_or_else(|| said(tr(pt::NO_ROTA), true))?;
    let t = &c.planning.rotas.list[k];
    let sunday = shown_sunday(c, args);
    let checked = pt::check(c, k, &week);
    let words = pt::PrintWords {
        title: tr("Rota"),
        info: format!(
            "{}  ·  {}  ·  {}  ·  {}",
            trf("A cycle of %{n} weeks", &[("n", &t.rows.len().to_string())]),
            trf("aims at %{h} h a week, averages %{a} h", &[("h", &format!("{:.1}", checked.target).replace('.', ",")), ("a", &format!("{:.1}", checked.average).replace('.', ","))]),
            trf("the drivers of %{week}", &[("week", &week_label(&sunday))]),
            c.name
        ),
        days: std::array::from_fn(|d| tr(DAYS[d])),
        row: tr("Rota week"),
        hours: tr("Hours"),
        driver: week_label(&sunday),
        off: tr("Day off"),
        page: tr("Page"),
        foot: tr("Every week each driver moves a week down; after the last week he begins again at week 1."),
    };
    let name = format!("rota-{}.pdf", t.name.chars().map(|ch| if ch.is_alphanumeric() { ch } else { '-' }).collect::<String>());
    Ok((name, pt::pdf(c, k, &week, &sunday, &words)))
}

/// Every English text this file hands to `tr`/`trf` besides its literals.
#[cfg(test)]
pub(super) fn texts() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = DAYS.to_vec();
    for k in RotaKind::ALL {
        v.extend(rules_of(k).iter().map(|r| r.1));
    }
    v.extend(KIND_NOTES);
    v.extend([Weekend::Off, Weekend::Work, Weekend::Half].map(|w| weekend_words(w).1));
    v.extend([Weekend::Off, Weekend::Work, Weekend::Half].map(|w| weekend_words(w).2));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::autopilot;
    use omsi_launcher_lib::company::model::Planner;

    fn session() -> Session {
        let (mut c, l) = super::super::tests::line5_company();
        assert_eq!(autopilot::set_planner(&mut c, Planner::Pro), Planner::Pro);
        // (two drivers)
        while c.staff.iter().filter(|e| co::staff::drives(e)).count() < 2 {
            for a in co::staff::applicants(&c) {
                if a.role == co::model::Role::Driver && c.staff.iter().filter(|e| co::staff::drives(e)).count() < 2 {
                    let _ = co::staff::hire(&mut c, &a);
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        let mut s = Session::default();
        s.company = Some(c);
        company::test_week(&mut s, vec![l]);
        s
    }

    #[test]
    fn the_turnus_tab_makes_a_rota_shows_it_and_a_drivers_weeks() {
        super::super::super::company::words::set_language("en");
        company::test_dry();
        let mut s = session();
        let mut r = Rf::default();
        let v = view(&mut s, &mut r, &json!({})).unwrap();
        assert!(v["rotas"].as_array().is_some_and(Vec::is_empty), "{v}");
        assert_eq!(v["days"][0], "Sun");
        // the week's duties: each day's, none in a rota yet
        assert_eq!(v["coverage"]["placed"], 0);
        let total = v["coverage"]["total"].as_u64().unwrap();
        assert!(total >= 7, "{v}");
        assert!(v["duties"][1].as_array().is_some_and(|d| d[0]["n"] == 1001));
        assert!(v["duties"][0].as_array().is_some_and(|d| d[0]["n"] == 3001));
        // a rota, made automatically: every duty of the week in it
        // "New rota": the three kinds, the weeks suggested
        let kinds = v["kinds"].as_array().unwrap();
        assert_eq!(kinds.iter().map(|k| k["key"].as_str().unwrap()).collect::<Vec<_>>(), ["normal", "split", "reserve"]);
        assert!(kinds[0]["weeks"].as_u64().unwrap() >= 2, "{kinds:?}");
        assert_eq!(kinds[0]["name"], "Rota 1");
        let out = act(&mut s, &mut r, &json!({ "act": "new", "name": "Nord", "kind": "normal", "weeks": 1 })).unwrap();
        let id = out["sel"].as_u64().unwrap();
        assert_eq!(s.company.as_ref().unwrap().planning.rotas.list[0].rows.len(), 1, "one week, as asked");
        let out = act(&mut s, &mut r, &json!({ "act": "auto", "id": id })).unwrap();
        assert_eq!(out["edited"], true, "{out}");
        let v = view(&mut s, &mut r, &json!({ "sel": id })).unwrap();
        let split = v["coverage"]["split_open"].as_u64().unwrap();
        assert_eq!(v["coverage"]["placed"].as_u64().unwrap() + split, total, "{}", v["coverage"]);
        // a rota made and filled at once: a split-duty rota takes the split duties, if any
        let out = act(&mut s, &mut r, &json!({ "act": "new", "name": "Delt", "kind": "split", "weeks": 0, "fill": true })).unwrap();
        assert_eq!(out["say"]["error"], false, "{out}");
        assert_eq!(s.company.as_ref().unwrap().planning.rotas.list[1].kind(), RotaKind::Split);
        if split == 0 {
            assert!(out["say"]["text"].as_str().unwrap().contains(pt::NO_SPLIT), "{out}");
        }
        let rota = &v["rota"];
        assert_eq!(rota["name"], "Nord");
        let rows = rota["rows"].as_array().unwrap();
        assert!(!rows.is_empty());
        assert_eq!(rows[0]["cells"].as_array().map(Vec::len), Some(7));
        assert!(rota["rules"].as_array().is_some_and(|r| r.len() == 7));
        assert_eq!(rota["kind"], "normal");
        // the weekends: one after each week, by turns
        let ws = rota["weekends"].as_array().unwrap();
        assert_eq!(ws.len(), rows.len());
        assert!(rows.len() % 2 == 0, "an even number of weeks");
        assert_eq!((ws[0]["state"].as_str(), ws[1]["state"].as_str()), (Some("work"), Some("off")), "{ws:?}");
        // a driver on row 1, his next weeks: a row down each week
        let driver = s.company.as_ref().unwrap().staff.iter().find(|e| co::staff::drives(e)).map(|e| e.id);
        assert!(driver.is_some());
        if let Some(d) = driver {
            let out = act(&mut s, &mut r, &json!({ "act": "driver", "id": id, "row": 0, "driver": d })).unwrap();
            assert_eq!(out["edited"], true, "{out}");
            let v = view(&mut s, &mut r, &json!({ "sel": id, "driver": d })).unwrap();
            assert_eq!(v["rota"]["rows"][0]["driver"]["id"], d);
            assert_eq!(v["person"]["weeks"].as_array().map(Vec::len), Some(4));
            // an exception for a day: said, shown in his weeks
            let date = v["person"]["weeks"][0]["days"][2]["date"].as_str().unwrap().to_string();
            let out = act(&mut s, &mut r, &json!({ "act": "exception", "driver": d, "date": date, "cell": { "kind": "off" }, "note": "holiday" })).unwrap();
            assert_eq!(out["edited"], true, "{out}");
            let v = view(&mut s, &mut r, &json!({ "sel": id, "driver": d })).unwrap();
            assert_eq!(v["person"]["weeks"][0]["days"][2]["exception"], true);
            assert_eq!(v["person"]["weeks"][0]["days"][2]["note"], "On holiday");
        }
        // a duty twice on a day is refused, said
        let n = v["duties"][1][0]["n"].as_u64().unwrap();
        act(&mut s, &mut r, &json!({ "act": "add_row", "id": id })).unwrap();
        let last = s.company.as_ref().unwrap().planning.rotas.list[0].rows.len() - 1;
        let id = s.company.as_ref().unwrap().planning.rotas.list[0].id as u64;
        let out = act(&mut s, &mut r, &json!({ "act": "cell", "id": id, "row": last, "day": 1, "cell": { "kind": "duty", "n": n } })).unwrap();
        assert_eq!(out["say"]["error"], true, "{out}");
        assert_eq!(out["say"]["text"], pt::TAKEN);
        // the print
        let (name, bytes) = made(&mut s, &mut r, &json!({ "turnus": id })).unwrap();
        assert!(name.starts_with("rota-Nord") && bytes.starts_with(b"%PDF"));
        // without RosterFlow Pro: nothing to show
        autopilot::set_planner(s.company.as_mut().unwrap(), Planner::RosterFlow);
        assert_eq!(view(&mut s, &mut r, &json!({})).unwrap()["off"], true);
    }
}

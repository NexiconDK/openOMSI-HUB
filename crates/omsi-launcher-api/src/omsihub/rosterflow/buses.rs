//! RosterFlow Pro's tab "7 Busdisponering" (round I, NexiconDK 5 November 2026; the core's
//! `company::pro_buses`): which bus of the fleet runs which block on a date.
//!
//! - the day (`rf_buses`: `date`, `mode` "day" | "week"): the buses a row each - their facts
//!   (size, fuel, what is in the tank, the service, the workshop) and their blocks of the day on a
//!   time axis (an electric bus's charging between them), the blocks without a bus on top; or the
//!   week, Sunday to Saturday, the blocks of each bus a day.
//! - a block (`rf_bus_pick`: `date`, `line`, `tour`): its facts, the buses that can take it (their
//!   warnings), the blocks it could swap buses with, the buses that cannot and why.
//! - the acts (`rf_bus_act`: `act` "set" `bus` | "remove" | "reset" | "swap" `with_line`
//!   `with_tour` | "auto" `redo` `week` | "restore" `picks`): each answers with what undoes it
//!   (`undo`: the blocks' choices before).

use super::super::company::words::{num, short_date, tr, trf};
use super::super::company::{self, Session};
use super::ops::{day_name, plans_of};
use super::{colour_of, str_arg, Rf};
use crate::Answer;
use omsi_launcher_lib::company::model::{BusSize, Drive, Vehicle};
use omsi_launcher_lib::company::plan::{DayPlan, Problem, Source};
use omsi_launcher_lib::company::pro_buses::{self as pb, Cannot, Warn};
use omsi_launcher_lib::company::pro_turnus as pt;
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// "05:30" (minutes of the service day; past midnight as the clock says it).
fn clock(m: i32) -> String {
    format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(60))
}

fn off(c: &Company) -> Option<Value> {
    (!co::pro_duties::on(c)).then(|| json!({ "off": true, "reading": tr("RosterFlow Pro is not the planner of this company: choose it on the planning page.") }))
}

/// "12 m", "Double-decker".
fn size_text(s: BusSize) -> String {
    match s {
        BusSize::Midi => trf("%{m} m", &[("m", "10")]),
        BusSize::Solo => trf("%{m} m", &[("m", "12")]),
        BusSize::Articulated => trf("%{m} m", &[("m", "18")]),
        BusSize::Double => tr("Double-decker"),
    }
}

/// What is in the bus's tank on the date shown (0 to 1): today as the company last knew it, a
/// day to come full from the depot.
fn level(c: &Company, v: &Vehicle, date: &str) -> f64 {
    if date == c.date {
        v.fuel.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// The bus's line of facts: "12 m · diesel · 92 %", "12 m · electric · 64 % · about 180 km".
fn facts(c: &Company, v: &Vehicle, date: &str) -> String {
    let l = level(c, v, date);
    let pct = num((l * 100.0).round(), 0);
    let size = size_text(v.kind.size);
    match v.kind.drive {
        Drive::Electric => trf("%{size} · electric · %{pct} % · about %{km} km", &[("size", &size), ("pct", &pct), ("km", &num((pb::range_km(v, l) / 10.0).round() * 10.0, 0))]),
        Drive::Cng => trf("%{size} · gas · %{pct} %", &[("size", &size), ("pct", &pct)]),
        Drive::Diesel => trf("%{size} · diesel · %{pct} %", &[("size", &size), ("pct", &pct)]),
    }
}

/// "På værkstedet til fre": the day's bar of a bus in the workshop - or None.
fn workshop_bar(v: &Vehicle, date: &str) -> Option<String> {
    v.in_workshop(date).then(|| {
        let until = v.workshop_until.clone().unwrap_or_default();
        trf("In the workshop until %{date}", &[("date", &tr(DAYS[pt::day_of(&until)]).to_lowercase())])
    })
}

/// "in the workshop until Fri" (the date's weekday, short) - or None.
fn workshop(v: &Vehicle, date: &str) -> Option<String> {
    v.in_workshop(date).then(|| {
        let until = v.workshop_until.clone().unwrap_or_default();
        trf("workshop until %{day}", &[("day", &tr(DAYS[pt::day_of(&until)]).to_lowercase())])
    })
}

/// "service in 240 km" when it is near (`pb::SERVICE_SOON_KM`), or None.
fn service(v: &Vehicle) -> Option<String> {
    let left = v.next_service_km - v.km;
    (left < pb::SERVICE_SOON_KM).then(|| if left <= 0.0 { tr("service due") } else { trf("service in %{km} km", &[("km", &num((left / 10.0).round() * 10.0, 0))]) })
}

/// What kind the filter chips count it as.
fn drive_key(v: &Vehicle) -> &'static str {
    match v.kind.drive {
        Drive::Electric => "el",
        Drive::Cng => "gas",
        Drive::Diesel => "diesel",
    }
}

/// Why a block has no bus, as the day says it.
fn why_none(c: &Company, dp: &DayPlan, ti: usize) -> String {
    let t = &dp.tours[ti];
    let meant = pb::pick_of(c, &dp.date, &t.tour.line, &t.tour.tour).map(|p| p.bus).unwrap_or_else(|| co::plan::roster(c, dp.weekday, &t.tour.line, &t.tour.tour).and_then(|r| r.bus));
    let number = meant.and_then(|id| c.vehicle(id)).map(|v| v.number.clone()).unwrap_or_default();
    match t.bus_problem {
        Some(Problem::Breakdown) => trf("Bus %{bus} did not start", &[("bus", &number)]),
        Some(Problem::Workshop) => trf("Bus %{bus} is in the workshop", &[("bus", &number)]),
        Some(Problem::Gone) => tr("Its bus is no longer in the fleet"),
        Some(Problem::BusBusy) => trf("Bus %{bus} is on another block then", &[("bus", &number)]),
        Some(Problem::NotLineBus) => trf("Bus %{bus} is not one of the line's buses", &[("bus", &number)]),
        Some(Problem::NoLineBus) => tr("The fleet has none of the line's buses"),
        _ if pb::pick_of(c, &dp.date, &t.tour.line, &t.tour.tour).is_some_and(|p| p.bus.is_none()) => tr("No bus, as chosen"),
        _ => tr("Not planned"),
    }
}

/// A block as the day draws it.
fn block(c: &Company, dp: &DayPlan, ti: usize) -> Value {
    let t = &dp.tours[ti];
    let hand = pb::pick_of(c, &dp.date, &t.tour.line, &t.tour.tour).is_some_and(|p| p.hand);
    let how = match t.bus_from {
        Source::Roster => tr("from the roster"),
        Source::Dispatcher if hand => tr("chosen by hand"),
        Source::Dispatcher => tr("assigned automatically"),
        Source::Central => tr("the central's"),
        Source::Auto => tr("the dispatcher's"),
        Source::None => String::new(),
    };
    json!({
        "ti": ti, "line": t.tour.line, "tour": t.tour.tour, "label": pb::label(&t.tour), "colour": colour_of(c, &t.tour.line), "number": t.tour.number.trim(),
        "from": t.tour.from(), "to": t.tour.to(), "start": clock(t.tour.from()), "end": clock(t.tour.to()), "km": num(pb::km_of(&t.tour).round(), 0), "hand": hand, "how": how,
    })
}

/// The blocks of plan `dp` the dispatch shows: the company's own, not a player's.
fn shown(dp: &DayPlan) -> Vec<usize> {
    let mut v: Vec<usize> = (0..dp.tours.len()).filter(|&i| !dp.tours[i].by_player && !dp.tours[i].tour.unplanned).collect();
    v.sort_by_key(|&i| dp.tours[i].tour.from());
    v
}

/// The fleet in number order.
fn fleet(c: &Company) -> Vec<&Vehicle> {
    let mut v: Vec<&Vehicle> = c.fleet.iter().collect();
    v.sort_by(|a, b| {
        let (x, y) = (a.number.trim().parse::<u64>().ok(), b.number.trim().parse::<u64>().ok());
        match (x, y) {
            (Some(x), Some(y)) => x.cmp(&y),
            _ => a.number.cmp(&b.number),
        }
    });
    v
}

/// The words of the tab.
fn words() -> Value {
    json!({
        "day": tr("Day"), "week": tr("Week"), "today": tr("Today"), "auto": tr("Assign buses automatically"), "redo": tr("Assign every bus again"),
        "redo_sure": tr("Give every block a bus again, also those chosen by hand?"), "missing": tr("Missing a bus"), "bus": tr("Bus"),
        "workshop": tr("In the workshop"), "charging": tr("Charging"), "none": tr("No buses yet: buy or lease them on the Fleet page."),
        "undo": tr("Undo"), "set": tr("Set on"), "swap": tr("Swap with"), "remove": tr("Remove"), "reset": tr("Back to the roster's"),
        "cannot": tr("%{n} buses cannot take it"), "close": tr("Close"), "blocks": tr("blocks"),
    })
}

/// The day, or the week.
fn view(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({ "none": true })) };
    if let Some(v) = off(&c) {
        return Ok(v);
    }
    let date = Some(str_arg(args, "date")).filter(|d| co::dates::parse(d).is_some()).unwrap_or_else(|| c.date.clone());
    let week = str_arg(args, "mode") == "week";
    let dates: Vec<String> = if week {
        let sunday = pt::sunday_of(&date);
        (0..7).map(|k| co::dates::add(&sunday, k)).collect()
    } else {
        vec![date.clone()]
    };
    let shown_as = if week {
        let (sunday, saturday) = (dates[0].clone(), dates[6].clone());
        trf("%{from} – %{to}", &[("from", &format!("{} {}", day_name(&sunday), short_date(&sunday))), ("to", &format!("{} {}", day_name(&saturday), short_date(&saturday)))])
    } else {
        format!("{} {}", day_name(&date), short_date(&date))
    };
    let mut out = json!({
        "date": date, "mode": if week { "week" } else { "day" }, "title": tr("Bus dispatch"), "words": words(),
        "day_name": shown_as,
        "is_today": date == c.date, "company_today": c.date,
        "prev": co::dates::add(&date, if week { -7 } else { -1 }), "next": co::dates::add(&date, if week { 7 } else { 1 }),
        "filters": [["all", tr("All")], ["diesel", tr("Diesel")], ["el", tr("Electric")], ["workshop", tr("In the workshop")]],
    });
    let plans = match plans_of(s, r, &dates) {
        Ok(p) => p,
        Err(e) => {
            out["reading"] = json!(e);
            return Ok(out);
        }
    };
    let c = s.company.clone().unwrap_or(c);
    if week {
        let days: Vec<Value> = dates.iter().map(|d| json!({ "date": d, "wd": tr(DAYS[pt::day_of(d)]), "day": short_date(d), "today": *d == c.date, "weekend": matches!(pt::day_of(d), 0 | 6) })).collect();
        let missing: Vec<Value> = dates
            .iter()
            .map(|d| {
                let dp = &plans[d];
                let n = shown(dp).into_iter().filter(|&i| dp.tours[i].bus.is_none()).count();
                json!({ "date": d, "n": n, "text": if n == 0 { "–".to_string() } else { trf("%{n} blocks", &[("n", &n.to_string())]) } })
            })
            .collect();
        let buses: Vec<Value> = fleet(&c)
            .into_iter()
            .map(|v| {
                let cells: Vec<Value> = dates
                    .iter()
                    .map(|d| {
                        let dp = &plans[d];
                        if v.in_workshop(d) {
                            return json!({ "date": d, "key": "workshop", "text": tr("Workshop") });
                        }
                        let labels: Vec<String> = shown(dp).into_iter().filter(|&i| pb::bus_of(&dp.tours[i]) == Some(v.id)).map(|i| pb::label(&dp.tours[i].tour)).collect();
                        if labels.is_empty() {
                            json!({ "date": d, "key": "free", "text": "–" })
                        } else {
                            json!({ "date": d, "key": "blocks", "text": labels.join(", ") })
                        }
                    })
                    .collect();
                json!({ "id": v.id, "number": v.number, "name": v.name, "facts": facts(&c, v, &date), "kind": drive_key(v), "workshop": workshop(v, &date), "service": service(v), "cells": cells })
            })
            .collect();
        out["days"] = json!(days);
        out["missing"] = json!(missing);
        out["buses"] = json!(buses);
        return Ok(out);
    }
    let dp = &plans[&date];
    let order = shown(dp);
    let missing: Vec<Value> = order
        .iter()
        .filter(|&&i| dp.tours[i].bus.is_none())
        .map(|&i| {
            let mut b = block(&c, dp, i);
            b["why"] = json!(why_none(&c, dp, i));
            b
        })
        .collect();
    let first = order.iter().map(|&i| dp.tours[i].tour.from()).min().unwrap_or(5 * 60);
    let last = order.iter().map(|&i| dp.tours[i].tour.to()).max().unwrap_or(23 * 60);
    let (axis_from, axis_to) = ((first / 60).min(4) * 60, ((last + 59) / 60).max(24) * 60);
    let downs: Vec<&pb::BusDown> = c.planning.ops.downs.iter().filter(|x| x.date == date).collect();
    let breaks = c.clock.today.as_ref().filter(|_| date == c.date).map(|t| t.breaks.clone()).unwrap_or_default();
    let buses: Vec<Value> = fleet(&c)
        .into_iter()
        .map(|v| {
            let mine: Vec<usize> = order.iter().copied().filter(|&i| pb::bus_of(&dp.tours[i]) == Some(v.id)).collect();
            let runs: Vec<pb::Run> = mine.iter().map(|&i| (dp.tours[i].tour.from(), dp.tours[i].tour.to(), pb::km_of(&dp.tours[i].tour))).collect();
            let charging: Vec<Value> = pb::charging(v, &runs).into_iter().filter(|g| g.1 > g.0).map(|(a, b)| json!({ "from": a, "to": b })).collect();
            let km: f64 = runs.iter().map(|r| r.2).sum();
            // (a bus broken down on its way today, and what took over: `clock::Broken`)
            let down = breaks.iter().find(|b| b.vehicle == v.id).map(|b| trf(if b.accident { "Accident at %{time}" } else { "Broke down at %{time}" }, &[("time", &clock(b.at))]))
                .or_else(|| downs.iter().find(|x| x.bus == v.id && x.why == pb::DownWhy::NoStart).map(|_| tr("Did not start this morning")));
            json!({
                "id": v.id, "number": v.number, "name": v.name, "facts": facts(&c, v, &date), "kind": drive_key(v), "electric": v.kind.drive == Drive::Electric,
                "workshop": workshop(v, &date), "shop": workshop_bar(v, &date), "service": service(v), "down": down, "km": if km > 0.0 { trf("%{km} km this day", &[("km", &num(km.round(), 0))]) } else { String::new() },
                "blocks": mine.iter().map(|&i| block(&c, dp, i)).collect::<Vec<_>>(), "charging": charging,
            })
        })
        .collect();
    out["axis"] = json!({ "from": axis_from, "to": axis_to, "hours": (axis_from / 60..=axis_to / 60).step_by(2).map(|h| json!({ "at": h * 60, "text": format!("{:02}", h % 24) })).collect::<Vec<_>>() });
    out["missing"] = json!(missing);
    out["missing_title"] = json!(trf("Missing a bus · %{n} blocks", &[("n", &missing.len().to_string())]));
    out["buses"] = json!(buses);
    Ok(out)
}

/// Why a bus cannot take a block, as the popup says it.
fn cannot_text(_c: &Company, why: &Cannot) -> String {
    match why {
        Cannot::Gone => tr("No longer in the fleet"),
        Cannot::Workshop { until } => trf("In the workshop until %{date}", &[("date", &short_date(until))]),
        Cannot::NoStart => tr("Did not start this morning"),
        Cannot::Unfit => tr("Not fit to run"),
        Cannot::NotLineBus => tr("Not one of the line's buses"),
        Cannot::Small { wants } => trf("Too small: the tour wants %{size}", &[("size", &size_text(*wants))]),
        Cannot::ZeroEmission => tr("The contract wants a zero-emission bus"),
        Cannot::Fuel => tr("The contract does not allow its fuel"),
        Cannot::Floor => tr("The contract does not allow its floor"),
        Cannot::Age => tr("Too old for the contract"),
        Cannot::Busy { with } => trf("Runs block %{block} then", &[("block", with)]),
        Cannot::Range { needs, has } => trf("Not enough in the tank: %{has} km of %{needs} km", &[("has", &num(has.round(), 0)), ("needs", &num(needs.round(), 0))]),
    }
}

/// What a bus that can take a block warns of, as the popup says it.
fn warn_text(w: &Warn) -> String {
    match w {
        Warn::ServiceSoon { km_left } => trf("Service due in %{km} km", &[("km", &num((km_left / 10.0).round() * 10.0, 0))]),
        Warn::Dirty => tr("Dirty inside"),
        Warn::Breakdowns { n } => trf("%{n} breakdowns", &[("n", &n.to_string())]),
        Warn::Bigger => tr("Bigger than the tour asks for"),
    }
}

/// The block of plan `dp` the arguments name.
fn find(dp: &DayPlan, line: &str, tour: &str) -> Option<usize> {
    let same = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
    dp.tours.iter().position(|t| same(&t.tour.line, line) && same(&t.tour.tour, tour))
}

/// A block's popup: its facts, who can take it.
fn pick(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    if let Some(v) = off(&c) {
        return Ok(v);
    }
    let date = str_arg(args, "date");
    let plans = match plans_of(s, r, std::slice::from_ref(&date)) {
        Ok(p) => p,
        Err(e) => return Ok(json!({ "reading": e })),
    };
    let dp = &plans[&date];
    let Some(ti) = find(dp, &str_arg(args, "line"), &str_arg(args, "tour")) else { return Ok(json!({ "gone": true })) };
    let t = &dp.tours[ti];
    let current = pb::bus_of(t);
    let wants = co::ownline::wanted(&c, &t.tour).map(size_text);
    let mut can = Vec::new();
    let mut cannot = Vec::new();
    for v in fleet(&c) {
        if Some(v.id) == current {
            continue;
        }
        let on_line = dp.tours.iter().any(|x| x.tour.line.eq_ignore_ascii_case(&t.tour.line) && pb::bus_of(x) == Some(v.id));
        match pb::check(&c, dp, ti, v) {
            Ok(w) => can.push(json!({ "id": v.id, "number": v.number, "name": v.name, "facts": facts(&c, v, &date), "warns": w.iter().map(warn_text).collect::<Vec<_>>(), "line": on_line })),
            Err(Cannot::Busy { .. }) if current.is_some() => {
                // (busy then: a swap of the two blocks' buses, when each can take the other's)
                let theirs = shown(dp).into_iter().find(|&j| j != ti && pb::bus_of(&dp.tours[j]) == Some(v.id) && !(t.tour.to() + co::staff::BUS_MARGIN <= dp.tours[j].tour.from() || dp.tours[j].tour.to() + co::staff::BUS_MARGIN <= t.tour.from()));
                let mine = current.and_then(|id| c.vehicle(id));
                let ok = theirs.zip(mine).is_some_and(|(j, me)| {
                    let theirs_others: Vec<(String, pb::Run)> = pb::runs_of(dp, v.id, Some(j));
                    let mine_others: Vec<(String, pb::Run)> = pb::runs_of(dp, me.id, Some(ti));
                    let gone = pb::broken(dp);
                    pb::fit(&c, &date, dp.today, &gone, &t.tour, v, &theirs_others).is_ok() && pb::fit(&c, &date, dp.today, &gone, &dp.tours[j].tour, me, &mine_others).is_ok()
                });
                match (ok, theirs) {
                    (true, Some(j)) => can.push(json!({ "id": v.id, "number": v.number, "name": v.name, "facts": facts(&c, v, &date), "warns": [], "line": on_line,
                        "swap": { "line": dp.tours[j].tour.line, "tour": dp.tours[j].tour.tour, "label": pb::label(&dp.tours[j].tour), "text": trf("Swap with %{block}", &[("block", &pb::label(&dp.tours[j].tour))]) } })),
                    _ => cannot.push(json!({ "id": v.id, "number": v.number, "facts": facts(&c, v, &date), "why": cannot_text(&c, &pb::check(&c, dp, ti, v).err().unwrap_or(Cannot::Gone)) })),
                }
            }
            Err(e) => cannot.push(json!({ "id": v.id, "number": v.number, "facts": facts(&c, v, &date), "why": cannot_text(&c, &e) })),
        }
    }
    // (those already on the line first, then without warnings, then by number - as the fleet is)
    can.sort_by_key(|x| (!x["line"].as_bool().unwrap_or(false), x.get("swap").is_some(), x["warns"].as_array().map_or(0, Vec::len)));
    let now = current.and_then(|id| c.vehicle(id)).map(|v| trf("now: bus %{bus}", &[("bus", &v.number)])).unwrap_or_else(|| why_none(&c, dp, ti));
    let picked = pb::pick_of(&c, &date, &t.tour.line, &t.tour.tour).is_some();
    let sub = [trf("%{km} km", &[("km", &num(pb::km_of(&t.tour).round(), 0))]), wants.map(|w| trf("wants %{size}", &[("size", &w)])).unwrap_or_default(), now].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
    Ok(json!({
        "date": date, "line": t.tour.line, "tour": t.tour.tour, "label": pb::label(&t.tour), "colour": colour_of(&c, &t.tour.line), "current": current, "picked": picked,
        "title": trf("Block %{block} · %{from}–%{to} · line %{n}", &[("block", &pb::label(&t.tour)), ("from", &clock(t.tour.from())), ("to", &clock(t.tour.to())), ("n", t.tour.number.trim())]),
        "sub": sub,
        "can": can, "cannot": cannot, "cannot_title": trf("%{n} buses cannot take it", &[("n", &cannot.len().to_string())]),
        "words": words(), "day": format!("{} {}", day_name(&date), short_date(&date)),
    }))
}

/// The choices of blocks as they are now (what undoes an act).
fn before(c: &Company, blocks: &[(String, String, String)]) -> Value {
    json!(blocks
        .iter()
        .map(|(d, l, t)| json!({ "date": d, "line": l, "tour": t, "pick": pb::pick_of(c, d, l, t).map(|p| json!({ "bus": p.bus, "hand": p.hand })) }))
        .collect::<Vec<_>>())
}

/// An act of the tab.
fn act(s: &mut Session, r: &mut Rf, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    if let Some(v) = off(&c) {
        return Ok(v);
    }
    let date = str_arg(args, "date");
    let (line, tour) = (str_arg(args, "line"), str_arg(args, "tour"));
    let what = str_arg(args, "act");
    let number = |c: &Company, id: Option<u32>| id.and_then(|i| c.vehicle(i)).map(|v| v.number.clone()).unwrap_or_default();
    match what.as_str() {
        "restore" => {
            let picks = args.get("picks").and_then(Value::as_array).cloned().unwrap_or_default();
            let done = company::act(s, |c| {
                for p in &picks {
                    let g = |k: &str| p.get(k).and_then(Value::as_str).unwrap_or("").to_string();
                    match p.get("pick").filter(|x| !x.is_null()) {
                        Some(x) => pb::set(c, &g("date"), &g("line"), &g("tour"), x.get("bus").and_then(Value::as_u64).map(|b| b as u32), x.get("hand").and_then(Value::as_bool).unwrap_or(true)),
                        None => pb::clear(c, &g("date"), &g("line"), &g("tour")),
                    }
                }
                Ok(())
            });
            return finish(s, done.map(|_| (tr("Undone."), Value::Null)));
        }
        "auto" => {
            let redo = args.get("redo").and_then(Value::as_bool).unwrap_or(false);
            let dates: Vec<String> = if args.get("week").and_then(Value::as_bool).unwrap_or(false) {
                let sunday = pt::sunday_of(&date);
                (0..7).map(|k| co::dates::add(&sunday, k)).collect()
            } else {
                vec![date.clone()]
            };
            let plans = match plans_of(s, r, &dates) {
                Ok(p) => p,
                Err(e) => return Ok(json!({ "say": { "text": e, "error": true } })),
            };
            let blocks: Vec<(String, String, String)> = dates.iter().flat_map(|d| plans[d].tours.iter().map(move |t| (d.clone(), t.tour.line.clone(), t.tour.tour.clone()))).collect();
            let undo = before(&c, &blocks);
            let mut n = 0;
            let done = company::act(s, |c| {
                for d in &dates {
                    n += pb::auto(c, &plans[d], redo);
                }
                Ok(())
            });
            let text = if n == 0 { tr("No block got a bus: none without one, or no bus can take them.") } else { trf("%{n} blocks got a bus.", &[("n", &n.to_string())]) };
            return finish(s, done.map(|_| (text, undo)));
        }
        _ => {}
    }
    let plans = match plans_of(s, r, std::slice::from_ref(&date)) {
        Ok(p) => p,
        Err(e) => return Ok(json!({ "say": { "text": e, "error": true } })),
    };
    let dp = &plans[&date];
    let Some(ti) = find(dp, &line, &tour) else { return Ok(json!({ "say": { "text": tr("That block is not in the day's plan."), "error": true } })) };
    let label = pb::label(&dp.tours[ti].tour);
    match what.as_str() {
        "set" => {
            let id = args.get("bus").and_then(Value::as_u64).unwrap_or(0) as u32;
            let Some(v) = c.vehicle(id) else { return Ok(json!({ "say": { "text": tr("That bus is not in the fleet."), "error": true } })) };
            if let Err(e) = pb::check(&c, dp, ti, v) {
                return Ok(json!({ "say": { "text": cannot_text(&c, &e), "error": true } }));
            }
            let undo = before(&c, &[(date.clone(), line.clone(), tour.clone())]);
            let text = trf("Bus %{bus} on block %{block}.", &[("bus", &v.number), ("block", &label)]);
            let done = company::act(s, |c| {
                pb::set(c, &date, &line, &tour, Some(id), true);
                Ok(())
            });
            finish(s, done.map(|_| (text, undo)))
        }
        "remove" => {
            let undo = before(&c, &[(date.clone(), line.clone(), tour.clone())]);
            let text = trf("Block %{block} has no bus now.", &[("block", &label)]);
            let done = company::act(s, |c| {
                pb::set(c, &date, &line, &tour, None, true);
                Ok(())
            });
            finish(s, done.map(|_| (text, undo)))
        }
        "reset" => {
            let undo = before(&c, &[(date.clone(), line.clone(), tour.clone())]);
            let text = trf("Block %{block} runs with the roster's bus again.", &[("block", &label)]);
            let done = company::act(s, |c| {
                pb::clear(c, &date, &line, &tour);
                Ok(())
            });
            finish(s, done.map(|_| (text, undo)))
        }
        "swap" => {
            let Some(tj) = find(dp, &str_arg(args, "with_line"), &str_arg(args, "with_tour")) else { return Ok(json!({ "say": { "text": tr("That block is not in the day's plan."), "error": true } })) };
            let other = &dp.tours[tj].tour;
            let undo = before(&c, &[(date.clone(), line.clone(), tour.clone()), (date.clone(), other.line.clone(), other.tour.clone())]);
            let text = trf("Blocks %{a} and %{b} swapped buses: %{x} and %{y}.", &[("a", &label), ("b", &pb::label(other)), ("x", &number(&c, pb::bus_of(&dp.tours[tj]))), ("y", &number(&c, pb::bus_of(&dp.tours[ti])))]);
            let done = company::act(s, |c| {
                pb::swap(c, dp, ti, tj);
                Ok(())
            });
            finish(s, done.map(|_| (text, undo)))
        }
        _ => Ok(json!({})),
    }
}

/// An act's answer: what it says and what undoes it, the plans made anew.
fn finish(s: &mut Session, done: Result<(String, Value), Value>) -> Answer {
    match done {
        Ok((text, undo)) => {
            s.plan_gen += 1;
            Ok(json!({ "edited": true, "say": { "text": text, "error": false }, "undo": undo }))
        }
        Err(v) => Ok(v),
    }
}

/// The commands of the tab (`super::run`).
pub(super) fn run(s: &mut Session, r: &mut Rf, cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "rf_buses" => view(s, r, args),
        "rf_bus_pick" => pick(s, r, args),
        "rf_bus_act" => act(s, r, args),
        _ => return None,
    })
}

/// The texts of the core it says (`pro_buses`).
#[cfg(test)]
pub(super) fn texts() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = DAYS.to_vec();
    v.extend([pb::DOWN_NO_START, pb::DOWN_WORKSHOP, pb::DOWN_COVERED]);
    v
}

#[cfg(test)]
mod tests {
    use super::super::live::Editor;
    use super::*;
    use omsi_launcher_lib::company::autopilot;
    use omsi_launcher_lib::company::market::{self, MarketBus, Payment};
    use omsi_launcher_lib::company::model::Planner;

    /// Line 5's company planning with RosterFlow Pro, two buses, its week read.
    fn session() -> Session {
        let (mut c, l) = super::super::tests::line5_company();
        assert_eq!(autopilot::set_planner(&mut c, Planner::Pro), Planner::Pro);
        c.cash += 5_000_000_00;
        let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        for _ in 0..2 {
            market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
        }
        for v in c.fleet.iter_mut() {
            (v.condition, v.clean, v.breakdowns, v.workshop_until) = (90.0, 90.0, 0, None);
            v.next_service_km = v.km + 20_000.0;
        }
        let id = c.id.clone();
        for n in 0.. {
            c.id = format!("{id}{n}");
            if (0..8).all(|k| co::plan::disruptions(&c, &co::dates::add(&c.date, k)).is_empty()) {
                break;
            }
        }
        let mut s = Session::default();
        s.company = Some(c);
        company::test_week(&mut s, vec![l]);
        s
    }

    /// A command through RosterFlow's own dispatcher (`rosterflow::run`), as the page sends it.
    fn cmd(s: &mut Session, r: &mut Rf, name: &str, args: Value) -> Value {
        super::super::run(s, r, Editor::Boss, name, &args).unwrap_or_else(|| panic!("{name} is not routed")).unwrap()
    }

    #[test]
    fn the_dispatch_shows_the_buses_their_blocks_and_who_can_take_a_block_and_its_acts_undo() {
        company::words::set_language("en");
        company::test_dry();
        let mut s = session();
        let mut r = Rf::default();
        let ids: Vec<u32> = s.company.as_ref().unwrap().fleet.iter().map(|v| v.id).collect();
        let date = s.company.as_ref().unwrap().date.clone();
        // the day: both blocks missing a bus, both buses free
        let v = cmd(&mut s, &mut r, "rf_buses", json!({}));
        assert!(v.get("reading").is_none(), "{v}");
        assert_eq!((v["missing"].as_array().map(Vec::len), v["buses"].as_array().map(Vec::len)), (Some(2), Some(2)), "{v}");
        assert_eq!(v["missing"][0]["label"], "5/01");
        assert_eq!(v["missing"][0]["why"], "Not planned");
        assert_eq!(v["missing_title"], "Missing a bus · 2 blocks");
        assert!(v["buses"][0]["facts"].as_str().unwrap().starts_with("12 m · diesel · "), "{}", v["buses"][0]["facts"]);
        // a block's popup: both buses can take it
        let p = cmd(&mut s, &mut r, "rf_bus_pick", json!({ "date": date, "line": "Linie5", "tour": "1" }));
        assert_eq!(p["can"].as_array().map(Vec::len), Some(2), "{p}");
        assert!(p["title"].as_str().unwrap().starts_with("Block 5/01 · 06:00–"), "{}", p["title"]);
        // bus 1 on block 5/01; not on 5/02 as well (the same time)
        let a = cmd(&mut s, &mut r, "rf_bus_act", json!({ "act": "set", "date": date, "line": "Linie5", "tour": "1", "bus": ids[0] }));
        assert_eq!(a["edited"], true, "{a}");
        let undo = a["undo"].clone();
        assert!(undo[0]["pick"].is_null());
        let v = cmd(&mut s, &mut r, "rf_buses", json!({}));
        assert_eq!(v["missing"].as_array().map(Vec::len), Some(1));
        assert_eq!(v["buses"][0]["blocks"][0]["label"], "5/01");
        assert_eq!(v["buses"][0]["blocks"][0]["how"], "chosen by hand");
        let no = cmd(&mut s, &mut r, "rf_bus_act", json!({ "act": "set", "date": date, "line": "Linie5", "tour": "2", "bus": ids[0] }));
        assert_eq!((no["say"]["error"].as_bool(), no["say"]["text"].as_str()), (Some(true), Some("Runs block 5/01 then")));
        let p = cmd(&mut s, &mut r, "rf_bus_pick", json!({ "date": date, "line": "Linie5", "tour": "2" }));
        assert_eq!((p["can"].as_array().map(Vec::len), p["cannot"][0]["why"].as_str()), (Some(1), Some("Runs block 5/01 then")), "{p}");
        // and the world drives it: today's plan
        let plan = company::plan_of(&mut s, &date).unwrap().unwrap();
        assert_eq!(plan.tours[0].bus, Some(co::plan::BusOf::Own(ids[0])));
        // a swap offered on the assigned block, with bus 2 once it runs 5/02
        cmd(&mut s, &mut r, "rf_bus_act", json!({ "act": "auto", "date": date }));
        let v = cmd(&mut s, &mut r, "rf_buses", json!({}));
        assert_eq!(v["missing"].as_array().map(Vec::len), Some(0), "{v}");
        let p = cmd(&mut s, &mut r, "rf_bus_pick", json!({ "date": date, "line": "Linie5", "tour": "1" }));
        assert_eq!(p["can"][0]["swap"]["label"], "5/02", "{p}");
        let x = cmd(&mut s, &mut r, "rf_bus_act", json!({ "act": "swap", "date": date, "line": "Linie5", "tour": "1", "with_line": "Linie5", "with_tour": "2" }));
        assert_eq!(x["edited"], true, "{x}");
        assert_eq!(cmd(&mut s, &mut r, "rf_buses", json!({}))["buses"][1]["blocks"][0]["label"], "5/01");
        // undone: back as before the first act (block 5/01 without a choice of the dispatch)
        cmd(&mut s, &mut r, "rf_bus_act", json!({ "act": "restore", "picks": undo }));
        let c = s.company.as_ref().unwrap();
        assert!(co::pro_buses::pick_of(c, &date, "Linie5", "1").is_none());
        // the week: seven days, from Sunday
        let w = cmd(&mut s, &mut r, "rf_buses", json!({ "mode": "week" }));
        assert_eq!((w["days"].as_array().map(Vec::len), w["days"][0]["wd"].as_str()), (Some(7), Some("Sun")), "{w}");
        assert_eq!(w["missing"].as_array().map(Vec::len), Some(7));
        assert_eq!(w["buses"][0]["cells"].as_array().map(Vec::len), Some(7));
    }
}

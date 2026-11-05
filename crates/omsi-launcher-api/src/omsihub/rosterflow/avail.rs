//! RosterFlow Pro's tab "6 Kan arbejde" (round H, NexiconDK 5 November 2026; the core's
//! `company::pro_avail`): the hourly paid and the players of Play together who drive, each a row,
//! the next 14 days (and the 14 after) - what each said he can that day: all day, from-to, not that
//! day, or nothing ("–"), who said it and when.
//!
//! - the tab (`rf_avail`: `page` 0 the next 14 days, 1 the 14 after): the days, the rows, which the
//!   one looking may change (the planner every row; a player only his own, unless he may plan).
//! - its act (`rf_avail_act`: `act` "set" | "clear", `driver`, `guest`, `date`, `kind` "all" |
//!   "window" | "not", `from`, `to` "HH:MM"): a day set or cleared.

use super::super::company::words::{short_date, tr, trf};
use super::super::company::{self, Session};
use super::live::Editor;
use super::{str_arg, usize_arg, Rf};
use crate::Answer;
use omsi_launcher_lib::company::pro_avail::{self as pa, By, Can};
use omsi_launcher_lib::company::pro_turnus as pt;
use omsi_launcher_lib::company::together::Perm;
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const FULL_DAYS: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

/// "onsdag 27 Mar" (the weekday lower case in the languages that write it so).
fn long_day(d: &str) -> String {
    let n = tr(FULL_DAYS[pt::day_of(d)]);
    let n = if ["", "en", "de"].contains(&company::words::language().as_str()) { n } else { n.to_lowercase() };
    format!("{n} {}", short_date(d))
}

/// "05:00" (minutes of the day; 24:00 the day's end).
fn hm(m: i32) -> String {
    format!("{:02}:{:02}", m / 60, m % 60)
}

/// Whether `by` may change the day of driver `driver` (`guest`: a player's).
fn may(c: &Company, by: Editor, driver: u32, guest: bool) -> bool {
    match by {
        Editor::Boss => true,
        Editor::Guest(no) => (guest && driver == no) || c.together.guests.iter().any(|g| g.no == no && g.can(Perm::Plans)),
    }
}

/// A day of a row as the tab shows it.
fn cell(c: &Company, rec: Option<&pa::Avail>, can: Option<Can>, guest: bool, date: &str) -> Value {
    let (key, text) = match can {
        Some(Can::AllDay) => ("all", tr(pa::ALL_DAY)),
        Some(Can::Window { from, to }) => ("window", pa::times(from, to)),
        Some(Can::Not) => ("not", tr(pa::CANNOT)),
        None => ("none", "–".to_string()),
    };
    let by = rec.map(|a| match a.by {
        By::Planner => tr(pa::BY_PLANNER),
        By::Driver => trf(if guest { pa::BY_PLAYER } else { pa::BY_DRIVER }, &[("time", &format!("{} {}", short_date(&co::clock::date_of(a.at)), co::clock::hhmm(a.at)))]),
    });
    let (from, to) = match can {
        Some(Can::Window { from, to }) => (hm(from), hm(to)),
        _ => ("05:00".into(), "15:00".into()),
    };
    let _ = c;
    json!({ "date": date, "key": key, "text": text, "by": by, "planner": rec.is_some_and(|a| a.by == By::Planner), "from": from, "to": to })
}

/// The tab: its days, its rows.
fn view(s: &mut Session, _r: &mut Rf, by: Editor, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({ "none": true })) };
    if !co::pro_duties::on(&c) {
        return Ok(json!({ "off": true, "reading": tr("RosterFlow Pro is not the planner of this company: choose it on the planning page.") }));
    }
    let page = usize_arg(args, "page").min(3) as i64;
    let first = co::dates::add(&c.date, pa::AHEAD * page);
    let dates: Vec<String> = (0..pa::AHEAD).map(|k| co::dates::add(&first, k)).collect();
    let days: Vec<Value> = dates.iter().map(|d| json!({ "date": d, "wd": tr(DAYS[pt::day_of(d)]), "day": short_date(d), "name": long_day(d), "today": *d == c.date, "weekend": matches!(pt::day_of(d), 0 | 6) })).collect();
    let mut rows = Vec::new();
    let mut staff: Vec<&co::model::Employee> = c.staff.iter().filter(|e| pa::says(e) && co::staff::drives(e)).collect();
    staff.sort_by_key(|e| e.id);
    for e in staff {
        let cells: Vec<Value> = dates.iter().map(|d| cell(&c, pa::record(&c, e.id, false, d), pa::can_of(&c, e, d), false, d)).collect();
        let n = cells.iter().filter(|x| x["key"] == "all" || x["key"] == "window").count();
        rows.push(json!({ "driver": e.id, "guest": false, "name": e.name, "sub": trf("chf. %{nr} · hourly paid", &[("nr", &e.id.to_string())]), "may": may(&c, by, e.id, false), "mine": false,
            "count": trf("can %{n} of %{of} days", &[("n", &n.to_string()), ("of", &dates.len().to_string())]), "cells": cells }));
    }
    for g in c.together.guests.iter().filter(|g| !g.banned && g.perms().contains(&Perm::Drive)) {
        let cells: Vec<Value> = dates.iter().map(|d| cell(&c, pa::record(&c, g.no, true, d), pa::guest_can(&c, g.no, d), true, d)).collect();
        let n = cells.iter().filter(|x| x["key"] == "all" || x["key"] == "window").count();
        let mine = by == Editor::Guest(g.no);
        rows.push(json!({ "driver": g.no, "guest": true, "name": g.name, "sub": if mine { tr("a player: you") } else { tr("a player: says it himself") }, "may": may(&c, by, g.no, true), "mine": mine,
            "count": trf("can %{n} of %{of} days", &[("n", &n.to_string()), ("of", &dates.len().to_string())]), "cells": cells }));
    }
    Ok(json!({
        "title": tr("When the hourly paid can work"),
        "lead": tr("Each hourly-paid driver says a day at a time when they can work: all day, from-to, or not that day. Asked for a duty within it they mostly say yes, and answer sooner; outside it, mostly no. Click a day to change it."),
        "empty": tr("No hourly-paid drivers yet: hire them on the Staff page."),
        "page": page, "pages": [tr("The next 14 days"), tr("The 14 days after")],
        "days": days, "rows": rows,
        "legend": [["all", tr(pa::ALL_DAY)], ["window", tr("From – to")], ["not", tr(pa::CANNOT)], ["none", tr("Nothing said (–)")]],
        "words": { "all": tr(pa::ALL_DAY), "window": tr("From – to"), "from": tr("From"), "to": tr("To"), "not": tr("Cannot that day"), "clear": tr("Clear"), "save": tr("Save"), "cancel": tr("Cancel"), "planner": tr("set by the planner"), "only_own": tr(pa::ONLY_OWN) },
    }))
}

/// A day set or cleared.
fn act(s: &mut Session, _r: &mut Rf, by: Editor, args: &Value) -> Answer {
    let Some(c) = s.company.clone() else { return Ok(json!({})) };
    let driver = args.get("driver").and_then(Value::as_u64).unwrap_or(0) as u32;
    let guest = args.get("guest").and_then(Value::as_bool).unwrap_or(false);
    let date = str_arg(args, "date");
    if !may(&c, by, driver, guest) {
        return Ok(json!({ "say": { "text": tr(pa::ONLY_OWN), "error": true } }));
    }
    // (a player's own day is his; the planner's is the planner's)
    let who = match by {
        Editor::Guest(no) if guest && driver == no => By::Driver,
        _ => By::Planner,
    };
    let can = match str_arg(args, "act").as_str() {
        "clear" => None,
        "set" => match str_arg(args, "kind").as_str() {
            "all" => Some(Can::AllDay),
            "not" => Some(Can::Not),
            "window" => match (pa::minutes(&str_arg(args, "from")), pa::minutes(&str_arg(args, "to"))) {
                (Some(f), Some(t)) => match pa::window(f, t) {
                    Ok(x) => Some(x),
                    Err(e) => return Ok(json!({ "say": { "text": tr(e), "error": true } })),
                },
                _ => return Ok(json!({ "say": { "text": tr(pa::BAD_TIMES), "error": true } })),
            },
            _ => return Ok(json!({})),
        },
        _ => return Ok(json!({})),
    };
    match company::act(s, |c| pa::set(c, driver, guest, &date, can, who)) {
        Ok(()) => {
            s.plan_gen += 1;
            Ok(json!({ "edited": true }))
        }
        Err(v) => Ok(v),
    }
}

/// The commands of the tab (`super::run`).
pub(super) fn run(s: &mut Session, r: &mut Rf, by: Editor, cmd: &str, args: &Value) -> Option<Answer> {
    Some(match cmd {
        "rf_avail" => view(s, r, by, args),
        "rf_avail_act" => act(s, r, by, args),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::autopilot;
    use omsi_launcher_lib::company::model::{Employment, Planner};

    #[test]
    fn the_tab_lists_the_hourly_paid_and_a_day_is_set_by_whom_may() {
        company::words::set_language("en");
        company::test_dry();
        let (mut c, _) = super::super::tests::line5_company();
        assert_eq!(autopilot::set_planner(&mut c, Planner::Pro), Planner::Pro);
        while c.staff.iter().filter(|e| co::staff::drives(e)).count() < 2 {
            for a in co::staff::applicants(&c) {
                if a.role == co::model::Role::Driver && c.staff.iter().filter(|e| co::staff::drives(e)).count() < 2 {
                    let _ = co::staff::hire(&mut c, &a);
                }
            }
            c.id.push('x');
            c.taken = Default::default();
        }
        let ids: Vec<u32> = c.staff.iter().filter(|e| co::staff::drives(e)).map(|e| e.id).collect();
        c.staff.iter_mut().find(|e| e.id == ids[1]).unwrap().employment = Employment::Hourly;
        c.together.guests.push(co::together::book::Guest { no: 4, name: "Ida".into(), grant: vec![Perm::Drive], ..Default::default() });
        let mut s = Session::default();
        s.company = Some(c);
        let mut r = Rf::default();
        // the tab: the hourly-paid driver (his days as he would fill them in) and the player
        let v = view(&mut s, &mut r, Editor::Boss, &json!({})).unwrap();
        assert_eq!(v["days"].as_array().map(Vec::len), Some(14));
        let rows = v["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2, "{v}");
        assert_eq!((rows[0]["driver"].as_u64(), rows[0]["guest"].as_bool(), rows[0]["may"].as_bool()), (Some(ids[1] as u64), Some(false), Some(true)));
        assert!(rows[0]["cells"].as_array().unwrap().iter().all(|x| x["key"] != "none"), "drawn: {}", rows[0]);
        assert_eq!(rows[1]["cells"][0]["text"], "–", "the player said nothing yet");
        // the planner sets 05-15 tomorrow; refused: too short
        let date = co::dates::add(&s.company.as_ref().unwrap().date, 1);
        let ok = act(&mut s, &mut r, Editor::Boss, &json!({ "act": "set", "driver": ids[1], "date": date, "kind": "window", "from": "05:00", "to": "15:00" })).unwrap();
        assert_eq!(ok["edited"], true, "{ok}");
        let v = view(&mut s, &mut r, Editor::Boss, &json!({})).unwrap();
        assert_eq!((v["rows"][0]["cells"][1]["text"].as_str(), v["rows"][0]["cells"][1]["by"].as_str()), (Some("05–15"), Some("Set by the planner")));
        let bad = act(&mut s, &mut r, Editor::Boss, &json!({ "act": "set", "driver": ids[1], "date": date, "kind": "window", "from": "05:00", "to": "05:30" })).unwrap();
        assert_eq!(bad["say"]["error"], true, "{bad}");
        // the player: his own, not another's
        let own = act(&mut s, &mut r, Editor::Guest(4), &json!({ "act": "set", "driver": 4, "guest": true, "date": date, "kind": "all" })).unwrap();
        assert_eq!(own["edited"], true, "{own}");
        let other = act(&mut s, &mut r, Editor::Guest(4), &json!({ "act": "set", "driver": ids[1], "date": date, "kind": "not" })).unwrap();
        assert_eq!(other["say"]["text"], pa::ONLY_OWN);
        let v = view(&mut s, &mut r, Editor::Guest(4), &json!({})).unwrap();
        assert_eq!((v["rows"][0]["may"].as_bool(), v["rows"][1]["may"].as_bool(), v["rows"][1]["mine"].as_bool()), (Some(false), Some(true), Some(true)));
        assert_eq!(v["rows"][1]["cells"][1]["text"], "All day");
        assert!(v["rows"][1]["cells"][1]["by"].as_str().unwrap().starts_with("Said by the player"));
        // cleared: "–"
        act(&mut s, &mut r, Editor::Boss, &json!({ "act": "clear", "driver": ids[1], "date": date })).unwrap();
        assert_eq!(view(&mut s, &mut r, Editor::Boss, &json!({})).unwrap()["rows"][0]["cells"][1]["key"], "none");
        // the 14 days after
        assert_eq!(view(&mut s, &mut r, Editor::Boss, &json!({ "page": 1 })).unwrap()["days"][0]["date"], co::dates::add(&s.company.as_ref().unwrap().date, 14));
    }
}

//! The Staff page's Ticket inspection tab (NexiconDK, ticket inspection round S, 9 November 2026;
//! `co::inspection`, `co::dodgers`): today's teams and the lines they check, the checks of a day
//! closed (when, the line, the trip, the team, the passengers checked, those caught and their
//! fines), the totals of the last 7 and 30 days, and every line's share of fare dodgers with its
//! trend and why. Before the research, and without inspectors, it says how to start.
//!
//! (round T, 10 November 2026) Its Plan part: the boss's inspection duties of the week (a grid of
//! the days) and of single dates, storaktions at a stop, what lacks inspectors in the days to come,
//! and what was planned against what was done (`co::inspection_plan`).

use super::super::kit::Go;
use super::super::words::{eur, grouped, local_day, local_weekday, num, tr, trf};
use omsi_launcher_lib::company::inspection_plan::{self as plan, Duty, Kind, StopLines};
use super::super::RESEARCH_TAB;
use omsi_launcher_lib::company::research::keys;
use omsi_launcher_lib::company::{self as co, Company, Role};
use serde_json::{json, Value};

/// The days of a line's trend.
const TREND_DAYS: usize = 30;
/// The closed days to pick from.
const PICK_DAYS: usize = 7;

fn hhmm(m: i32) -> String {
    co::clock::hhmm(m.rem_euclid(1440) as i64)
}

fn pct(x: f64) -> String {
    format!("{} %", num(100.0 * x, 1))
}

/// "Anna Berg & Ole Hansen"; more: "Anna Berg, Ole Hansen & Eva Lund".
fn team(names: &[String]) -> String {
    let names: Vec<&String> = names.iter().filter(|n| !n.is_empty()).collect();
    match names.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} & {last}", rest.iter().map(|n| n.as_str()).collect::<Vec<_>>().join(", ")),
        Some((last, _)) => last.to_string(),
        None => String::new(),
    }
}

/// A line's number (its name when it has none).
fn number_of(c: &Company, line: &str) -> String {
    c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(line)).map(|l| l.number.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| line.to_string())
}

/// A duty's hours: "16:00–22:00".
fn hours_of(from: i32, to: i32) -> String {
    format!("{}–{}", hhmm(from), hhmm(to))
}

/// A duty's lines in words: "134 · 131", or the busiest.
fn lines_text(c: &Company, d: &Duty) -> String {
    match d.kind {
        Kind::Action => d.action_lines().iter().map(|l| number_of(c, l)).collect::<Vec<_>>().join(" · "),
        Kind::Patrol if d.lines.is_empty() => tr("Busiest line"),
        Kind::Patrol => d.lines.iter().map(|l| number_of(c, l)).collect::<Vec<_>>().join(" · "),
    }
}

/// A duty's name in a sentence: "Kontrolvagt 134 16:00–22:00", "Storaktion Banegården 15:00–18:00".
pub fn duty_name(c: &Company, d: &Duty) -> String {
    match d.kind {
        Kind::Action => trf("Storaktion %{stop} %{time}", &[("stop", &d.stop), ("time", &hours_of(d.from, d.to))]),
        Kind::Patrol => trf("Inspection duty %{lines} %{time}", &[("lines", &lines_text(c, d)), ("time", &hours_of(d.from, d.to))]),
    }
}

/// A duty's name without its hours: "Kontrolvagt 134 · 153", "Storaktion Banegården".
pub fn duty_short(c: &Company, d: &Duty) -> String {
    match d.kind {
        Kind::Action => trf("Storaktion %{stop}", &[("stop", &d.stop)]),
        Kind::Patrol => trf("Inspection duty %{lines}", &[("lines", &lines_text(c, d))]),
    }
}

/// A duty for the page: what the grid shows, and its fields for the form.
fn duty_json(c: &Company, d: &Duty) -> Value {
    let names: Vec<String> = d.named.iter().filter_map(|id| c.employee(*id).map(|e| e.name.clone())).collect();
    let team = if names.is_empty() { trf("%{n} inspectors", &[("n", &d.size.to_string())]) } else { team(&names) };
    json!({
        "id": d.id, "days": d.days, "kind": if d.kind == Kind::Action { "action" } else { "patrol" },
        "lines": d.lines, "from": d.from, "to": d.to, "size": d.size, "named": d.named, "stop": d.stop, "through": d.through,
        "time": hours_of(d.from, d.to), "what": if d.kind == Kind::Action { d.stop.clone() } else { lines_text(c, d) },
        "via": if d.kind == Kind::Action { trf("Lines %{lines}", &[("lines", &lines_text(c, d))]) } else { String::new() },
        "team": team, "action": d.kind == Kind::Action,
    })
}

/// The Plan part: the week's duties by day, the dates planned otherwise, what lacks inspectors in
/// the next days, what was planned against what was done, and what the form chooses from.
fn plan_part(c: &Company, stops: &[StopLines]) -> Value {
    let p = &c.inspection_plan;
    let week: Vec<Value> = (0..7u8).map(|wd| Value::Array(p.duties.iter().filter(|d| d.days.contains(&wd)).map(|d| duty_json(c, d)).collect())).collect();
    let dates: Vec<Value> = p
        .dates
        .iter()
        .map(|x| json!({ "date": x.date, "label": local_day(&x.date, false), "duties": x.duties.iter().map(|d| duty_json(c, d)).collect::<Vec<_>>(), "auto": x.duties.is_empty() }))
        .collect();
    // (the next seven days: what lacks inspectors, what is dropped)
    let mut warnings: Vec<Value> = Vec::new();
    for k in 0..7 {
        let date = co::dates::add(&c.date, k);
        for a in plan::assigned(c, &date).unwrap_or_default() {
            if a.missing == 0 {
                continue;
            }
            let what = duty_name(c, &a.duty);
            let text = if a.dropped() {
                trf("%{day}: %{duty} has nobody - it is dropped", &[("day", &local_day(&date, false)), ("duty", &what)])
            } else if a.missing == 1 {
                trf("%{day}: %{duty} lacks 1 inspector", &[("day", &local_day(&date, false)), ("duty", &what)])
            } else {
                trf("%{day}: %{duty} lacks %{n} inspectors", &[("day", &local_day(&date, false)), ("duty", &what), ("n", &a.missing.to_string())])
            };
            warnings.push(json!({ "text": text, "tone": if a.dropped() { "danger" } else { "warn" } }));
        }
    }
    // (the last days closed: what was planned, who worked it, what it did)
    let mut done: Vec<Value> = Vec::new();
    for d in c.inspection.days.iter().rev().take(7) {
        for r in &d.planned {
            let checks: Vec<&co::inspection::Check> = d.checks.iter().filter(|k| k.duty == r.duty).collect();
            let what = if r.kind == Kind::Action {
                trf("Storaktion %{stop}", &[("stop", &r.stop)])
            } else if r.lines.is_empty() {
                tr("Busiest line")
            } else {
                r.lines.iter().map(|l| number_of(c, l)).collect::<Vec<_>>().join(" · ")
            };
            done.push(json!({
                "day": local_day(&d.date, false), "what": what, "time": hours_of(r.from, r.to),
                "team": if r.names.is_empty() { "–".to_string() } else { team(&r.names) },
                "staffed": format!("{} / {}", r.ids.len(), r.wants), "short": r.missing > 0,
                "trips": checks.len(), "checked": checks.iter().map(|k| k.checked).sum::<u32>(), "caught": checks.iter().map(|k| k.caught).sum::<u32>(),
            }));
        }
        // (the spare inspectors' automatic patrols of a planned day, a row a team)
        if !d.planned.is_empty() {
            let mut teams: Vec<Vec<u32>> = Vec::new();
            for k in d.checks.iter().filter(|k| k.duty == 0) {
                if !teams.contains(&k.team) {
                    teams.push(k.team.clone());
                }
            }
            for t in teams {
                let checks: Vec<&co::inspection::Check> = d.checks.iter().filter(|k| k.duty == 0 && k.team == t).collect();
                let mut numbers: Vec<String> = Vec::new();
                for k in &checks {
                    if !numbers.contains(&k.number) {
                        numbers.push(k.number.clone());
                    }
                }
                let (a, b) = (checks.iter().map(|k| k.at).min().unwrap_or(0), checks.iter().map(|k| if k.off > 0 { k.off } else { k.arr }).max().unwrap_or(0));
                done.push(json!({
                    "day": local_day(&d.date, false), "what": trf("Automatic %{lines}", &[("lines", &numbers.join(" · "))]), "time": hours_of(a, b),
                    "team": team(&checks[0].names), "staffed": "–", "short": false, "auto": true,
                    "trips": checks.len(), "checked": checks.iter().map(|k| k.checked).sum::<u32>(), "caught": checks.iter().map(|k| k.caught).sum::<u32>(),
                }));
            }
        }
    }
    let lines: Vec<Value> = c.lines.iter().map(|l| json!({ "name": l.name, "number": number_of(c, &l.name) })).collect();
    let inspectors: Vec<Value> = c.staff.iter().filter(|e| e.role == Role::Inspector && e.employed_on(&c.date)).map(|e| json!({ "id": e.id, "name": e.name })).collect();
    let stops: Vec<Value> = stops.iter().take(200).map(|st| json!({ "stop": st.stop, "lines": st.lines().iter().map(|l| number_of(c, l)).collect::<Vec<_>>().join(" · "), "through": st.passes })).collect();
    json!({
        "auto": p.duties.is_empty() && p.dates.iter().all(|x| x.duties.is_empty()),
        "auto_text": tr("Automatic (busiest lines first)"),
        "auto_note": tr("Nothing is planned: the inspectors work in teams of two, each team on a line of its own, the busiest lines first. A day with inspection duties follows them instead; inspectors without a duty that day patrol so, on the lines no duty checks."),
        "week_title": tr("Inspection duties of the week").to_uppercase(),
        "heads": (0..7).map(local_weekday).collect::<Vec<_>>(), "week": week,
        "add_patrol": tr("New inspection duty"), "add_action": tr("New storaktion"),
        "dates_title": tr("Dates planned otherwise").to_uppercase(), "dates": dates,
        "dates_note": tr("A date planned otherwise has its own inspection duties instead of its weekday's; without any it is automatic."),
        "add_date": tr("Plan a date otherwise"), "remove_date": tr("Back to the weekday's"), "date_auto": tr("Automatic (busiest lines first)"),
        "warn_title": tr("Missing inspectors").to_uppercase(), "warnings": warnings, "no_warnings": tr("Every inspection duty of the next 7 days has its inspectors."),
        "done_title": tr("Planned and done").to_uppercase(), "done": done, "done_note": tr("On a planned day the inspectors without a duty work their core hours and check the busiest lines no duty checks (rows marked automatic)."), "no_done": tr("Nothing planned in the last days."),
        "done_cols": [tr("Day"), tr("Inspection duty"), tr("Time"), tr("Team"), tr("Staffed"), tr("Buses checked"), tr("Caught")],
        "form": {
            "patrol": tr("Inspection duty"), "action": tr("Storaktion"),
            "kind": tr("Kind"), "days": tr("Days"), "date": tr("Date"), "from": tr("From"), "to": tr("To"),
            "lines": tr("Lines"), "busiest": tr("Busiest line"), "busiest_tip": tr("No line chosen: the busiest line no other team checks that day."),
            "team": tr("Team"), "size": tr("Inspectors"), "named": tr("Named inspectors"), "named_tip": tr("Named inspectors work it when they are at work and rested; free ones take the places of those away."),
            "stop": tr("Stop"), "stop_pick": tr("Choose a stop"), "no_stops": tr("The timetable is being read: the stops come with it."),
            "through": tr("Lines through the stop"), "action_tip": tr("Many inspectors at one stop board every bus of the company's lines passing it within the hours and check everyone aboard. More are caught, and every line through the stop keeps fewer dodgers for the days after."),
            "hours_tip": tr("2 to 10 hours; past midnight too."),
            "save": tr("Save"), "delete": tr("Delete"), "cancel": tr("Cancel"),
            "new_patrol": tr("New inspection duty"), "new_action": tr("New storaktion"), "edit": tr("Edit inspection duty"),
            "heads": (0..7).map(local_weekday).collect::<Vec<_>>(),
            "team_range": [plan::TEAM.0, plan::TEAM.1], "action_range": [plan::ACTION_TEAM.0, plan::ACTION_TEAM.1],
        },
        "lines": lines, "inspectors": inspectors, "stops": stops, "today": c.date,
    })
}

/// The stops of the timetable's trips (`lines` read), by the company line, the first stop and the
/// minute it leaves: where a boarding got on and off is named by them (`part`).
pub type TripStops = std::collections::HashMap<(String, String, i32), Vec<String>>;

/// `TripStops` of the timetable read.
pub fn trip_stops(c: &Company, lines: &[omsi_launcher_lib::LineInfo]) -> TripStops {
    let mut out = TripStops::new();
    for cl in &c.lines {
        for li in lines.iter().filter(|l| l.name.trim().eq_ignore_ascii_case(cl.name.trim())) {
            for y in li.tours.iter().flat_map(|t| t.trips.iter()) {
                let dep = (y.departure / 60.0).round() as i32;
                out.entry((cl.name.to_lowercase(), y.from.trim().to_lowercase(), dep)).or_insert_with(|| y.stops.iter().map(|x| x.name.trim().to_string()).collect());
            }
        }
    }
    out
}

/// A boarding's stretch in words: "Banegården → Rådhuspladsen (5 stops)" by the timetable's stops
/// (the trip's same count), else "Banegården → Skejby, stops 3–8"; a check of before (no
/// boarding): its trip.
fn stretch_text(k: &co::inspection::Check, names: &TripStops) -> String {
    let trip = if k.from.is_empty() { k.to.clone() } else { format!("{} → {}", k.from, k.to) };
    if k.off == 0 || k.stops < 2 {
        return trip;
    }
    let n = (k.alight - k.board).to_string();
    match names.get(&(k.line.to_lowercase(), k.from.trim().to_lowercase(), k.dep)).filter(|x| x.len() as u32 == k.stops) {
        Some(stops) => trf("%{from} → %{to} (%{n} stops)", &[("from", &stops[k.board as usize]), ("to", &stops[k.alight as usize]), ("n", &n)]),
        None => trf("%{trip}, stops %{a}–%{b}", &[("trip", &trip), ("a", &(k.board + 1).to_string()), ("b", &(k.alight + 1).to_string())]),
    }
}

/// The tab: `ui.day` the closed day picked (0 the last), `ui.insp_part` its part (0 the
/// overview, 1 the plan); `stops` the timetable's stops (`inspection_plan::stops_of`, empty while
/// it is read), `names` its trips' stops (`trip_stops`).
pub fn part(c: &Company, ui: &Value, stops: &[StopLines], names: &TripStops) -> Value {
    let shown = ui.get("insp_part").and_then(Value::as_u64).unwrap_or(0).min(1);
    let researched = co::research::has(c, keys::TICKET_INSPECTORS);
    let inspectors = c.staff.iter().filter(|e| e.role == Role::Inspector && e.employed_on(&c.date)).count();
    let each = co::inspection::fine(c);
    let easy = co::autopilot::on(c);
    let empty = if !researched {
        Some(json!({
            "text": tr("Ticket inspectors check the passengers' tickets on your lines: fewer people ride without one, and those caught pay a fine. Research ticket inspection first; then they can be hired on the Hire employees tab."),
            "go": if easy { Go::ToNormal(RESEARCH_TAB).json() } else { Go::Research.json() },
        }))
    } else if inspectors == 0 {
        Some(json!({
            "text": tr("The company may hire ticket inspectors now. They work in teams of two, each team on a line of its own - the busiest lines first - and a line they check keeps fewer dodgers for about two weeks."),
            "hire": tr("Hire ticket inspectors"),
            "hire_role": Role::ALL.iter().position(|r| *r == Role::Inspector).unwrap_or(0),
        }))
    } else {
        None
    };
    // (today's teams)
    let mut patrols = co::staff::patrols(c, &c.date);
    // (round T: on a day the boss planned, the spare inspectors' automatic patrols are marked)
    let planned_day = plan::duties_on(c, &c.date).is_some();
    // (a storaktion once, with every line through its stop)
    let mut seen_actions: Vec<u32> = Vec::new();
    patrols.retain(|p| {
        if p.stop.is_none() {
            return true;
        }
        let first = !seen_actions.contains(&p.duty);
        seen_actions.push(p.duty);
        first
    });
    let teams: Vec<Value> = patrols
        .iter()
        .map(|p| {
            let names: Vec<String> = p.ids.iter().filter_map(|id| c.employee(*id).map(|e| e.name.clone())).collect();
            let number = c.lines.iter().find(|l| l.name == p.line).map(|l| l.number.clone()).unwrap_or_else(|| p.line.clone());
            json!({ "line": number, "who": team(&names), "time": format!("{}–{}", hhmm(p.from), hhmm(p.to)), "alone": p.ids.len() < 2 && p.stop.is_none(), "stop": p.stop.as_ref().map(|st| trf("Storaktion %{stop}", &[("stop", st)])), "planned": p.duty != 0, "auto": planned_day && p.duty == 0 })
        })
        .collect();
    // (the closed days with checks to pick from, the last first)
    let days: Vec<&co::inspection::Day> = c.inspection.days.iter().rev().filter(|d| !d.checks.is_empty()).take(PICK_DAYS).collect();
    let pick = ui.get("day").and_then(Value::as_u64).unwrap_or(0) as usize;
    let pick = pick.min(days.len().saturating_sub(1));
    let row = |k: &co::inspection::Check| {
        // (round T: a boarding - when the team got on and off; a check of before, its trip)
        let time = if !k.stop.is_empty() {
            hhmm(k.at)
        } else if k.off > 0 {
            format!("{}–{}", hhmm(k.at), hhmm(k.off))
        } else {
            format!("{}–{}", hhmm(k.dep), hhmm(k.arr))
        };
        json!({
            "time": time, "line": k.number,
            "trip": if k.stop.is_empty() { stretch_text(k, names) } else if k.from.is_empty() { k.to.clone() } else { format!("{} → {}", k.from, k.to) },
            "who": team(&k.names), "checked": k.checked, "caught": k.caught,
            "fines": if k.caught > 0 { eur(k.caught as i64 * k.fine) } else { "–".into() },
        })
    };
    // (round T: a storaktion's buses together under its stop, where its first bus came)
    let checks: Vec<Value> = days
        .get(pick)
        .map(|d| {
            let mut out: Vec<Value> = Vec::new();
            let mut grouped_at: Vec<(u32, usize)> = Vec::new();
            for k in &d.checks {
                if k.stop.is_empty() {
                    out.push(row(k));
                    continue;
                }
                match grouped_at.iter().find(|x| x.0 == k.duty) {
                    Some(&(_, at)) => {
                        if let Some(rows) = out[at]["rows"].as_array_mut() {
                            rows.push(row(k));
                        }
                    }
                    None => {
                        let of: Vec<&co::inspection::Check> = d.checks.iter().filter(|x| x.duty == k.duty && !x.stop.is_empty()).collect();
                        let (checked, caught): (u32, u32) = (of.iter().map(|x| x.checked).sum(), of.iter().map(|x| x.caught).sum());
                        let (a, b) = d.planned.iter().find(|r| r.duty == k.duty).map(|r| (r.from, r.to)).unwrap_or((k.at, k.at));
                        out.push(json!({
                            "group": trf("Storaktion %{stop}  ·  %{time}", &[("stop", &k.stop), ("time", &hours_of(a, b))]),
                            "sum": trf("%{n} buses  ·  %{checked} checked  ·  %{caught} without a ticket", &[("n", &of.len().to_string()), ("checked", &checked.to_string()), ("caught", &caught.to_string())]),
                            "who": team(&k.names), "rows": [row(k)],
                        }));
                        grouped_at.push((k.duty, out.len() - 1));
                    }
                }
            }
            out
        })
        .unwrap_or_default();
    let day_sum = days.get(pick).map(|d| {
        let checked: u32 = d.checks.iter().map(|k| k.checked).sum();
        let caught: u32 = d.checks.iter().map(|k| k.caught).sum();
        trf("%{n} buses checked  ·  %{checked} passengers  ·  %{caught} without a ticket  ·  %{written} written, %{paid} paid", &[("n", &d.checks.len().to_string()), ("checked", &grouped(checked as f64)), ("caught", &caught.to_string()), ("written", &eur(d.written)), ("paid", &eur(d.paid))])
    });
    // (the totals: up to the last day closed)
    let last = c.inspection.days.last().map(|d| d.date.clone()).unwrap_or_else(|| co::dates::add(&c.date, -1));
    let totals: Vec<Value> = [(7, "Last 7 days"), (30, "Last 30 days")]
        .iter()
        .map(|(n, label)| {
            let (k, checked, caught, written, paid) = co::inspection::totals(c, &last, *n);
            let rate = if checked > 0 { pct(caught as f64 / checked as f64) } else { "–".into() };
            json!({ "label": tr(label), "checks": k, "checked": grouped(checked as f64), "caught": caught, "rate": rate, "written": eur(written), "paid": eur(paid) })
        })
        .collect();
    // (every line's share now, its trend and why)
    let watched = co::staff::inspected(c, &c.date);
    let mut lines: Vec<(f64, Value)> = c
        .lines
        .iter()
        .filter(|l| co::network::in_service(l, co::clock::moment(&c.date, 24 * 60 - 1)) || c.inspection.days.iter().rev().take(TREND_DAYS).any(|d| d.lines.iter().any(|x| x.line == l.name)))
        .map(|l| {
            let why = co::dodgers::of_line(c, &l.name);
            let trend: Vec<Value> = {
                let kept: Vec<&co::inspection::Day> = c.inspection.days.iter().rev().take(TREND_DAYS).collect();
                kept.into_iter().rev().map(|d| d.lines.iter().find(|x| x.line == l.name).filter(|x| x.passengers > 0).map_or(Value::Null, |x| json!((1000.0 * x.share()).round() / 1000.0))).collect()
            };
            let mut tags: Vec<Value> = Vec::new();
            // (the evening and night trips raise its share by a point or more)
            if why.raw - co::dodgers::BASE - co::dodgers::FULL * why.full >= 0.01 {
                tags.push(json!([tr("Evening and night buses"), "warn"]));
            }
            if why.full >= 0.1 {
                tags.push(json!([tr("Full buses"), "warn"]));
            }
            if let Some(x) = watched.iter().find(|x| x.0 == l.name) {
                tags.push(json!([trf("Checked today by %{n}", &[("n", &x.2.len().to_string())]), "ok"]));
            }
            if why.deterrence >= 0.02 {
                tags.push(json!([trf("Inspections keep %{n} off", &[("n", &format!("{:.0} %", 100.0 * why.deterrence))]), "ok"]));
            }
            let checked_days = c.inspection.days.iter().rev().take(14).filter(|d| d.lines.iter().any(|x| x.line == l.name && x.checked > 0.0)).count();
            (
                why.share,
                json!({
                    "number": l.number, "name": l.name, "share": pct(why.share), "frac": (why.share / co::dodgers::MAX).min(1.0),
                    "raw": trf("without inspections %{n}", &[("n", &pct(why.raw))]),
                    "tone": if why.share >= 0.07 { "danger" } else if why.share >= co::dodgers::BASE - 0.001 { "warn" } else { "ok" },
                    "tags": tags, "trend": trend,
                    "checked": trf("checked %{n} of the last 14 days", &[("n", &checked_days.to_string())]),
                }),
            )
        })
        .collect();
    lines.sort_by(|a, b| b.0.total_cmp(&a.0));
    let parts = [tr("Overview"), tr("Plan")];
    json!({
        "kind": "inspection",
        "empty": empty,
        "parts": parts, "part": shown,
        "plan": (shown == 1 && empty.is_none()).then(|| plan_part(c, stops)),
        "planned_tag": tr("planned"), "auto_tag": tr("automatic"),
        "today_title": format!("{}  ·  {}", tr("Today's teams"), local_day(&c.date, false)).to_uppercase(), "teams": teams,
        "no_teams": tr("No ticket inspectors at work today."),
        "alone": tr("alone"),
        "checks_title": tr("Checks").to_uppercase(),
        "days": days.iter().map(|d| local_day(&d.date, false)).collect::<Vec<_>>(), "day": pick,
        "day_label": days.get(pick).map(|d| local_day(&d.date, true)),
        "day_sum": day_sum, "checks": checks,
        "no_checks": tr("No checks yet: the day's close shows what the inspectors saw."),
        "cols": [tr("Time"), tr("Line"), tr("Bus trip"), tr("Team"), tr("Checked"), tr("Caught"), tr("Fines")],
        "totals_title": tr("Totals").to_uppercase(), "totals": totals,
        "total_cols": [tr("Buses checked"), tr("Passengers"), tr("Without a ticket"), tr("Share caught"), tr("Fines written"), tr("Paid")],
        "lines_title": tr("Fare dodgers by line").to_uppercase(), "lines": lines.into_iter().map(|x| x.1).collect::<Vec<_>>(),
        "lines_note": trf("About %{base} of the passengers ride without a ticket by day, more in the evening, at night and on full buses. A line the inspectors check keeps fewer for about two weeks. Each one caught pays %{fine}; about two thirds of the fines get paid.", &[("base", &format!("{:.0} %", 100.0 * co::dodgers::BASE)), ("fine", &eur(each))]),
        "trend_tip": tr("The last 30 days"),
    })
}

/// A duty as the page sent it (`duty`).
fn duty_arg(args: &Value) -> Duty {
    let d = args.get("duty").cloned().unwrap_or(Value::Null);
    let num = |k: &str| d.get(k).and_then(Value::as_i64).unwrap_or(0);
    let strs = |k: &str| d.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default();
    Duty {
        id: num("id") as u32,
        days: d.get("days").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as u8).collect()).unwrap_or_default(),
        kind: if d.get("kind").and_then(Value::as_str) == Some("action") { Kind::Action } else { Kind::Patrol },
        lines: strs("lines"),
        from: num("from") as i32,
        to: num("to") as i32,
        size: num("size").clamp(0, 99) as u8,
        named: d.get("named").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|x| x as u32).collect()).unwrap_or_default(),
        stop: d.get("stop").and_then(Value::as_str).unwrap_or("").to_string(),
        through: d.get("through").cloned().and_then(|v| serde_json::from_value(v).ok()).unwrap_or_default(),
    }
}

/// What the Plan part's buttons do (`people::act`): a duty kept (`date`: one of a date planned
/// otherwise) or taken out, a date planned otherwise or back to its weekday's.
pub fn act(s: &mut super::super::Session, what: &str, args: &Value) -> Result<Value, String> {
    let date = args.get("date").and_then(Value::as_str).filter(|d| !d.is_empty()).map(str::to_string);
    let r = match what {
        "insp_duty_save" => {
            let d = duty_arg(args);
            super::super::act(s, |c| plan::save(c, date.as_deref(), d).map(|_| ()))
        }
        "insp_duty_remove" => {
            let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
            super::super::act(s, |c| plan::remove(c, id))
        }
        "insp_date_add" => super::super::act(s, |c| plan::add_date(c, date.as_deref().unwrap_or(""))),
        "insp_date_remove" => super::super::act(s, |c| plan::remove_date(c, date.as_deref().unwrap_or(""))),
        _ => Ok(()),
    };
    Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})))
}

#[cfg(test)]
mod tests {
    /// A day is written as each language writes it.
    #[test]
    fn days_are_written_as_each_language_writes_them() {
        use super::super::super::words::{local_day, set_language};
        set_language("da");
        assert_eq!(local_day("2026-12-05", false), "lø 5. dec.");
        assert_eq!(local_day("2026-12-05", true), "lø 5. dec. 2026");
        set_language("de");
        assert_eq!(local_day("2026-05-03", false), "So. 3. Mai");
        set_language("fr");
        assert_eq!(local_day("2026-12-05", false), "sam. 5 déc.");
        set_language("en");
        assert_eq!(local_day("2026-12-05", false), "Sat 5 Dec");
    }

    /// Every text of the tab and of the inspectors' daily message is translated.
    #[test]
    fn every_inspection_text_is_translated() {
        let own = include_str!("inspection.rs");
        let own = &own[..own.find("#[cfg(test)]").unwrap()];
        let core = include_str!("../../../../../omsi-launcher-core/src/company/inspection.rs");
        let mut texts: Vec<String> = Vec::new();
        for (src, calls) in [(own, &["tr(\"", "trf(\""][..]), (core, &["Message::new(Kind::Staff, \""][..])] {
            for call in calls {
                let mut rest = src;
                while let Some(at) = rest.find(call) {
                    rest = &rest[at + call.len()..];
                    let text: String = rest.chars().take_while(|c| *c != '"').collect();
                    texts.push(text.clone());
                    // (a message's text is its second literal)
                    if call.starts_with("Message") {
                        let after = &rest[text.len() + 1..];
                        let start = after.find('"').unwrap() + 1;
                        texts.push(after[start..].chars().take_while(|c| *c != '"').collect());
                    }
                }
            }
        }
        texts.extend(["Last 7 days", "Last 30 days", "Ticket inspection", "Ticket inspection fines"].map(String::from));
        assert!(texts.len() > 30, "{texts:?}");
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl", "da"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing:\n{}", missing.len(), missing.join("\n"));
    }
}

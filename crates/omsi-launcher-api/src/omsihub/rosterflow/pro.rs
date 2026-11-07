//! RosterFlow Pro's tabs "1 Vognløb" and "2 Vagter" (step 2, NexiconDK 4 November 2026; the core's
//! `company::pro_duties`): the day's blocks with their relief points, the pieces they are cut into
//! and the pool of pieces without a duty, and the duties - their numbers, types, times, pieces and
//! the marks of the rules they break - as the page (`launcher/ui/js/omsihub/rosterflow/pro.js`)
//! draws them; and the acts that build them. Every edit is RosterFlow's own (`edit`): in the
//! draft, Undo and Save as every other.

use super::{colour_of, company, edit, issue_words, rules_applied, said, Cached, Rf, Session};
use super::super::company::words::{eur, num, tr, trf};
use omsi_launcher_lib::company::pro_dutyplan as dp;
use omsi_launcher_lib::LineInfo;
use omsi_launcher_lib::company::network::{PlannedTrip, TourOfDay};
use omsi_launcher_lib::company::pro_duties as pd;
use omsi_launcher_lib::company::rosterflow::{self as rf, Board, EvKind};
use omsi_launcher_lib::company::studio as st;
use omsi_launcher_lib::company::{self as co, Company};
use serde_json::{json, Value};

/// The day types' names (`pro_duties::day_type`).
const DAY_TYPES: [&str; 3] = ["Monday to Friday", "Saturday", "Sunday"];

/// A piece's letter on its block: A, B, C ...
fn letter(k: usize) -> String {
    if k < 26 {
        ((b'A' + k as u8) as char).to_string()
    } else {
        format!("{}", k + 1)
    }
}

/// (Round J) A block's short name: "131 · H02" (not "131/131H02").
fn block_label(t: &TourOfDay) -> String {
    dp::short_block(t)
}

/// The lines a piece runs, in words: its line, and the other lines its trips run ("131 → NE2 🌙").
fn lines_words(t: &TourOfDay, trips: &[PlannedTrip]) -> String {
    let (_, other) = dp::piece_lines(t, trips);
    let own = t.number.trim().to_string();
    if other.is_empty() {
        return own;
    }
    let o: Vec<String> = other.iter().map(|l| if l.night { format!("{} 🌙", l.number) } else { l.number.clone() }).collect();
    format!("{own} → {}", o.join(", "))
}

/// A piece on a duty's bar: "131 · H02 A", a night line it runs after it ("131 · H09 C → NE2 🌙")
/// - the day lines a tour also runs are in its whole label (`piece_label`).
fn bar_label(t: &TourOfDay, k: usize) -> String {
    let (_, other) = dp::piece_lines(t, trips_of(t, k));
    let night: Vec<String> = other.iter().filter(|l| l.night).map(|l| format!("{} 🌙", l.number)).collect();
    let head = format!("{} · {} {}", t.number.trim(), dp::short_tour(&t.number, &t.tour), letter(k));
    if night.is_empty() { head } else { format!("{head} → {}", night.join(", ")) }
}

/// A piece in words: its lines, its block and letter ("131 · H02 C", "131 → NE2 🌙 · H09 C").
fn piece_label(tours: &[TourOfDay], i: usize, k: usize) -> String {
    tours.get(i).map(|t| format!("{} · {} {}", lines_words(t, trips_of(t, k)), dp::short_tour(&t.number, &t.tour), letter(k))).unwrap_or_default()
}

fn trips_of(t: &TourOfDay, k: usize) -> &[PlannedTrip] {
    let r = co::network::duties_of(t).get(k).cloned().unwrap_or(0..0);
    t.trips.get(r).unwrap_or(&[])
}

fn line_number(t: &TourOfDay) -> String {
    t.number.trim().to_string()
}

/// The row of the board's duty piece (block `i`, piece `k`) is in.
fn row_of(board: &Board, i: usize, k: usize) -> Option<usize> {
    st::duty_of(&board.duties, i, k).and_then(|di| board.shaped.iter().position(|s| s.duty == Some(di)))
}

/// What the Vognløb and Vagter tabs draw of the day (`board_json`'s `pro`).
pub(super) fn pro_json(c: &Company, board: &Board) -> Value {
    let wd = board.weekday;
    let tours = &board.tours;
    let pool_rows: Vec<bool> = board.shaped.iter().map(|s| pd::is_pool_duty(c, wd, tours, s)).collect();
    // the relief points: where a bus's driver can change, and the lines there
    let mut points: Vec<(String, Vec<String>)> = Vec::new();
    let mut point = |name: &str, line: &str| {
        let name = name.trim();
        if name.is_empty() {
            return;
        }
        let at = match points.iter().position(|p| p.0.eq_ignore_ascii_case(name)) {
            Some(k) => k,
            None => {
                points.push((name.to_string(), Vec::new()));
                points.len() - 1
            }
        };
        if !points[at].1.iter().any(|l| l == line) {
            points[at].1.push(line.to_string());
        }
    };
    let blocks: Vec<Value> = tours
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let pieces = co::network::duties_of(t);
            let reliefs: Vec<Value> = (1..t.trips.len())
                .filter(|at| !pieces.iter().any(|r| r.start == *at))
                .filter(|&at| st::can_cut(c, tours, i, at).is_ok())
                .map(|at| {
                    point(&t.trips[at].from, &line_number(t));
                    json!({ "at": at, "time": (t.trips[at - 1].arr + t.trips[at].dep) / 2, "stop": t.trips[at].from.trim() })
                })
                .collect();
            let cuts: Vec<Value> = pieces
                .iter()
                .skip(1)
                .map(|r| {
                    point(&t.trips[r.start].from, &line_number(t));
                    let prev = r.start.checked_sub(1).and_then(|p| t.trips.get(p)).map(|x| x.arr).unwrap_or(t.trips[r.start].dep);
                    json!({ "at": r.start, "time": (prev + t.trips[r.start].dep) / 2, "stop": t.trips[r.start].from.trim() })
                })
                .collect();
            let parts: Vec<Value> = pieces
                .iter()
                .enumerate()
                .map(|(k, _)| {
                    let trips = trips_of(t, k);
                    let row = row_of(board, i, k);
                    let pool = row.is_some_and(|r| pool_rows[r]);
                    let number = row.filter(|_| !pool).map(|r| board.shaped[r].label(r));
                    json!({
                        "k": k, "letter": letter(k), "label": piece_label(tours, i, k),
                        "from": trips.first().map(|x| x.dep).unwrap_or(0), "to": trips.iter().map(|x| x.arr).max().unwrap_or(0),
                        "from_stop": trips.first().map(|x| x.from.trim().to_string()).unwrap_or_default(),
                        "to_stop": trips.last().map(|x| x.end().trim().to_string()).unwrap_or_default(),
                        "row": row, "number": number, "pool": pool,
                        "locked": rf::piece_locked(c, wd, tours, i, k),
                    })
                })
                .collect();
            let trips: Vec<Value> = t
                .trips
                .iter()
                .map(|x| json!({ "dep": x.dep, "arr": x.arr, "counts": x.counts(), "out": super::outbound(tours, &t.line, x), "from": x.from.trim(), "to": x.end().trim() }))
                .collect();
            json!({
                "i": i, "label": block_label(t), "line": t.line, "number": line_number(t), "colour": colour_of(c, &t.line),
                "from": t.from(), "to": t.to(), "km": (t.km() * 10.0).round() / 10.0,
                "trips": trips, "count": t.trips.iter().filter(|x| x.counts()).count(),
                "reliefs": reliefs, "cuts": cuts, "pieces": parts,
                "locked": rf::block_locked(c, wd, &t.line, &t.tour),
            })
        })
        .collect();
    // the pool
    let pool: Vec<Value> = board
        .shaped
        .iter()
        .enumerate()
        .filter(|(row, _)| pool_rows[*row])
        .filter_map(|(row, s)| {
            let sp = s.spans.first()?;
            let t = tours.get(sp.tour)?;
            let trips = trips_of(t, sp.duty);
            Some(json!({
                "row": row, "tour": sp.tour, "duty": sp.duty, "label": piece_label(tours, sp.tour, sp.duty),
                "number": line_number(t), "colour": colour_of(c, &t.line),
                "from": trips.first().map(|x| x.dep).unwrap_or(sp.from), "to": trips.iter().map(|x| x.arr).max().unwrap_or(sp.to),
                "from_stop": trips.first().map(|x| x.from.trim().to_string()).unwrap_or_default(),
                "to_stop": trips.last().map(|x| x.end().trim().to_string()).unwrap_or_default(),
                "runs_as": s.label(row),
            }))
        })
        .collect();
    // the duties
    // (round J: in the order they sign on)
    let mut duties: Vec<Value> = board.shaped.iter().enumerate().filter(|(row, s)| !pool_rows[*row] && s.spare.is_none()).map(|(row, s)| duty_json(c, board, row, s)).collect();
    duties.sort_by_key(|d| (d["start"].as_i64().unwrap_or(0), d["number"].as_str().unwrap_or("").to_string()));
    // the lines and how many blocks each has
    let mut lines: Vec<Value> = Vec::new();
    for t in tours {
        if lines.iter().any(|l| l["line"] == json!(t.line)) {
            continue;
        }
        let n = tours.iter().filter(|o| o.line.eq_ignore_ascii_case(&t.line)).count();
        let caption = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&t.line)).map(|l| l.caption.clone()).unwrap_or_default();
        lines.push(json!({ "line": t.line, "number": line_number(t), "colour": colour_of(c, &t.line), "blocks": n, "caption": caption }));
    }
    let p = &c.planning.flow.prefs;
    let dt = pd::day_type(c, wd) as usize;
    let departures = departures_json(c, board, &pool_rows);
    json!({
        "on": true,
        "departures": departures,
        "day_type": tr(DAY_TYPES[dt.min(2)]),
        "base": pd::base(c, wd),
        "blocks": blocks,
        "pool": pool,
        "duties": duties,
        "lines": lines,
        "points": points.iter().map(|(name, ls)| json!({ "name": name, "lines": ls.join(", ") })).collect::<Vec<_>>(),
        "rules": {
            "same_stop": p.change.same_stop, "other_stop": p.change.other_stop, "sign_on": p.sign_on, "sign_off": p.sign_off,
            "continuous": rf::hm(p.continuous.value), "spread": rf::hm(pd::SPLIT_SPREAD), "split_gap": rf::hm(pd::SPLIT_GAP), "short": rf::hm(pd::SHORT_DUTY),
            "meal": p.meal.value, "meal_after": p.meal_after,
        },
        // (round J: the planner's goals for "Make duties automatically")
        "goals": {
            "target_from": p.goals.target_from, "target_to": p.goals.target_to, "longest": p.goals.longest,
            "min_piece": p.min_piece.value, "same_line": p.goals.same_line, "weekday_splits": true,
        },
        "trips": board.tours.iter().map(|t| t.trips.iter().filter(|x| x.counts()).count()).sum::<usize>(),
    })
}

/// A duty of the Vagter tab.
fn duty_json(c: &Company, board: &Board, row: usize, s: &rf::Shaped) -> Value {
    let wd = board.weekday;
    let tours = &board.tours;
    let kind = pd::kind_of(s);
    let mut spans = s.spans.clone();
    spans.sort_by_key(|x| x.from);
    let pieces: Vec<Value> = spans
        .iter()
        .map(|sp| {
            let t = &tours[sp.tour];
            let trips = trips_of(t, sp.duty);
            json!({
                "tour": sp.tour, "duty": sp.duty, "label": piece_label(tours, sp.tour, sp.duty), "short": bar_label(t, sp.duty), "number": line_number(t), "colour": colour_of(c, &t.line),
                "from": sp.from, "to": sp.to,
                "from_stop": trips.first().map(|x| x.from.trim().to_string()).unwrap_or_default(),
                "to_stop": trips.last().map(|x| x.end().trim().to_string()).unwrap_or_default(),
            })
        })
        .collect();
    let lines: Vec<&str> = spans.iter().map(|sp| tours[sp.tour].line.as_str()).collect();
    let line_change = lines.windows(2).any(|w| !w[0].eq_ignore_ascii_case(w[1]));
    // between the pieces: a walk to another stop, a break, the split's gap
    let mut between: Vec<Value> = Vec::new();
    let mut words: Vec<String> = Vec::new();
    let on_at = s.events.iter().find(|e| e.kind == EvKind::SignOn).map(|e| e.from).unwrap_or(s.start);
    let off_at = s.events.iter().rev().find(|e| e.kind == EvKind::SignOff).map(|e| e.to).unwrap_or(s.end);
    words.push(trf("Signs on %{time}", &[("time", &rf::clock(on_at))]));
    for (k, sp) in spans.iter().enumerate() {
        let t = &tours[sp.tour];
        let trips = trips_of(t, sp.duty);
        let to_stop = trips.last().map(|x| x.end().trim().to_string()).unwrap_or_default();
        words.push(trf("line %{n} vehicle %{v} piece %{p} to %{stop} %{time}", &[("n", &line_number(t)), ("v", &block_label(t)), ("p", &letter(sp.duty)), ("stop", &to_stop), ("time", &rf::clock(sp.to))]));
        if let Some(next) = spans.get(k + 1) {
            let nt = &tours[next.tour];
            let from_stop = trips_of(nt, next.duty).first().map(|x| x.from.trim().to_string()).unwrap_or_default();
            let gap = next.from - sp.to;
            let walk = !from_stop.eq_ignore_ascii_case(&to_stop);
            let what = if gap >= pd::SPLIT_GAP { "split" } else if walk { "walk" } else { "break" };
            between.push(json!({ "from": sp.to, "to": next.from, "kind": what, "minutes": gap }));
            if walk {
                let need = c.planning.flow.prefs.change.other_stop.max(rf::travel_between(c, tours, &to_stop, &from_stop, None).1);
                words.push(trf("walks %{m} min to %{stop}", &[("m", &need.to_string()), ("stop", &from_stop)]));
            }
            if gap >= pd::SPLIT_GAP {
                words.push(trf("off duty %{time}", &[("time", &rf::hm(gap))]));
            } else if !walk {
                words.push(trf("break %{time}", &[("time", &rf::hm(gap))]));
            }
        }
    }
    let off_stop = spans.last().and_then(|sp| trips_of(&tours[sp.tour], sp.duty).last().map(|x| x.end().trim().to_string())).unwrap_or_default();
    words.push(trf("signs off %{time} at %{stop}", &[("time", &rf::clock(off_at)), ("stop", &off_stop)]));
    // the marks: RosterFlow's issues and Pro's own
    let issues: Vec<&rf::Issue> = s.issues.iter().map(|&i| &board.issues[i]).filter(|i| !rf::dismissed(c, wd, &i.key)).collect();
    let marks = pd::marks(c, board, row);
    let mut tips: Vec<String> = issues.iter().map(|i| issue_words(i)).collect();
    tips.extend(marks.iter().map(|m| {
        let args: Vec<(&str, &str)> = m.args.iter().map(|(k, v)| (*k, v.as_str())).collect();
        trf(m.text, &args)
    }));
    let bad = issues.iter().any(|i| i.hard) || marks.iter().any(|m| m.hard);
    let warn = !bad && (!issues.is_empty() || !marks.is_empty());
    let level = if bad { "bad" } else if warn { "warn" } else { "ok" };
    if level == "ok" {
        words.push(tr("all rules kept"));
    }
    // (round J: the rules as a chip and a checklist, the duty's break, its breaks on the axis)
    let (soft, hard) = dp::rule_counts(c, board, row);
    let chip = if hard > 0 { format!("✗ {hard}") } else if soft > 0 { format!("! {soft}") } else { tr("✓ all") };
    let checks: Vec<Value> = dp::checklist(c, board, row)
        .iter()
        .map(|k| {
            let a: Vec<(&str, &str)> = k.args.iter().map(|(x, v)| (*x, v.as_str())).collect();
            json!({ "title": tr(k.title), "level": k.level.key(), "text": trf(k.text, &a) })
        })
        .collect();
    let meal = s.plan.breaks.iter().find(|b| b.meal && b.kind != rf::BreakKind::Split);
    let split = s.splits.iter().max_by_key(|g| g.1 - g.0);
    let pause = match (meal, split) {
        (Some(m), _) => format!("{} min · {}", m.to - m.from, rf::clock(m.from)),
        (None, Some(g)) if kind == pd::Kind::Split => trf("split %{time}", &[("time", &rf::hm(g.1 - g.0))]),
        (None, Some(g)) => trf("unpaid %{time}", &[("time", &rf::hm(g.1 - g.0))]),
        _ => "–".to_string(),
    };
    // (a break between two pieces a P on the axis; a meal break inside a piece - the bus standing
    // at its terminus - a small P over it; the other layovers not drawn)
    let inside = |b: &rf::DutyBreak| spans.iter().any(|sp| b.from >= sp.from && b.to <= sp.to);
    let breaks: Vec<Value> = s.plan.breaks.iter().filter(|b| (b.resets || b.meal) && b.kind != rf::BreakKind::Split && (b.meal || !inside(b))).map(|b| json!({ "from": b.from, "to": b.to, "meal": b.meal, "inside": inside(b) })).collect();
    let on_stop = spans.first().and_then(|sp| trips_of(&tours[sp.tour], sp.duty).first().map(|x| x.from.trim().to_string())).unwrap_or_default();
    let title = trf("Duty %{d} · %{kind}", &[("d", &s.label(row)), ("kind", &tr(kind.label()))]);
    let sub = trf("Signs on %{on} at %{a} · off %{off} at %{b} · %{n} pieces, %{c} changes of bus", &[("on", &rf::clock(on_at)), ("a", &on_stop), ("off", &rf::clock(off_at)), ("b", &off_stop), ("n", &spans.len().to_string()), ("c", &s.changeovers.to_string())]);
    json!({
        "row": row, "number": s.label(row), "kind": kind.key(), "kind_label": tr(kind.label()),
        "start": on_at, "end": off_at, "work": s.work, "paid": s.paid, "spread": s.spread(),
        "pieces": pieces, "between": between, "line_change": line_change,
        "level": level, "tip": tips.join("\n"), "detail": words.join("  ·  "),
        "locked": s.locked,
        "soft": soft, "hard": hard, "chip": chip, "checks": checks, "pause": pause, "breaks": breaks,
        "title": title, "sub": sub,
        // (round K: its own name, empty when it has none)
        "name": s.name.clone().unwrap_or_default(),
    })
}

/// (Round K) Every trip of the day for the "Afgange" tab, in order of departure: its block and
/// place on it, its times and stops, the line its displays show (a night line marked), whether it
/// has passengers, and the duty it is in (its name or number; none for a piece in the pool).
fn departures_json(c: &Company, board: &Board, pool_rows: &[bool]) -> Vec<Value> {
    let tours = &board.tours;
    let mut out: Vec<(i32, Value)> = Vec::new();
    for (i, t) in tours.iter().enumerate() {
        let pieces = co::network::duties_of(t);
        for (j, x) in t.trips.iter().enumerate() {
            let k = pieces.iter().position(|r| r.contains(&j)).unwrap_or(0);
            let row = row_of(board, i, k);
            let pool = row.is_some_and(|r| pool_rows.get(r).copied().unwrap_or(false));
            let duty = row.filter(|_| !pool).map(|r| board.shaped[r].label(r));
            let shown = co::specials::display_number(&x.line);
            let line = if shown.is_empty() { line_number(t) } else { shown };
            let night = !line.eq_ignore_ascii_case(&line_number(t)) && (co::specials::night_name(&line) || { let h = x.dep.rem_euclid(1440); h >= 22 * 60 || h < 5 * 60 });
            let relief = j == 0 || rf::relief_ok(c, tours, &t.trips[j - 1], x);
            out.push((x.dep, json!({
                "i": i, "j": j, "k": k, "dep": x.dep, "arr": x.arr,
                "from": x.from.trim(), "to": x.end().trim(),
                "line": line, "night": night, "own": line_number(t), "colour": colour_of(c, &t.line),
                "counts": x.counts(), "block": block_label(t),
                "duty": duty, "row": row, "pool": pool,
                "first": pieces.get(k).is_some_and(|r| r.start == j), "relief": relief,
            })));
        }
    }
    out.sort_by_key(|x| x.0);
    out.into_iter().map(|x| x.1).collect()
}

/// The departures `trips` (block, trip) of an act (`[[i, j], ...]`).
fn trips_arg(args: &Value) -> Vec<(usize, usize)> {
    args.get("trips").and_then(Value::as_array).map(|a| a.iter().filter_map(|p| Some((p.get(0)?.as_u64()? as usize, p.get(1)?.as_u64()? as usize))).collect()).unwrap_or_default()
}

/// (Round K) The duty the departures chosen in "Afgange" would make, as the Vagter tab shows a
/// duty - its times, break, type, rules and price - nothing kept; or why it cannot be made.
fn build_preview(k: &Cached, trips: &[(usize, usize)], name: &str) -> Value {
    let board = &k.board;
    let mut v = (*k.view).clone();
    let wd = board.weekday;
    match pd::build_duty(&mut v, wd, &board.tours, trips, name) {
        Err(why) => json!({ "error": tr(why) }),
        Ok(pieces) => {
            let tours = co::network::tours_of_day(&v, &k.lines, &k.date);
            let after = rf::board_of(&v, wd, tours, Vec::new());
            let Some(first) = pieces.first().and_then(|p| st::piece_at(&after.tours, p)) else { return json!({ "error": tr(st::NOT_FOUND) }) };
            let Some(row) = row_of(&after, first.0, first.1) else { return json!({ "error": tr(st::NOT_FOUND) }) };
            let mut d = duty_json(&v, &after, row, &after.shaped[row]);
            let mut list = json!([d.clone()]);
            prices(&mut list, &v, &after, &k.date, dp::hour_wage(&v));
            d = list[0].clone();
            json!({ "duty": d })
        }
    }
}

/// (Round J) What the Vagter tab shows of the day that needs its date and timetable (`ensure`
/// adds it to the board's JSON, `json`): each duty's price and its parts, the day in figures, and
/// the game's own plan of the day (`dp::game_plan`) - its figures, its duties - with each figure's
/// difference from it.
pub(super) fn add_day(json: &mut Value, c: &Company, board: &Board, lines: &[LineInfo], date: &str) {
    if json.get("pro").is_none_or(Value::is_null) {
        return;
    }
    let hour = dp::hour_wage(c);
    prices(&mut json["pro"]["duties"], c, board, date, hour);
    let now = dp::summary(c, board, date);
    let (g, gb) = dp::game_plan(c, lines, date);
    let was = dp::summary(&g, &gb, date);
    let mut plan_duties: Value = json!(pro_json(&g, &gb)["duties"].clone());
    prices(&mut plan_duties, &g, &gb, date, hour);
    json["pro"]["summary"] = summary_json(&now, Some(&was));
    json["pro"]["plan"] = json!({
        "summary": summary_json(&was, None),
        "duties": plan_duties,
        "line": trf("The game's plan: %{n} duties, %{h}, %{r} red", &[("n", &was.duties.to_string()), ("h", &hours(was.paid)), ("r", &was.bad.to_string())]),
    });
}

/// Hours in words: "338 h".
fn hours(minutes: i32) -> String {
    trf("%{h} h", &[("h", &num(minutes as f64 / 60.0, 0))])
}

/// Each duty's price (`dp::price`) on the duties' JSON (`duties`, by their rows).
fn prices(duties: &mut Value, c: &Company, board: &Board, date: &str, hour: f64) {
    let Some(list) = duties.as_array_mut() else { return };
    for d in list {
        let Some(s) = d["row"].as_u64().and_then(|r| board.shaped.get(r as usize)) else { continue };
        let pr = dp::price(c, s, date, hour);
        d["price"] = json!(eur(pr.total));
        d["price_cents"] = json!(pr.total);
        let mut parts = vec![json!({ "label": trf("Wage %{t} × %{rate}", &[("t", &rf::hm(pr.paid)), ("rate", &eur(pr.hour.round() as i64))]), "value": eur(pr.wages) })];
        if pr.supplements > 0 {
            parts.push(json!({ "label": trf("Supplements, evening, night and weekend (%{t})", &[("t", &rf::hm(pr.supplement_minutes))]), "value": eur(pr.supplements) }));
        }
        parts.push(json!({ "label": tr("Pension, holiday pay and other employer's costs"), "value": eur(pr.oncosts) }));
        d["price_parts"] = json!(parts);
        d["price_total"] = json!(eur(pr.total));
    }
}

/// The day in figures (`now`), each with its difference from the game's plan (`was`) and whether
/// that is better: fewer duties, fewer hours, more of them driving, cheaper, fewer red.
fn summary_json(now: &dp::Summary, was: Option<&dp::Summary>) -> Value {
    let figure = |key: &str, label: &str, value: String, diff: Option<(f64, String)>, lower_better: Option<bool>| {
        let (d, words) = diff.unwrap_or((0.0, String::new()));
        let good = match lower_better {
            _ if d.abs() < 1e-9 => Value::Null,
            Some(true) => json!(d < 0.0),
            Some(false) => json!(d > 0.0),
            None => Value::Null,
        };
        json!({ "key": key, "label": tr(label), "value": value, "diff": if d.abs() < 1e-9 { String::new() } else { words }, "good": good })
    };
    let signed = |x: f64, s: String| if x > 0.0 { format!("+{s}") } else if x < 0.0 { format!("−{}", s.trim_start_matches('-')) } else { s };
    let d = |f: &dyn Fn(&dp::Summary) -> f64, show: &dyn Fn(f64) -> String| was.map(|w| {
        let x = f(now) - f(w);
        (x, signed(x, show(x.abs())))
    });
    let figures = vec![
        figure("duties", FIGURES[0], now.duties.to_string(), d(&|s| s.duties as f64, &|x| format!("{x}")), Some(true)),
        figure("paid", FIGURES[1], hours(now.paid), d(&|s| s.paid as f64 / 60.0, &|x| trf("%{h} h", &[("h", &num(x, 0))])), Some(true)),
        figure("driving", FIGURES[2], format!("{} %", num(now.driving_share(), 0)), d(&|s| s.driving_share().round(), &|x| format!("{} %", num(x, 0))), Some(false)),
        figure("cost", FIGURES[3], eur(now.cost), d(&|s| s.cost as f64, &|x| eur(x.round() as i64)), Some(true)),
        figure("average", FIGURES[4], rf::hm(now.average), d(&|s| s.average as f64, &|x| rf::hm(x.round() as i32)), None),
    ];
    let status = json!({ "ok": now.ok, "warn": now.warn, "bad": now.bad, "diff_bad": was.map(|w| now.bad as i64 - w.bad as i64) });
    json!({ "figures": figures, "status": status })
}

/// The acts of the Pro tabs (`rf_act`'s `pro_*`), on the day `k` shows.
pub(super) fn act(s: &mut Session, r: &mut Rf, k: &Cached, what: &str, args: &Value) -> Value {
    let board = k.board.clone();
    let tours = board.tours.clone();
    let wd = board.weekday;
    let u = |name: &str| args.get(name).and_then(Value::as_u64).unwrap_or(0) as usize;
    let (tour, duty) = (u("tour"), u("duty"));
    let piece = piece_label(&tours, tour, duty);
    match what {
        "pro_cut" => {
            let at = u("at");
            let time = tours.get(tour).and_then(|t| t.trips.get(at)).map(|x| rf::clock(x.dep)).unwrap_or_default();
            let label = tours.get(tour).map(block_label).unwrap_or_default();
            edit(s, r, trf("Vehicle %{v} cut at %{time}: the piece after it has no duty yet", &[("v", &label), ("time", &time)]), |c| pd::cut_to_pool(c, wd, &tours, tour, at))
        }
        "pro_pool" => edit(s, r, trf("Piece %{p} taken out of its duty", &[("p", &piece)]), |c| pd::to_pool(c, wd, &tours, (tour, duty))),
        "pro_dissolve" => {
            let number = row_of(&board, tour, duty).map(|row| board.shaped[row].label(row)).unwrap_or_default();
            edit(s, r, trf("Duty %{d} taken apart: its pieces have no duty", &[("d", &number)]), |c| pd::dissolve(c, wd, &tours, (tour, duty)))
        }
        "pro_new" => edit(s, r, trf("Piece %{p} made a duty of its own", &[("p", &piece)]), |c| pd::new_duty(c, wd, &tours, (tour, duty))),
        "pro_add" => {
            let into = (u("into_tour"), u("into_duty"));
            let number = row_of(&board, into.0, into.1).map(|row| board.shaped[row].label(row)).unwrap_or_default();
            edit(s, r, trf("Piece %{p} put in duty %{d}", &[("p", &piece), ("d", &number)]), |c| pd::add_to_duty(c, wd, &tours, (tour, duty), into))
        }
        "pro_auto" => {
            let (lines, date) = (k.lines.clone(), k.date.clone());
            let mut n = 0;
            let out = edit(s, r, tr("Duties made automatically"), |c| {
                let before = c.clone();
                n = pd::auto(c, lines.as_slice(), &date);
                rf::signing::keep_points(&before, c, &lines, &date);
                Ok(())
            });
            if out.get("edited").is_some() {
                // (round J: numbered in the order they sign on, 1001 the first - the edit kept the
                // numbers the duties had)
                let _ = company::act(s, |c| {
                    let mut v = rf::view(c);
                    pd::number_by_sign_on(&mut v, &lines, &date);
                    rf::keep_draft(c, &v);
                    Ok(())
                });
                said(trf("%{n} duties made from the pieces: SAVE keeps them.", &[("n", &n.to_string())]), false)
            } else {
                out
            }
        }
        // (round K: the "Afgange" tab - a duty built of departures, looked at first, and a duty's
        // own name)
        "pro_build_preview" => build_preview(k, &trips_arg(args), args.get("name").and_then(Value::as_str).unwrap_or("")),
        "pro_build" => {
            let trips = trips_arg(args);
            let name = args.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let words = if name.trim().is_empty() { trf("A duty built of %{n} departures", &[("n", &trips.len().to_string())]) } else { trf("Duty %{d} built of %{n} departures", &[("d", name.trim()), ("n", &trips.len().to_string())]) };
            edit(s, r, words, |c| pd::build_duty(c, wd, &tours, &trips, &name).map(|_| ()))
        }
        "pro_name" => {
            let name = args.get("name").and_then(Value::as_str).unwrap_or("").to_string();
            let number = row_of(&board, tour, duty).map(|row| board.shaped[row].number.map(|n| n.to_string()).unwrap_or_else(|| board.shaped[row].label(row))).unwrap_or_default();
            let words = if name.trim().is_empty() { trf("Duty %{d} shown by its number again", &[("d", &number)]) } else { trf("Duty %{d} named %{name}", &[("d", &number), ("name", name.trim())]) };
            edit(s, r, words, |c| pd::set_name(c, wd, &tours, (tour, duty), &name))
        }
        "pro_rules" => {
            // (the least times to change pieces, sign on and sign off: the company's preferences)
            let num = |name: &str, lo: i64, hi: i64| args.get(name).and_then(Value::as_i64).map(|v| v.clamp(lo, hi) as i32);
            let was = s.company.as_ref().map(|c| c.planning.flow.prefs.clone()).unwrap_or_default();
            let date = k.date.clone();
            match company::act(s, |c| {
                let p = &mut c.planning.flow.prefs;
                if let Some(v) = num("same_stop", 0, 60) {
                    p.change.same_stop = v;
                }
                if let Some(v) = num("other_stop", 0, 90) {
                    p.change.other_stop = v;
                }
                if let Some(v) = num("sign_on", 0, 60) {
                    p.sign_on = v;
                }
                if let Some(v) = num("sign_off", 0, 60) {
                    p.sign_off = v;
                }
                // (round J: the goals of "Make duties automatically", and the shortest piece)
                if let Some(v) = num("target_from", 0, 780) {
                    p.goals.target_from = v;
                    p.goals.target_to = p.goals.target_to.max(v);
                }
                if let Some(v) = num("target_to", 60, 780) {
                    p.goals.target_to = v;
                    p.goals.target_from = p.goals.target_from.min(v);
                    p.goals.longest = p.goals.longest.max(v);
                }
                if let Some(v) = num("longest", 240, 780) {
                    p.goals.longest = v;
                    p.goals.target_to = p.goals.target_to.min(v);
                    p.goals.target_from = p.goals.target_from.min(p.goals.target_to);
                }
                if let Some(v) = num("min_piece", 0, 300) {
                    p.min_piece.value = v;
                }
                if let Some(v) = args.get("same_line").and_then(Value::as_bool) {
                    p.goals.same_line = v;
                }
                Ok(())
            }) {
                Ok(()) => rules_applied(s, r, &date, &was, None),
                Err(_) => json!({}),
            }
        }
        _ => json!({}),
    }
}

/// (Round J) The day's figures' names (`summary_json`).
const FIGURES: [&str; 5] = ["Duties", "Paid hours", "Driving of the paid time", "Price of the day", "Average duty"];

/// Every English text this file hands to `tr`/`trf` besides its literals (the day types, the
/// figures' names).
#[cfg(test)]
pub(super) fn texts() -> Vec<&'static str> {
    let mut v = DAY_TYPES.to_vec();
    v.extend(FIGURES);
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::autopilot;
    use omsi_launcher_lib::company::model::Planner;

    #[test]
    fn the_day_in_figures_says_the_difference_from_the_games_plan() {
        super::super::company::words::set_language("en");
        let was = dp::Summary { duties: 44, paid: 337 * 60, driving: 190 * 60, average: 459, cost: 41_500_00, ok: 12, warn: 9, bad: 23 };
        let now = dp::Summary { duties: 41, paid: 338 * 60, driving: 200 * 60, average: 495, cost: 41_880_00, ok: 36, warn: 5, bad: 0 };
        let v = summary_json(&now, Some(&was));
        let f = |key: &str| v["figures"].as_array().unwrap().iter().find(|x| x["key"] == key).unwrap().clone();
        assert_eq!((f("duties")["value"].clone(), f("duties")["diff"].clone(), f("duties")["good"].clone()), (json!("41"), json!("−3"), json!(true)));
        assert_eq!((f("paid")["diff"].clone(), f("paid")["good"].clone()), (json!("+1 h"), json!(false)));
        assert_eq!(f("driving")["good"], json!(true), "more of the paid time driving is better");
        assert_eq!(f("cost")["good"], json!(false));
        assert!(f("average")["good"].is_null(), "the average is told, not judged");
        assert_eq!(v["status"]["diff_bad"], json!(-23));
        // the game's plan itself: no differences
        let w = summary_json(&was, None);
        assert!(w["figures"].as_array().unwrap().iter().all(|x| x["diff"] == "" && x["good"].is_null()));
    }

    #[test]
    fn the_pro_tabs_get_their_blocks_pieces_pool_and_duties() {
        super::super::company::words::set_language("en");
        let (mut c, line) = super::super::tests::line5_company();
        assert_eq!(autopilot::set_planner(&mut c, Planner::Pro), Planner::Pro);
        let lines = vec![line];
        let date = c.date.clone();
        let b = rf::board(&c, &lines, &date);
        let v = super::super::board_json(&c, &b);
        let p = &v["pro"];
        assert_eq!(p["on"], true);
        assert_eq!(p["base"], 1000);
        let blocks = p["blocks"].as_array().unwrap();
        assert_eq!(blocks.len(), b.tours.len());
        assert!(blocks.iter().all(|x| x["pieces"].as_array().is_some_and(|ps| !ps.is_empty() && ps[0]["letter"] == "A")));
        assert!(blocks.iter().any(|x| x["reliefs"].as_array().is_some_and(|r| !r.is_empty())), "a bus can be cut somewhere");
        assert!(p["pool"].as_array().unwrap().is_empty());
        let duties = p["duties"].as_array().unwrap();
        assert_eq!(duties.len(), b.shaped.len());
        assert_eq!(duties[0]["number"], "1001");
        assert!(duties.iter().all(|d| ["whole", "split", "short"].contains(&d["kind"].as_str().unwrap_or("")) && d["detail"].as_str().is_some_and(|t| t.starts_with("Signs on"))));
        // (round J) each duty its rules as a chip, its checklist, its break; in the order they sign on
        assert!(duties.iter().all(|d| d["chip"].as_str().is_some_and(|c| !c.is_empty()) && d["checks"].as_array().is_some_and(|k| k.len() >= 5) && d["pause"].is_string()));
        let starts: Vec<i64> = duties.iter().map(|d| d["start"].as_i64().unwrap()).collect();
        assert!(starts.windows(2).all(|w| w[0] <= w[1]));
        assert!(blocks.iter().all(|b| !b["label"].as_str().unwrap().contains('/')), "short names: 5 · 01");
        // with its date and timetable: the prices, the figures, the game's plan
        let mut full = v.clone();
        add_day(&mut full, &c, &b, &lines, &date);
        let fp = &full["pro"];
        assert!(fp["duties"].as_array().unwrap().iter().all(|d| d["price_cents"].as_i64().is_some_and(|x| x > 0) && d["price_parts"].as_array().is_some_and(|p| p.len() >= 2)));
        assert_eq!(fp["summary"]["figures"].as_array().map(Vec::len), Some(5));
        assert_eq!(fp["plan"]["duties"].as_array().map(Vec::len), Some(duties.len()), "nothing built: the game's plan is the day");
        assert!(fp["plan"]["line"].as_str().is_some_and(|t| t.starts_with("The game's plan")));
        // a bus cut where it can be: the piece after it in the pool, numbered from 1501 to run
        let i = blocks.iter().position(|x| !x["reliefs"].as_array().unwrap().is_empty()).unwrap();
        let at = blocks[i]["reliefs"][0]["at"].as_u64().unwrap() as usize;
        let wd = b.weekday;
        pd::cut_to_pool(&mut c, wd, &b.tours, i, at).unwrap();
        let b = rf::board(&c, &lines, &date);
        let p = super::super::board_json(&c, &b)["pro"].clone();
        assert_eq!(p["pool"].as_array().map(Vec::len), Some(1));
        assert_eq!(p["pool"][0]["runs_as"], "1501");
        assert!(p["blocks"][i]["pieces"].as_array().unwrap().iter().any(|x| x["pool"] == true && x["number"].is_null()));
        // (round K) every trip of the day for the Afgange tab, in order of departure, each with its
        // block, line and duty
        let deps = p["departures"].as_array().unwrap();
        assert_eq!(deps.len(), b.tours.iter().map(|t| t.trips.len()).sum::<usize>());
        assert!(deps.windows(2).all(|w| w[0]["dep"].as_i64() <= w[1]["dep"].as_i64()));
        assert!(deps.iter().all(|d| d["block"].as_str().is_some_and(|x| !x.is_empty()) && d["line"].is_string()));
        assert!(deps.iter().any(|d| d["duty"].is_null() && d["pool"] == true), "the piece in the pool has no duty");
        // a duty built of departures, looked at first: its times and checklist; two at once refused
        let i = deps.iter().find(|d| d["counts"] == true && d["pool"] == false).unwrap()["i"].as_u64().unwrap() as usize;
        let js: Vec<usize> = deps.iter().filter(|d| d["i"] == i && d["counts"] == true).map(|d| d["j"].as_u64().unwrap() as usize).take(2).collect();
        let trips: Vec<(usize, usize)> = js.iter().map(|&j| (i, j)).collect();
        let k = super::super::Cached { date: date.clone(), generation: 0, board: std::sync::Arc::new(b.clone()), view: std::sync::Arc::new(rf::view(&c)), lines: std::sync::Arc::new(lines.clone()), json: std::sync::Arc::new(json!({})), rev: String::new() };
        let pv = build_preview(&k, &trips, "Tidlig");
        assert!(pv["duty"]["checks"].as_array().is_some_and(|x| !x.is_empty()), "{pv}");
        assert_eq!(pv["duty"]["number"], "Tidlig");
        assert!(pv["duty"]["price_cents"].as_i64().is_some_and(|x| x > 0));
        let other = deps.iter().find(|d| d["i"].as_u64() != Some(i as u64) && d["counts"] == true && d["dep"].as_i64() == Some(b.tours[i].trips[js[0]].dep as i64));
        if let Some(o) = other {
            let two = vec![(i, js[0]), (o["i"].as_u64().unwrap() as usize, o["j"].as_u64().unwrap() as usize)];
            assert!(build_preview(&k, &two, "")["error"].is_string());
        }
        // without RosterFlow Pro: no Pro part
        autopilot::set_planner(&mut c, Planner::RosterFlow);
        let b = rf::board(&c, &lines, &date);
        assert!(super::super::board_json(&c, &b).get("pro").is_none());
    }
}

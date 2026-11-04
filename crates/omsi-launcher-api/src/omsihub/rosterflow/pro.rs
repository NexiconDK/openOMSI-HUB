//! RosterFlow Pro's tabs "1 Vognløb" and "2 Vagter" (step 2, NexiconDK 4 November 2026; the core's
//! `company::pro_duties`): the day's blocks with their relief points, the pieces they are cut into
//! and the pool of pieces without a duty, and the duties - their numbers, types, times, pieces and
//! the marks of the rules they break - as the page (`launcher/ui/js/omsihub/rosterflow/pro.js`)
//! draws them; and the acts that build them. Every edit is RosterFlow's own (`edit`): in the
//! draft, Undo and Save as every other.

use super::{block_label, colour_of, company, edit, issue_words, rules_applied, said, Cached, Rf, Session};
use super::super::company::words::{tr, trf};
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

/// A piece in words: its block and letter ("1/02 C").
fn piece_label(tours: &[TourOfDay], i: usize, k: usize) -> String {
    tours.get(i).map(|t| format!("{} {}", block_label(t), letter(k))).unwrap_or_default()
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
    let duties: Vec<Value> = board.shaped.iter().enumerate().filter(|(row, s)| !pool_rows[*row] && s.spare.is_none()).map(|(row, s)| duty_json(c, board, row, s)).collect();
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
    json!({
        "on": true,
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
                "tour": sp.tour, "duty": sp.duty, "label": piece_label(tours, sp.tour, sp.duty), "number": line_number(t), "colour": colour_of(c, &t.line),
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
    json!({
        "row": row, "number": s.label(row), "kind": kind.key(), "kind_label": tr(kind.label()),
        "start": on_at, "end": off_at, "work": s.work, "paid": s.paid, "spread": s.spread(),
        "pieces": pieces, "between": between, "line_change": line_change,
        "level": level, "tip": tips.join("\n"), "detail": words.join("  ·  "),
        "locked": s.locked,
    })
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
                n = pd::auto(c, &lines, &date);
                rf::signing::keep_points(&before, c, &lines, &date);
                Ok(())
            });
            if out.get("edited").is_some() {
                said(trf("%{n} duties made from the pieces: SAVE keeps them.", &[("n", &n.to_string())]), false)
            } else {
                out
            }
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
                Ok(())
            }) {
                Ok(()) => rules_applied(s, r, &date, &was, None),
                Err(_) => json!({}),
            }
        }
        _ => json!({}),
    }
}

/// Every English text this file hands to `tr`/`trf` besides its literals (the day types).
#[cfg(test)]
pub(super) fn texts() -> Vec<&'static str> {
    DAY_TYPES.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::autopilot;
    use omsi_launcher_lib::company::model::Planner;

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
        // without RosterFlow Pro: no Pro part
        autopilot::set_planner(&mut c, Planner::RosterFlow);
        let b = rf::board(&c, &lines, &date);
        assert!(super::super::board_json(&c, &b).get("pro").is_none());
    }
}

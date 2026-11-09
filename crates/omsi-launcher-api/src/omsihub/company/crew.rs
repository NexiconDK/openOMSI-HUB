//! The driver's side of the company for a player of "Play together" (`co_crew`, relayed by his
//! launcher: `together::relay`): his duty swaps with the other players, his sick call and his days
//! off, a handover note and a defect of the bus he drives - each on his own duty and his own bus
//! only, the rules the company's (`co::crew`). The boss and a single player have the same on the
//! phone (`companion/company.rs`, the company's queue).

use super::words::{tr, trf};
use super::{plan_of, week_work, Session};
use crate::Answer;
use omsi_launcher_lib::company::crew::{self, DefectKind, NoteTemplate, SwapState};
use omsi_launcher_lib::company::duty_launch::DutyRef;
use omsi_launcher_lib::company::holidays::Reason;
use omsi_launcher_lib::company::plan::{BusOf, DayPlan, Who};
use omsi_launcher_lib::company::pro_ops::{NoWhy, Reply};
use omsi_launcher_lib::company::player_wish::{self as pw, Kind, Miss, PlayerWish};
use omsi_launcher_lib::company::together::book::ClaimState;
use omsi_launcher_lib::company::together::Perm;
use omsi_launcher_lib::company as co;
use serde_json::{json, Value};

/// Why a player's crew action is refused (English: keys of the translations).
pub const NOT_YOUR_BUS: &str = "That is not the bus of your duty.";
pub const NOT_READY: &str = "The timetable of the day is still being read.";

/// The company's day plan (None: still being read).
fn today(s: &mut Session) -> Option<DayPlan> {
    let date = s.company.as_ref()?.date.clone();
    week_work(s);
    plan_of(s, &date)?.ok()
}

/// The buses of player `guest`'s duties of the day: those he may write a note or a defect for.
fn buses_of(s: &mut Session, guest: u32) -> Vec<u32> {
    let Some(p) = today(s) else { return Vec::new() };
    let Some(c) = s.company.as_ref() else { return Vec::new() };
    c.together
        .claims
        .iter()
        .filter(|x| x.guest == guest && x.duty.date == p.date && x.state != ClaimState::Asked)
        .filter_map(|x| x.duty.find(&p).and_then(|(ti, _)| p.tours[ti].bus))
        .filter_map(|b| if let BusOf::Own(id) = b { Some(id) } else { None })
        .collect()
}

fn duty_of(args: &Value) -> Option<DutyRef> {
    serde_json::from_value(args.get("duty").cloned()?).ok()
}

fn s_arg<'a>(args: &'a Value, k: &str) -> &'a str {
    args.get(k).and_then(Value::as_str).unwrap_or("")
}

/// The weekdays' names (English: keys of the translations), Monday first.
pub const WEEKDAY_NAMES: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/// (round R) A wish a duty does not keep, in words ("avoids night duties").
pub fn miss_words(m: &Miss) -> String {
    let (t, args) = m.words(&|x| tr(x), &|d| tr(WEEKDAY_NAMES[(d as usize).min(6)]));
    let args: Vec<(&str, &str)> = args.iter().map(|(k, v)| (*k, v.as_str())).collect();
    trf(t, &args)
}

/// (round R) "Wish not kept: avoids night duties, wished Wednesday off".
pub fn misses_words(ms: &[Miss]) -> String {
    trf("Wish not kept: %{what}", &[("what", &ms.iter().map(miss_words).collect::<Vec<_>>().join(", "))])
}

/// (round R) Player `guest`'s wishes for the form.
fn wishes_json(c: &co::Company, guest: u32) -> Value {
    let w = pw::of(c, guest).cloned().unwrap_or_default();
    json!({
        "prefer": w.prefer.iter().map(|k| k.key()).collect::<Vec<_>>(),
        "avoid": w.avoid.iter().map(|k| k.key()).collect::<Vec<_>>(),
        "off": w.off, "dates": w.dates, "max_hours": w.max_week.map(|m| m / 60),
        "kinds": Kind::ALL.iter().map(|k| json!([k.key(), tr(k.label())])).collect::<Vec<_>>(),
        "weekdays": (0..7).map(|d| json!([d, tr(super::words::WEEKDAYS[d])])).collect::<Vec<_>>(),
    })
}

/// What player `guest` sees of his side: the swaps asked of him and his own, the players he may
/// ask, the buses of his duties with their notes, the forms' choices.
pub fn view(s: &mut Session, guest: u32) -> Value {
    let buses = buses_of(s, guest);
    let Some(c) = s.company.as_ref() else { return json!({ "none": true }) };
    let me = Some(Who::Guest(guest));
    let swaps: Vec<Value> = c
        .crew
        .swaps
        .iter()
        .filter(|x| x.from == me || x.with == me)
        .map(|x| json!({ "no": x.no, "mine": x.mine, "theirs": x.theirs, "state": x.state, "why": tr(&x.why), "asked_of_me": x.with == me && x.state == SwapState::Asked, "from": x.from.map(|w| if let Who::Guest(g) = w { c.together.name_of(g) } else { String::new() }) }))
        .collect();
    // (round R: each colleague's duties of the day not begun - the one to drive instead)
    let clock_of = |m: i32| format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(60));
    let today = c.date.clone();
    let players: Vec<Value> = c
        .together
        .guests
        .iter()
        .filter(|g| g.no != guest && !g.banned)
        .map(|g| {
            let duties: Vec<Value> = c
                .together
                .claims
                .iter()
                .filter(|x| x.guest == g.no && x.duty.date == today && x.state == ClaimState::Planned && x.part.is_none() && x.from > c.clock.minute as i32)
                .map(|x| json!({ "duty": x.duty, "label": format!("{} · {}–{}", x.number, clock_of(x.from), clock_of(x.to)) }))
                .collect();
            json!({ "no": g.no, "name": g.name, "duties": duties })
        })
        .collect();
    // (round R: the swaps waiting for an approval - for whoever gives or approves duties)
    let approves = c.together.guest(guest).is_some_and(|g| g.can(Perm::Approve) || g.can(Perm::Assign));
    let name_of = |w: Option<Who>| match w {
        Some(Who::Guest(g)) => c.together.name_of(g),
        _ => String::new(),
    };
    let pending: Vec<Value> = if approves {
        crew::waiting(c)
            .map(|x| json!({ "no": x.no, "text": trf("%{a} and %{b} want to swap: %{what}", &[("a", &name_of(x.from)), ("b", &name_of(x.with)), ("what", &format!("{} {} ⇄ {}", x.mine.line, x.mine.tour, x.theirs.as_ref().map(|t| format!("{} {}", t.line, t.tour)).unwrap_or_else(|| tr("nothing in return"))))]) }))
            .collect()
    } else {
        Vec::new()
    };
    // (NexiconDK, 4 November 2026: RosterFlow Pro's daily operations ask him for a duty -
    // `pro_ops::ask`; he answers for himself)
    let clock = |m: i32| format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(60));
    let asks: Vec<Value> = co::pro_ops::asked_of(c, guest)
        .iter()
        .map(|a| json!({ "id": a.id, "n": a.n, "day": super::words::day_label(&a.date), "time": format!("{}–{}", clock(a.start), clock(a.end)), "start": a.start, "end": a.end }))
        .collect();
    let buses: Vec<Value> = buses
        .iter()
        .filter_map(|id| c.vehicle(*id))
        .map(|v| json!({ "id": v.id, "number": v.number, "notes": crew::notes_for(c, v.id).iter().map(|n| super::launch::note_words(n)).collect::<Vec<_>>() }))
        .collect();
    json!({
        "swaps": swaps, "players": players, "buses": buses, "asks": asks, "pending": pending, "wishes": wishes_json(c, guest),
        "templates": NoteTemplate::ALL.iter().map(|t| json!([t.key(), tr(t.text())])).collect::<Vec<_>>(),
        "checks": DefectKind::ALL.iter().map(|k| json!([k.key(), tr(k.label())])).collect::<Vec<_>>(),
        "reasons": Reason::ALL.iter().map(|r| json!([r, tr(r.label())])).collect::<Vec<_>>(),
        "words": {
            "caps": tr("My duties").to_uppercase(), "swaps": tr("Swap requests"), "asked_of_me": tr("%{name} asks you to swap duty %{mine}"),
            "accept": tr("Accept"), "refuse": tr("Refuse"), "swap": tr("Swap"), "with": tr("Ask to swap with"), "ask": tr("Ask"),
            "asked": tr("asked"), "accepted": tr("accepted"), "refused": tr("refused"),
            // (round R: the boss's approval, a duty of theirs named, the wishes)
            "waiting": tr("waits for the boss"), "pending_caps": tr("Swaps waiting for your approval"), "approve": tr("Approve"),
            "theirs": tr("Their duty"), "nothing": tr("Nothing in return: they take mine"),
            "wishes_caps": tr("My wishes"), "wishes_say": tr("Wishes are not rules: the boss sees them where the duties are given, and gives those that suit first."),
            "prefer": tr("I would like"), "avoid": tr("I would rather not"), "off": tr("Days off I wish"), "dates": tr("Dates off I wish"), "add_date": tr("Add"),
            "max": tr("Most hours a week"), "max_ph": tr("as many as come"), "save": tr("Save my wishes"),
            "sick": tr("Call in sick"), "sick_confirm": tr("Your duties of those days go to the dispatcher. Call in sick?"),
            "bus": tr("Bus %{bus} today"), "notes": tr("Handover notes"), "no_notes": tr("No notes for today."), "note": tr("Leave a note"), "note_ph": tr("Or type a note"), "none": tr("None"),
            "defect": tr("Report a defect"), "defect_ph": tr("What is wrong (optional)"), "report": tr("Report"), "no_bus": tr("No duty with a bus today."),
            "sent": tr("Sent: the boss's launcher carries it out."),
            "asked_caps": tr("The control room asks"), "asked": tr("Can you drive duty %{n} on %{day}, %{time}?"),
            "yes": tr("Yes"), "no": tr("No"), "early": tr("Too early"), "late": tr("Too late"), "from": tr("I can from"), "until": tr("I can until"),
            "own": tr("Or say it in your own words"), "send": tr("Send"),
            // (NexiconDK, Bus Company Online: a delay reported to the control room - its late
            // departures not fined, `server_fines::report_late`)
            "late": c.online.on_server.then(|| tr("Report a delay")),
            "late_tip": c.online.on_server.then(|| tr("Tell the control room you run late: from now on your departures more than 15 minutes late are not fined.")),
        },
    })
}

/// A crew action of player `guest` (`args.act`): carried out on the company by its rules, on his
/// own duty and bus only. Its answer, or the refusal's words.
pub fn act(s: &mut Session, guest: u32, args: &Value) -> Answer {
    let me = Who::Guest(guest);
    let by = s.company.as_ref().map(|c| c.together.name_of(guest)).unwrap_or_default();
    let refused = |e: Value| e.get("text").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| tr("This cannot be done"));
    match s_arg(args, "act") {
        "view" => Ok(view(s, guest)),
        "note" | "defect" => {
            let act = s_arg(args, "act");
            let bus = args.get("bus").and_then(Value::as_u64).unwrap_or(0) as u32;
            if !buses_of(s, guest).contains(&bus) {
                return Err(tr(NOT_YOUR_BUS));
            }
            let text = s_arg(args, "text").to_string();
            let r = if act == "note" {
                let template = NoteTemplate::from_key(s_arg(args, "template"));
                super::act(s, |c| crew::add_note(c, bus, &by, template, &text))
            } else {
                let kind = DefectKind::from_key(s_arg(args, "kind")).unwrap_or(DefectKind::Other);
                super::act(s, |c| crew::report_defect(c, bus, kind, &text, &by, false))
            };
            r.map(|id| json!({ "ok": true, "id": id })).map_err(refused)
        }
        "swap" => {
            let (Some(mine), Some(with)) = (duty_of(args), args.get("with").and_then(Value::as_u64)) else { return Err(tr(crew::NO_DUTY)) };
            // (round R: the colleague's duty to drive instead, when named)
            let theirs: Option<DutyRef> = args.get("theirs").cloned().and_then(|v| serde_json::from_value(v).ok());
            let give = args.get("give").and_then(Value::as_bool).unwrap_or(false);
            let date = mine.date.clone();
            week_work(s);
            let Some(Ok(p)) = plan_of(s, &date) else { return Err(tr(NOT_READY)) };
            super::act(s, |c| crew::ask_swap_for(c, &p, me, &mine, Who::Guest(with as u32), theirs.clone(), give)).map(|x| json!({ "ok": true, "state": x.state, "why": tr(&x.why) })).map_err(refused)
        }
        // (round R: the boss approves a swap two players agreed on, or not)
        "swap_decide" => {
            let no = args.get("no").and_then(Value::as_u64).unwrap_or(0) as u32;
            let yes = args.get("yes").and_then(Value::as_bool).unwrap_or(false);
            let date = s.company.as_ref().and_then(|c| c.crew.swaps.iter().find(|x| x.no == no).map(|x| x.mine.date.clone())).unwrap_or_default();
            week_work(s);
            let Some(Ok(p)) = plan_of(s, &date) else { return Err(tr(crew::NOT_WAITING)) };
            super::act(s, |c| crew::decide_swap(c, &p, no, yes)).map(|x| json!({ "ok": true, "state": x })).map_err(refused)
        }
        // (round R: his wishes for his duties)
        "wishes" => {
            let kinds = |k: &str| -> Vec<Kind> { args.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).filter_map(Kind::from_key).collect()).unwrap_or_default() };
            let w = PlayerWish {
                prefer: kinds("prefer"),
                avoid: kinds("avoid"),
                off: args.get("off").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(|d| d as u8).collect()).unwrap_or_default(),
                dates: args.get("dates").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default(),
                max_week: args.get("max_hours").and_then(Value::as_f64).filter(|h| *h > 0.0).map(|h| (h * 60.0).round() as i32),
                ..Default::default()
            };
            super::act(s, |c| pw::set(c, guest, w.clone())).map(|_| json!({ "ok": true, "said": tr("Your wishes are saved.") })).map_err(refused)
        }
        "swap_answer" => {
            let no = args.get("no").and_then(Value::as_u64).unwrap_or(0) as u32;
            let yes = args.get("yes").and_then(Value::as_bool).unwrap_or(false);
            let date = s.company.as_ref().and_then(|c| c.crew.swaps.iter().find(|x| x.no == no).map(|x| x.mine.date.clone())).unwrap_or_default();
            week_work(s);
            let Some(Ok(p)) = plan_of(s, &date) else { return Err(tr(crew::NO_SWAP)) };
            super::act(s, |c| crew::answer_swap(c, &p, no, me, yes)).map(|x| json!({ "ok": true, "state": x })).map_err(refused)
        }
        // (NexiconDK, 4 November 2026: his answer to RosterFlow Pro's daily operations, asked for
        // a duty - `pro_ops::answer`: yes, no, too early from a time, too late until one, his own
        // words)
        "pro_answer" => {
            let id = args.get("id").and_then(Value::as_u64).unwrap_or(0) as u32;
            let time = |k: &str| -> i32 {
                let t = s_arg(args, k);
                let mut it = t.split(':').map(|x| x.trim().parse::<i32>().unwrap_or(0));
                let (h, m) = (it.next().unwrap_or(0), it.next().unwrap_or(0));
                (h.clamp(0, 24) * 60 + m.clamp(0, 59)).min(1440)
            };
            let text = s_arg(args, "text").trim().to_string();
            let reply = match s_arg(args, "kind") {
                "yes" => Reply::Yes { say: 0 },
                "no" => Reply::No { why: NoWhy::Plans, say: 0, own: 0 },
                "early" => Reply::Early { from: time("time"), say: 0 },
                "late" => Reply::Late { until: time("time"), say: 0 },
                _ if !text.is_empty() => Reply::Text { text, yes: args.get("yes").and_then(Value::as_bool).unwrap_or(false) },
                _ => return Err(tr("Say yes, no, or in your own words.")),
            };
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
            super::act(s, |c| co::pro_ops::answer(c, id, guest, reply, now)).map(|_| json!({ "ok": true })).map_err(refused)
        }
        // (NexiconDK: his delay reported to the control room - Bus Company Online's server)
        "late" => super::act(s, |c| omsi_launcher_lib::company::server_fines::report_late(c, guest)).map(|_| json!({ "ok": true, "said": tr(omsi_launcher_lib::company::server_fines::REPORTED) })).map_err(refused),
        "sick" => {
            let days = args.get("days").and_then(Value::as_u64).unwrap_or(1) as u32;
            super::act(s, |c| crew::call_in_sick(c, me, days)).map(|until| json!({ "ok": true, "until": until })).map_err(refused)
        }
        "leave" => {
            let reason = args.get("reason").cloned().and_then(|r| serde_json::from_value::<Reason>(r).ok()).unwrap_or_default();
            let (from, until) = (s_arg(args, "from").to_string(), s_arg(args, "until").to_string());
            super::act(s, |c| crew::ask_leave(c, me, &from, &until, reason)).map(|(no, yes)| json!({ "ok": true, "no": no, "approved": yes })).map_err(refused)
        }
        other => Err(format!("no crew action {other}")),
    }
}

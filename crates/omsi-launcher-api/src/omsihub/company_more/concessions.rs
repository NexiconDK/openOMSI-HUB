//! The concession market (`crates/omsi-launcher-ui/src/company/concessions.rs`): the map's
//! lines out to tender with their week and the contract each lets, the concessions the company
//! holds with their bus contracts (kind, payment, rating), and the auction of the tender picked
//! as it runs with the clock - the authority's cap, the advice price, the rivals and their bids,
//! the least bid that leads now and its chance, buying the line outright - and how it ended,
//! every bid with its score. The rules are `company::concessions`' and `company::bus_contracts`'.
//!
//! The lines' weeks are read from the timetable of the company's week on a thread of their own
//! (the page says so meanwhile). The Contracts page's "Tender bids" (`bids`) lists the tenders
//! the company bid on.

use super::super::words::{day_label, eur, eur_cents, grouped, num, span_text, tr, trf};
use super::super::{act as apply, busy, may_step, simulate, Session};
use super::contracts as kc;
use super::{more, more_state, u32_arg};
use crate::Answer;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::auction::{Placed, Who};
use omsi_launcher_lib::company::bus_contracts::{self as bc, Period};
use omsi_launcher_lib::company::clock::{self as ck, Step};
use omsi_launcher_lib::company::concessions::{self as cn, Outcome, Tender, Week};
use omsi_launcher_lib::company::viborg::{self, Award};
use omsi_launcher_lib::company::shared_tenders as co_tenders;
use omsi_launcher_lib::company::{dates, Cents, Company, Drive};
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Default)]
pub struct TendersState {
    /// The lines' weeks, for the map, the first day and the timetable's revision they were read
    /// for; a read on its way.
    weeks: HashMap<String, Week>,
    weeks_for: Option<(String, String, u64)>,
    reading: bool,
    /// The tender in the auction panel.
    pub selected: Option<u32>,
}

/// The Monday of a date's week (the week is read from it).
fn monday_of(date: &str) -> String {
    let d = dates::parse(date).unwrap_or(0);
    dates::fmt(d - dates::weekday(d) as i64)
}

/// The weeks of every map line, read in the background once per map and week - and again when
/// the line editor wrote the map's timetable (`concessions::weeks`).
fn weeks(s: &Session) {
    let Some(c) = s.company.as_ref() else { return };
    let monday = monday_of(&c.date);
    let key = (c.map.clone(), monday.clone(), s.timetable);
    let mut m = more(s);
    if m.tenders.weeks_for.as_ref() == Some(&key) {
        return;
    }
    m.tenders.weeks_for = Some(key.clone());
    m.tenders.reading = true;
    let map = c.map.clone();
    let _ = std::thread::Builder::new().name("company weeks".into()).spawn(move || {
        // (Monday to Sunday in their order: a day not read is a day without its lines)
        let days: Vec<Vec<core::LineInfo>> = (0..7).map(|k| core::list_lines(&map, &dates::add(&monday, k)).unwrap_or_default()).collect();
        let mut out = HashMap::new();
        for l in days.iter().find(|d| !d.is_empty()).map(|d| d.iter().map(|l| l.name.clone()).collect::<Vec<_>>()).unwrap_or_default() {
            out.insert(l.to_lowercase(), cn::week_of(&days, &l));
        }
        let mut m = more_state().lock().unwrap_or_else(|e| e.into_inner());
        // (read for the week still wanted: else a newer read is on its way)
        if m.tenders.weeks_for.as_ref() == Some(&key) {
            m.tenders.weeks = out;
            m.tenders.reading = false;
        }
        drop(m);
        let mut s = super::super::session().lock().unwrap_or_else(|e| e.into_inner());
        s.generation += 1;
    });
}

/// When a minute of the clock is: its time, and its day when it is not today.
fn when(c: &Company, m: i64) -> String {
    let d = ck::date_of(m);
    if d == c.date {
        ck::hhmm(m)
    } else {
        format!("{} {}", day_label(&d), ck::hhmm(m))
    }
}

/// "2:10 h".
fn span(minutes: i64) -> String {
    let m = minutes.max(0);
    format!("{}:{:02} h", m / 60, m % 60)
}

/// (NexiconDK, decided 25 October 2026) An ask of a tender in words: on the server by the
/// timetable hour ("862,37 kr an hour"), elsewhere a year's.
pub fn money(t: &Tender, amount: Cents) -> String {
    if t.market.is_some() {
        trf("%{amount} an hour", &[("amount", &eur_cents(cn::per_hour(t, amount)))])
    } else {
        eur(amount)
    }
}

/// (NexiconDK, 3 November 2026) An ask of a tender of the server's in its two parts - the
/// timetable hour's price and a contract bus a day - when it has a bus part (`money` without).
pub fn money_parts(t: &Tender, amount: Cents, bus_day: Cents) -> String {
    if t.market.is_some() && bus_day > 0 {
        trf("%{hour} an hour + %{bus} a bus a day", &[("hour", &eur_cents(cn::hour_part(t, amount, bus_day))), ("bus", &eur(bus_day))])
    } else {
        money(t, amount)
    }
}

/// The company's own bid on a tender in words and its tone (`concessions::my_bid_text`).
pub fn my_bid_text(c: &Company, t: &Tender) -> Option<(String, &'static str)> {
    let b = cn::my_bid(c, t, ck::now(c))?;
    let bid = trf("Your bid: %{amount} at %{when}", &[("amount", &money_parts(t, b.amount, t.bus_day)), ("when", &when(c, b.at))]);
    let bid = if b.count > 1 { format!("{bid} ({})", trf("%{n} bids placed", &[("n", &b.count.to_string())])) } else { bid };
    // (NexiconDK, 31 October 2026: the map's tenders take sealed bids - nobody leads till they
    // are opened at the close)
    if t.market.is_some() && b.result.is_none() {
        return Some((format!("{bid}  ·  {}", tr("sealed: opened at the close")), "ok"));
    }
    Some(match &b.result {
        None if b.leads => (format!("{bid}  ·  {}", tr("leading")), "ok"),
        None => (format!("{bid}  ·  {}", tr("beaten: bid again before it closes")), "warn"),
        Some((winner, _)) if winner.is_empty() => (format!("{bid}  ·  {}", tr("you won")), "ok"),
        Some((winner, amount)) => (format!("{bid}  ·  {}", trf("won by %{who} with %{amount}", &[("who", winner), ("amount", &money(t, *amount))])), "warn"),
    })
}

/// A tender's state in a few words, and its tone (`concessions::state_of`).
pub fn state_of(c: &Company, t: &Tender) -> (String, &'static str) {
    let now = ck::now(c);
    match &t.outcome {
        None if now < t.opens_at => (trf("Opens %{when}", &[("when", &when(c, t.opens_at))]), "soft"),
        // (NexiconDK, 31 October 2026: sealed - nothing of the others' till the close)
        None if t.market.is_some() => {
            let mine = if t.offers.is_empty() { tr("no bid of yours yet") } else { tr("your sealed bid is in") };
            (format!("{}  ·  {mine}", trf("Sealed bids until %{when}", &[("when", &when(c, t.closes_at))])), if t.offers.is_empty() { "accent2" } else { "ok" })
        }
        None => {
            let placed = cn::bids(c, t, now);
            let lead = match placed.last() {
                Some(p) if p.who == Who::Player => trf("you lead with %{amount}", &[("amount", &money(t, p.amount))]),
                Some(p) => trf("%{who} leads with %{amount}", &[("who", &cn::bidder_name(c, t, p.who)), ("amount", &money(t, p.amount))]),
                None => tr("no bid yet"),
            };
            let mine = !t.offers.is_empty();
            let tone = match placed.last() {
                Some(p) if p.who == Who::Player => "ok",
                _ if mine => "warn",
                _ => "accent2",
            };
            (format!("{}  ·  {}", trf("Running until %{time}", &[("time", &ck::hhmm(t.closes_at))]), lead), tone)
        }
        // (NexiconDK: on the server by the timetable hour)
        Some(Outcome::Won { amount, .. }) if t.market.is_some() => (trf("Won with an ask of %{amount}", &[("amount", &money(t, *amount))]), "ok"),
        Some(Outcome::Lost { winner, amount, .. }) if t.market.is_some() => (trf("Lost to %{who}, who asked %{amount}", &[("who", winner), ("amount", &money(t, *amount))]), "warn"),
        Some(Outcome::Won { amount, bought, .. }) => (trf(if *bought { "Bought for %{amount}" } else if t.scored() { "Won with an ask of %{amount} a year" } else { "Won for %{amount}" }, &[("amount", &eur(*amount))]), "ok"),
        Some(Outcome::Lost { winner, amount, .. }) => (trf(if t.scored() { "Lost to %{who}, who asked %{amount} a year" } else { "Lost to %{who} for %{amount}" }, &[("who", winner), ("amount", &eur(*amount))]), "warn"),
        Some(Outcome::NoBid { winner, amount }) if !winner.is_empty() => (trf("Went to %{who} for %{amount}", &[("who", winner), ("amount", &money(t, *amount))]), "faint"),
        Some(Outcome::NoBid { .. }) => (tr("Nobody bid"), "faint"),
    }
}

fn caption_of(t: &Tender) -> String {
    let caption = if t.caption.is_empty() { t.line.clone() } else { t.caption.clone() };
    // (NexiconDK, 1 November 2026: a special service names its event)
    match event_of(t) {
        Some((event, _)) => format!("{caption}  ·  {}", trf("Special service: %{event}", &[("event", &event)])),
        None => caption,
    }
}

/// (NexiconDK, 1 November 2026) A special service's event and its days in words (a tender of a
/// chrono event's own line on the server, `shared_tenders::MapEvent`); None: an ordinary tender.
fn event_of(t: &Tender) -> Option<(String, String)> {
    let m = t.market.as_ref().filter(|m| !m.event.is_empty())?;
    let days = if m.from == m.until || m.until.is_empty() { day_label(&m.from) } else { format!("{} – {}", day_label(&m.from), day_label(&m.until)) };
    Some((m.event.clone(), days))
}

/// The tenders the company bid on, the latest first (the Contracts page's "Tender bids",
/// `concessions::bids_list`).
pub fn bids(c: &Company) -> Value {
    let mut list: Vec<&Tender> = c.concessions.tenders.iter().filter(|t| !t.offers.is_empty()).collect();
    list.sort_by(|a, b| b.open().cmp(&a.open()).then(b.closes_at.cmp(&a.closes_at)));
    let rows: Vec<Value> = list
        .iter()
        .map(|t| {
            let (state, tone) = state_of(c, t);
            let mine = my_bid_text(c, t);
            json!({ "id": t.id, "number": t.number, "caption": caption_of(t), "bid": mine.as_ref().map(|m| &m.0), "bid_tone": mine.map(|m| m.1), "state": state, "tone": tone })
        })
        .collect();
    json!({
        "title": tr("Your tender bids"), "rows": rows,
        "empty": tr("The company has bid on no tender yet. The Concessions page lists the lines out to tender: bid there, and your bids show here with their deadline and result."),
    })
}

pub fn view(s: &mut Session, args: &Value) -> Value {
    weeks(s);
    let Some(c) = s.company.clone() else { return Value::Null };
    let mut list: Vec<Tender> = c.concessions.tenders.clone();
    list.sort_by(|a, b| b.open().cmp(&a.open()).then(if a.open() { a.opens_at.cmp(&b.opens_at) } else { b.closes_at.cmp(&a.closes_at) }));
    // (the panel shows the one picked, else the first running or announced)
    let (weeks, reading, selected) = {
        let mut m = more(s);
        if m.tenders.selected.is_none_or(|id| !list.iter().any(|t| t.id == id)) {
            m.tenders.selected = list.first().map(|t| t.id);
        }
        (m.tenders.weeks.clone(), m.tenders.reading, m.tenders.selected)
    };
    let today_read = s.today.is_some();
    json!({
        "tenders": tenders(&c, &list, &weeks, reading, today_read, selected),
        "held": held(&c),
        "auction": auction(&c, selected, args),
        "offers": offers(&c),
        "doubling": doubling_orders(&c),
        "changes": changes(&c),
        "subs": subs(&c),
        "results": results(&c),
    })
}

/// (NexiconDK, 3 November 2026) The company's subcontracts (`subcontracts`, H10): given or run,
/// proposed to it (accept, decline), running (end with notice); and, holding contracts, a
/// proposal to another company of the map. Not on a server: none.
fn subs(c: &Company) -> Value {
    use omsi_launcher_lib::company::subcontracts::{State, NOTICE_DAYS};
    if !c.online.on_server {
        return Value::Null;
    }
    let others: Vec<Value> = omsi_launcher_lib::company::store::list(&super::super::data(), "")
        .into_iter()
        .filter(|o| o.online.on_server && o.id != c.id && co_tenders::key_of(&o.map) == co_tenders::key_of(&c.map))
        .map(|o| json!({ "id": o.id, "name": o.name }))
        .collect();
    let held: Vec<Value> = c.concessions.held.iter().filter(|h| dates::between(&c.date, &h.until) > 0).map(|h| json!({ "line": h.line, "number": h.number, "until": day_label(&h.until) })).collect();
    let rows: Vec<Value> = c
        .online
        .subs
        .iter()
        .rev()
        .map(|x| {
            let main = x.main == c.id;
            let other = if main { &x.sub_name } else { &x.main_name };
            let title = if main { trf("%{lines}: run by %{who} for you", &[("lines", &x.numbers.join(", ")), ("who", other)]) } else { trf("%{lines}: you run them for %{who}", &[("lines", &x.numbers.join(", ")), ("who", other)]) };
            let say = trf("From %{from} to %{until}, at %{price} a timetable hour.", &[("from", &day_label(&x.from)), ("until", &day_label(x.end())), ("price", &eur(x.hour))]);
            let state = match x.state {
                State::Proposed => tr("Proposed"),
                State::Accepted => tr("Accepted: runs when the joint bid wins"),
                State::Running if !x.last.is_empty() => trf("Ends with notice on %{d}", &[("d", &day_label(&x.last))]),
                State::Running => tr("Running"),
                State::Declined => tr("Declined"),
                State::Ended => tr("Ended"),
            };
            json!({
                "id": x.id, "number": x.numbers.first().cloned().unwrap_or_default(), "title": title, "say": say, "state": state,
                "answer": x.state == State::Proposed && !main, "withdraw": x.state == State::Proposed && main,
                "end": matches!(x.state, State::Running | State::Accepted) && x.last.is_empty(),
            })
        })
        .collect();
    if rows.is_empty() && (held.is_empty() || others.is_empty()) {
        return Value::Null;
    }
    json!({
        "title": tr("Subcontracts"), "rows": rows, "others": others, "held": held,
        "hint": trf("Give lines of your contracts to another company of the map for a price a timetable hour: it runs them, you are paid by the authority and pay it its hours. Either may end it with %{n} days' notice.", &[("n", &NOTICE_DAYS.to_string())]),
        "accept": tr("Accept"), "decline": tr("Decline"), "withdraw": tr("Withdraw"), "end_label": tr("End with notice"),
        "propose": tr("Propose"), "lines_label": tr("Lines"), "who_label": tr("Company"), "price_label": tr("A timetable hour"), "from_label": tr("From"),
        "tomorrow": dates::add(&c.date, 1),
    })
}

/// (NexiconDK, 31 October 2026) The map's tenders as their sealed bids were opened at their
/// close, the latest first: who won at what, and every bid with its points - the company's own,
/// the other companies' and the operators', a rejected one marked (`sealed::score`).
fn results(c: &Company) -> Value {
    if !c.online.on_server {
        return Value::Null;
    }
    let mut list: Vec<&Tender> = c.concessions.tenders.iter().filter(|t| t.market.is_some() && !t.open() && !t.standings.is_empty()).collect();
    list.sort_by(|a, b| b.closes_at.cmp(&a.closes_at));
    let rows: Vec<Value> = list
        .iter()
        .take(20)
        .map(|t| {
            let winner = t.standings.first().filter(|s| !s.rejected);
            let bids: Vec<Value> = t
                .standings
                .iter()
                .map(|s| {
                    let who = if s.name.is_empty() { tr("You") } else { s.name.clone() };
                    let points = if s.rejected {
                    tr("rejected: abnormally low or above the cap")
                } else if s.parts.iter().any(|x| *x != 0.0) {
                    // (3 November 2026: the Viborg model's quality by its parts)
                    let f = |x: f64| format!("{x:.1}");
                    trf("%{p} price + %{q} quality (operation %{a}, vehicles %{v}, plan %{l}, working environment %{w}) = %{t} points", &[("p", &f(s.price)), ("q", &f(s.quality)), ("a", &f(s.parts[0])), ("v", &f(s.parts[1])), ("l", &f(s.parts[2])), ("w", &f(s.parts[3])), ("t", &f(s.total))])
                } else {
                    trf("%{p} price + %{q} quality = %{t} points", &[("p", &format!("{:.1}", s.price)), ("q", &format!("{:.1}", s.quality)), ("t", &format!("{:.1}", s.total))])
                };
                    json!({ "who": who, "bid": money(t, s.amount), "points": points, "rejected": s.rejected, "me": s.name.is_empty(), "won": winner.is_some_and(|w| std::ptr::eq(w, s)) })
                })
                .collect();
            let said = match winner {
                Some(w) if w.name.is_empty() => trf("You won with %{amount}", &[("amount", &money(t, w.amount))]),
                Some(w) => trf("%{who} won with %{amount}", &[("who", &w.name), ("amount", &money(t, w.amount))]),
                None => tr("No valid bid"),
            };
            let failed = t.market.as_ref().map(|m| m.failed.clone()).filter(|f| !f.is_empty()).map(|f| trf("Out again: %{who} gave the line up", &[("who", &f)]));
            json!({ "id": t.id, "number": t.number, "caption": caption_of(t), "closed": when(c, t.closes_at), "said": said, "tone": if winner.is_some_and(|w| w.name.is_empty()) { "ok" } else { "soft" }, "failed": failed, "bids": bids })
        })
        .collect();
    json!({
        "title": tr("Tender results"), "sub": tr("Every bid of the map's tenders, opened at their close"), "rows": rows,
        "empty": tr("No tender of the map has closed yet: when one does, every bid is shown here with its points."),
    })
}

/// (NexiconDK, decided 25 October 2026) The buses offered with a line the company won from
/// another company of the server, not answered yet (`takeover`): each with its buses and their
/// book value, taken over all or none.
fn offers(c: &Company) -> Value {
    let rows: Vec<Value> = c
        .online
        .takeover
        .offers
        .iter()
        .filter(|o| o.answer.is_none())
        .map(|o| {
            json!({
                "id": o.id, "number": o.number,
                "title": trf("%{name}'s buses of line %{n}", &[("name", &o.name), ("n", &o.number)]),
                "say": trf("%{count} buses at their book value, %{amount} together, taken over on %{date}, the day your term begins.", &[("count", &o.buses.len().to_string()), ("amount", &eur(o.total())), ("date", &day_label(&o.date))]),
                "buses": o.buses.iter().map(|b| json!([b.label, eur(b.value)])).collect::<Vec<_>>(),
            })
        })
        .collect();
    if rows.is_empty() {
        return Value::Null;
    }
    json!({ "title": tr("Buses offered"), "rows": rows, "yes": tr("Take them over"), "no": tr("No, thank you") })
}

/// (NexiconDK, 3 November 2026) The authority's doubling orders (`doubling`): those to answer -
/// until 18:00 the day before they begin - and those said yes to still to run, with what they
/// ran and were paid.
fn doubling_orders(c: &Company) -> Value {
    use omsi_launcher_lib::company::doubling::{Answer, DayKind};
    let now = ck::now(c);
    let today = c.date.clone();
    let hm = |m: i32| format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(1440) % 60);
    let rows: Vec<Value> = c
        .online
        .doubling
        .orders
        .iter()
        .filter(|o| omsi_launcher_lib::company::dates::between(&today, &o.until) >= 0 && o.answer != Answer::No)
        .map(|o| {
            let days = match o.day {
                Some(DayKind::Weekday) => tr("weekdays"),
                Some(DayKind::Saturday) => tr("Saturdays"),
                Some(DayKind::Sunday) => tr("Sundays"),
                None => tr("every day"),
            };
            let ask = o.answer == Answer::Asked && now < o.answer_by() && o.by != "admin";
            let state = match o.answer {
                Answer::Asked => trf("Answer by 18:00 on %{d}", &[("d", &day_label(&omsi_launcher_lib::company::dates::add(&o.from, -1)))]),
                _ if o.by == "admin" => tr("Ordered by the authority: to be run"),
                _ if o.by == "dispatcher" => tr("Your dispatcher said yes"),
                _ => tr("You said yes"),
            };
            json!({
                "id": o.id, "number": o.number,
                "title": trf("An extra bus behind the %{time} of tour %{tour}", &[("time", &hm(o.dep)), ("tour", &o.tour)]),
                "say": trf("Its %{days} from %{from} to %{until}: %{runs} runs so far, %{paid} paid; %{left} full departures without it.", &[("days", &days), ("from", &day_label(&o.from)), ("until", &day_label(&o.until)), ("runs", &o.runs.to_string()), ("paid", &eur(o.paid)), ("left", &o.declined.to_string())]),
                "state": state, "ask": ask,
            })
        })
        .collect();
    if rows.is_empty() {
        return Value::Null;
    }
    json!({ "title": tr("Doubling: extra buses"), "rows": rows, "yes": tr("Run it"), "no": tr("Decline") })
}

/// The tenders: announced and running first, then the results (`concessions::tenders`).
fn tenders(c: &Company, list: &[Tender], weeks: &HashMap<String, Week>, reading: bool, today_read: bool, selected: Option<u32>) -> Value {
    let note = cn::may_add_directly(c).then(|| tr("On an easy economy the map's lines are taken on directly on the Lines page and their concessions renew by themselves; tenders are offered all the same, for the practice."));
    let empty = list.is_empty().then(|| tr(if !today_read { "Reading the timetable…" } else { "No line is out to tender now. A new round comes every four weeks; a line of the map can also be applied for on the Lines page." }));
    let now = ck::now(c);
    let rows: Vec<Value> = list
        .iter()
        .map(|t| {
            let week = t.week.or_else(|| weeks.get(&t.line.to_lowercase()).copied());
            // (Luc 2026-10-16: the tender's card - its contract, tours, km a week and cap as chips)
            let mut chips = Vec::new();
            if t.scored() {
                chips.push(json!({ "text": tr(t.kind.label()), "tone": "accent", "icon": "description" }));
            }
            match week {
                Some(wk) => {
                    // (NexiconDK: its tours by the day - Monday to Friday, Saturday, Sunday)
                    let text = if wk.by_day { trf("%{wd} tours Mon–Fri · %{sat} Sat · %{sun} Sun", &[("wd", &wk.mon_fri.to_string()), ("sat", &wk.sat.to_string()), ("sun", &wk.sun.to_string())]) } else { trf("%{n} tours", &[("n", &wk.tours.to_string())]) };
                    chips.push(json!({ "text": text, "tone": "dim", "icon": "directions_bus" }));
                    chips.push(json!({ "text": if wk.km < 1.0 { trf("%{n} trips a week", &[("n", &wk.trips.to_string())]) } else { trf("%{km} km a week", &[("km", &grouped(wk.km))]) }, "tone": "dim", "icon": "route" }));
                }
                None => chips.push(json!({ "text": trf("%{n} tours", &[("n", &t.day_tours.to_string())]), "tone": "dim", "icon": "directions_bus" })),
            }
            if t.scored() {
                chips.push(json!({ "text": trf("cap %{amount}", &[("amount", &money(t, t.reference))]), "tone": "early", "icon": "payments" }));
            }
            let countdown = if !t.open() {
                None
            } else if now < t.opens_at {
                Some(json!([trf("opens in %{span}", &[("span", &span_text(t.opens_at - now))]), "early"]))
            } else {
                Some(json!([trf("closes in %{span}", &[("span", &span_text(t.closes_at - now))]), if t.closes_at - now < 60 { "warn" } else { "ok" }]))
            };
            let figures = match week {
                Some(wk) if wk.km < 1.0 => trf("%{t} tours, %{n} trips a week", &[("t", &wk.tours.to_string()), ("n", &wk.trips.to_string())]),
                Some(wk) => trf("%{t} tours, %{km} km a week", &[("t", &wk.tours.to_string()), ("km", &format!("{:.0}", wk.km))]),
                None if reading => tr("Reading its week…"),
                None => trf("%{t} tours, %{km} km on the day it was offered", &[("t", &t.day_tours.to_string()), ("km", &format!("{:.0}", t.day_km))]),
            };
            // (the contract it lets, and the authority's cap)
            let cap = if t.market.is_some() { trf("cap %{amount}", &[("amount", &money(t, t.reference))]) } else { trf("cap %{amount} a year", &[("amount", &eur(t.reference))]) };
            let figures = if t.scored() { format!("{}  ·  {figures}  ·  {cap}", tr(t.kind.label())) } else { figures };
            let (state, tone) = state_of(c, t);
            let (bid, bid_tone) = my_bid_text(c, t).unwrap_or_else(|| (tr("You have not bid"), "faint"));
            let badge = if t.renewal {
                Some(json!({ "text": tr("renewal").to_uppercase(), "tone": "accent2" }))
            } else if t.running(now) {
                Some(json!({ "text": tr("live").to_uppercase(), "tone": "ok" }))
            } else {
                None
            };
            json!({
                "id": t.id, "number": t.number, "caption": caption_of(t), "dim": !t.open(), "on": selected == Some(t.id),
                "figures": figures, "state": state, "tone": tone, "bid": bid, "bid_tone": bid_tone, "badge": badge,
                "chips": chips, "countdown": countdown,
                // (NexiconDK, the owner's wish: what the contract asks, behind a button of its card)
                "terms": t.market.is_some().then(|| requirements(c, t, week)), "terms_label": t.market.is_some().then(|| tr("Contract requirements")),
            })
        })
        .collect();
    let open = list.iter().filter(|t| t.open()).count();
    json!({ "title": tr("Open tenders"), "count": open, "note": note, "empty": empty, "rows": rows })
}

/// The concessions the company holds and its own lines (`concessions::held`): each with its bus
/// contract (kind, payment, term, rating and points) - or commercial, without one.
fn held(c: &Company) -> Value {
    let today = c.date.clone();
    let rows: Vec<Value> = c
        .concessions
        .held
        .iter()
        .map(|h| {
            let left = dates::between(&today, &h.until);
            let until = if dates::between(&today, &h.from) > 0 { trf("from %{date}", &[("date", &day_label(&h.from))]) } else { trf("until %{date}", &[("date", &day_label(&h.until))]) };
            let k = bc::of_line(c, &h.line);
            let price = match k {
                Some(k) => format!("{}  ·  {}", tr(k.kind.label()), kc::pay_text(c, k)),
                None => tr("Commercial: no contract, the fares are the company's"),
            };
            let more = k.map(|k| format!("{}  ·  {}", kc::points_text(c, k), kc::unpaid_text(c, k)));
            let caption = c.lines.iter().find(|l| l.name.eq_ignore_ascii_case(&h.line)).map(|l| l.caption.clone()).filter(|x| !x.is_empty()).unwrap_or_else(|| h.line.clone());
            let change = k.filter(|k| matches!(k.kind, bc::Kind::Gross | bc::Kind::Net) && k.live()).map(|_| tr("Request a change"));
            json!({ "line": h.line, "change": change, "number": h.number, "caption": caption, "until": until, "tone": if left <= cn::RENEW_BEFORE { "warn" } else { "text" }, "price": price, "more": more, "points": k.map(|k| kc::points_text(c, k)), "unpaid": k.map(|k| kc::unpaid_text(c, k)), "rag": k.map(|k| kc::rag_json(k.rag(&today))), "direct": (h.direct && k.is_none()).then(|| tr("commercial").to_uppercase()) })
        })
        .collect();
    let lic = eur((cn::LICENCE as f64 * c.price_index).round() as i64);
    let own: Vec<Value> = c
        .lines
        .iter()
        .filter(|x| x.own)
        .map(|x| {
            let text = match bc::of_line(c, &x.name) {
                Some(k) => format!("{}  ·  {}", tr(k.kind.label()), kc::pay_text(c, k)),
                None => trf("Your own line: no concession, a licence of %{amount} a month", &[("amount", &lic)]),
            };
            let k = bc::of_line(c, &x.name);
            json!({ "number": x.number, "caption": x.caption, "text": text, "points": k.map(|k| kc::points_text(c, k)), "rag": k.map(|k| kc::rag_json(k.rag(&today))) })
        })
        .collect();
    let empty = (rows.is_empty() && own.is_empty()).then(|| tr("The company holds no concession yet. Win a tender, or apply for a line of the map on the Lines page."));
    json!({ "title": tr("Your contracts"), "sub": record_short(c), "rows": rows, "own": own, "empty": empty, "own_label": tr("own line") })
}

/// The track record in a few words: the default points a contract and the quality points they
/// give in the next tenders.
fn record_short(c: &Company) -> String {
    let pts = bc::scored_points(c);
    trf("Track record: %{pts} default points a contract  ·  %{q} of 30 quality points", &[("pts", &num(pts, 1)), ("q", &num(bc::quality_points(pts), 1))])
}

/// (NexiconDK, decided 25 October 2026) What a contract of the map's market asks of the company
/// on Bus Company Online's server, in groups of a label and its words: the buses (its tours by
/// the day and by the bus their timetable asks for, the term by the buses bid with), the tender
/// (three days of bidding, the term's first day, the renewal, the buses taken over) and the
/// authority's fines (`server_fines`, in the company's currency).
fn requirements(c: &Company, t: &Tender, week: Option<Week>) -> Value {
    use omsi_launcher_lib::company::server_fines::{self, Fines};
    let row = |label: &str, text: String| json!([tr(label), text]);
    let s = t.sizes;
    let mut sizes: Vec<String> = Vec::new();
    for (n, key) in [(s.articulated, "%{n} with an articulated bus"), (s.double, "%{n} with a double-decker"), (s.solo, "%{n} with a standard bus"), (s.midi, "%{n} with a midibus"), (s.any, "%{n} with any bus")] {
        if n > 0 {
            sizes.push(trf(key, &[("n", &n.to_string())]));
        }
    }
    let tours = match week {
        Some(wk) if wk.by_day => trf("%{wd} tours Mon–Fri · %{sat} Sat · %{sun} Sun", &[("wd", &wk.mon_fri.to_string()), ("sat", &wk.sat.to_string()), ("sun", &wk.sun.to_string())]),
        _ => trf("%{n} tours a day", &[("n", &t.day_tours.to_string())]),
    };
    let buses = vec![
        row("Tours", tours),
        row("Bus types", if sizes.is_empty() { tr("Any bus") } else { sizes.join(" · ") }),
        row("The rule", tr("Each tour is run with the bus its timetable asks for, or a bigger one.")),
        row("Diesel, CNG or electric", lengths_text(t)),
    ];
    // (NexiconDK, 3 November 2026: what the tender asks of the buses - floor, fuel, age, screens -
    // each against the company's own buses: ✓ all of them, ! how many do not)
    let mut buses = buses;
    if let Some(n) = t.market.as_ref().and_then(|m| m.needs) {
        use omsi_launcher_lib::company::server_buses as sb;
        let all = c.fleet.len();
        let check = |ok: usize| {
            if all == 0 {
                tr("you have no buses yet")
            } else if ok == all {
                trf("✓ all %{n} of your buses", &[("n", &all.to_string())])
            } else {
                trf("! %{k} of your %{n} buses do not", &[("k", &(all - ok).to_string()), ("n", &all.to_string())])
            }
        };
        let count = |f: &dyn Fn(&core::company::model::Vehicle) -> bool| c.fleet.iter().filter(|v| f(v)).count();
        buses.push(row("Floor", format!("{} · {}", tr(n.floor.label()), check(count(&|v| n.floor.allows(sb::floor_of(c, v)))))));
        buses.push(row("Fuel", format!("{} · {}", tr(n.fuel.label()), check(count(&|v| n.fuel.allows(v.kind.drive))))));
        if n.max_age > 0 {
            buses.push(row("Age", format!("{} · {}", trf("At most %{n} years old", &[("n", &n.max_age.to_string())]), check(count(&|v| n.age_ok(c, v))))));
        }
        if n.screens {
            buses.push(row("Passenger information", format!("{} · {}", tr("Screens in every bus"), check(count(&|v| sb::screens_of(c, v))))));
        }
    }
    // (NexiconDK, 3 November 2026: the service starts two days after the close - `server_start`)
    let from = cn::server_start(t);
    // (NexiconDK, 3 November 2026: the term's last day and the next tender's days - by the buses
    // the contract is won with; a special service's: its event's last day, out again with its next)
    let event_until = t.market.as_ref().filter(|m| !m.event.is_empty()).map(|m| m.until.clone()).filter(|u| dates::parse(u).is_some());
    let ends: Vec<Value> = match event_until {
        Some(until) => vec![row("The term ends", trf("%{date}, the event's last day", &[("date", &day_label(&until))])), row("Out to tender again", tr(co_tenders::EVENT_LINE))],
        None => [(Drive::Diesel, "Ends with diesel buses"), (Drive::Cng, "Ends with CNG buses"), (Drive::Electric, "Ends with electric buses")]
            .into_iter()
            .map(|(d, label)| {
                let end = cn::server_term(d, cn::week_hours(t), &from);
                let next = dates::add(&end, 1);
                let (open, close) = (dates::add(&next, -co_tenders::RENEW_OPENS), dates::add(&next, -co_tenders::RENEW_DECIDED));
                row(label, trf("%{end} · out to tender again from %{open} to %{close}", &[("end", &day_label(&end)), ("open", &day_label(&open)), ("close", &day_label(&close))]))
            })
            .collect(),
    };
    let mut tender = vec![
        row("Bidding", trf("Three days: from %{open} to %{close}", &[("open", &when(c, t.opens_at)), ("close", &when(c, t.closes_at))])),
        row("The term", trf("From %{date}. Option: %{d} days more, at most %{n} times, when run satisfactorily (not red, at most 2 % of its journeys dropped)", &[("date", &day_label(&from)), ("d", &co_tenders::OPTION_DAYS.to_string()), ("n", &co_tenders::OPTIONS.to_string())])),
        row("Renewal", tr("The next term is announced six days before it begins, takes bids from five days before and is decided two days before it: the winner has two days to make ready.")),
        row("The buses", tr("The winner may take over the buses its last operator ran the line with, at their book value.")),
        row("The cap", trf("%{amount}: an ask above it does not count", &[("amount", &money(t, t.reference))])),
        row("Payment", tr("Asks are a price a timetable hour. The contract pays it each day for the timetable hours run: a journey not run is not paid.")),
    ];
    tender.splice(2..2, ends);
    let f = c.online.fines.unwrap_or_else(|| server_fines::of_folder(&super::super::data()));
    let kr = |x: f64| eur(Fines::cents(x));
    let fines = vec![
        row("A journey not run", trf("%{amount} a minute of its time", &[("amount", &eur_cents(Fines::cents(f.dropped_minute) as f64))])),
        row("A smaller bus than the tour's", trf("%{amount} a journey", &[("amount", &kr(f.wrong_bus))])),
        row("A diesel bus on an electric tour", trf("%{amount} a journey", &[("amount", &kr(f.diesel_on_electric))])),
        row("A bus of a fuel the tender does not allow", trf("%{amount} a journey", &[("amount", &kr(f.wrong_fuel))])),
        row("A bus of a floor the tender does not allow", trf("%{amount} a journey", &[("amount", &kr(f.wrong_floor))])),
        row("Leaving a stop early", trf("%{amount} a departure more than a minute early", &[("amount", &kr(f.early))])),
        row("Leaving a stop late", trf("%{amount} a departure more than 15 minutes late, unless the delay was reported to the control room: a player reports his on his duty, the dispatcher those of the company's drivers", &[("amount", &kr(f.late))])),
        row("A complaint about the driving", kr(f.driving)),
        row("A complaint about the temperature", kr(f.temperature)),
        row("A complaint about the route", kr(f.route)),
        row("A ticket sold wrong", kr(f.ticket)),
        row("A red light run", kr(f.red_light)),
    ];
    // (NexiconDK, 3 November 2026: the work - a package's lines, each with its caption)
    let fg = cn::figures(t);
    let g = |x: f64| grouped(x.round());
    let mut work = vec![
        row("Timetable hours", trf("%{h} a week · over the contract: %{list}", &[("h", &g(fg.hours_week)), ("list", &term_hours(t, fg.hours_week))])),
        row("Buses", trf("%{n} contract buses and %{r} reserve buses: the reserve is not paid as contract buses", &[("n", &fg.buses.to_string()), ("r", &fg.reserve.to_string())])),
        row("Drivers", trf("about %{n}: the week's paid hours (1.5 a timetable hour) by a driver's week of 37 hours", &[("n", &fg.drivers.to_string())])),
    ];
    if fg.pax.iter().any(|x| *x > 0.0) {
        work.push(row("Passengers a day", trf("%{wd} Mon–Fri · %{sat} Saturday · %{sun} Sunday, as the game expects them", &[("wd", &g(fg.pax[0])), ("sat", &g(fg.pax[1])), ("sun", &g(fg.pax[2]))])));
    }
    for (number, caption) in &fg.lines {
        work.push(json!([trf("Line %{n}", &[("n", number)]), caption]));
    }
    let title = if t.is_package() { trf("Contract requirements: the package %{n}", &[("n", &t.number)]) } else { trf("Contract requirements: line %{n}", &[("n", &t.number)]) };
    json!({
        "title": title,
        "groups": [
            { "caps": tr("The work").to_uppercase(), "rows": work },
            { "caps": tr("The buses").to_uppercase(), "rows": buses },
            { "caps": tr("The tender").to_uppercase(), "rows": tender },
            { "caps": tr("The authority's fines").to_uppercase(), "rows": fines },
        ],
        "close": tr("Close"),
    })
}

/// (NexiconDK, 3 November 2026) The work of a tender of the server's as the tender page shows it
/// (`concessions::figures`): its timetable hours a week and in all, its duties a day, its buses
/// and reserve, the drivers it needs, its passengers a day - a package's the lines' together, and
/// (NexiconDK, 3 November 2026) The timetable hours over the contract with each fuel the tender's
/// requirements allow, each with its term's days ("643 with diesel (14 days), 803 with CNG (17
/// days), 964 with electric (21 days)").
fn term_hours(t: &Tender, hours_week: f64) -> String {
    use omsi_launcher_lib::company::model::Drive;
    let size = cn::WorkSize::of_hours(hours_week);
    let rule = t.market.as_ref().and_then(|m| m.needs).map(|n| n.fuel);
    [(Drive::Diesel, "%{h} with diesel (%{d} days)"), (Drive::Cng, "%{h} with CNG (%{d} days)"), (Drive::Electric, "%{h} with electric (%{d} days)")]
        .iter()
        .filter(|(d, _)| rule.is_none_or(|r| r.allows(*d)))
        .map(|(d, text)| {
            let days = cn::server_days(*d, size);
            trf(text, &[("h", &grouped((hours_week / 7.0 * days as f64).round())), ("d", &days.to_string())])
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// its lines. As tiles: label, value, the line under it.
fn work_tiles(c: &Company, t: &Tender) -> Vec<Value> {
    let f = cn::figures(t);
    let g = |x: f64| grouped(x.round());
    let tile = |label: &str, value: String, sub: String| json!({ "label": tr(label).to_uppercase(), "value": value, "sub": sub });
    let mut tiles = vec![
        tile("Timetable hours", trf("%{h} a week", &[("h", &g(f.hours_week))]), trf("over the contract: %{list}", &[("list", &term_hours(t, f.hours_week))])),
        if f.by_day {
            tile("Duties a day", format!("{} · {} · {}", f.duties.0, f.duties.1, f.duties.2), tr("Mon–Fri · Saturday · Sunday"))
        } else {
            tile("Duties a day", f.duties.0.to_string(), tr("on the day it was offered"))
        },
        tile("Buses · drivers", format!("{} + {} · ≈ {}", f.buses, f.reserve, f.drivers), tr("contract + reserve · drivers expected")),
    ];
    if f.pax.iter().any(|x| *x > 0.0) {
        tiles.push(tile("Passengers a day", format!("{} · {} · {}", g(f.pax[0]), g(f.pax[1]), g(f.pax[2])), tr("Mon–Fri · Saturday · Sunday, expected")));
    }
    if !f.lines.is_empty() {
        tiles.push(tile("Lines", f.lines.len().to_string(), f.lines.iter().map(|l| l.0.clone()).collect::<Vec<_>>().join(", ")));
    }
    // (3 November 2026: the departures expected full, the doubling they would bring and its price
    // with a bus of no contract - not in the score: `doubling`)
    if f.full_week > 0.0 {
        let rules = c.online.doubling_rules.unwrap_or_default();
        let price = eur((rules.price_hour as f64 * c.price_index).round() as i64);
        tiles.push(tile("Doubling (extra buses)", trf("≈ %{n} full a week · ≈ %{h} hours", &[("n", &g(f.full_week)), ("h", &format!("{:.1}", f.doubling_hours))]), trf("%{p} an hour with a bus of no contract · not in the score", &[("p", &price)])));
    }
    // (3 November 2026: the holidays of a diesel term from its start - fewer journeys then, the
    // buses paid every day: `contract_events`)
    let start = cn::server_start(t);
    let days = cn::server_days(omsi_launcher_lib::company::model::Drive::Diesel, cn::WorkSize::of_hours(cn::week_hours(t)));
    let holidays = core::holiday_days(&c.map, &start, days);
    if holidays > 0 {
        tiles.push(tile("Holidays", trf("%{n} of %{d} days", &[("n", &holidays.to_string()), ("d", &days.to_string())]), tr("public or school holidays in a diesel term: fewer journeys, the buses paid every day")));
    }
    tiles
}

/// (NexiconDK, 3 November 2026) The authority's changes of the company's contracts
/// (`contract_events`): waiting for their day - to decline while it may -, applied, declined.
fn changes(c: &Company) -> Value {
    use omsi_launcher_lib::company::contract_events::ChangeKind;
    let today = c.date.clone();
    let sign = |x: f64| if x.round() > 0.0 { format!("+{:.0}", x.round()) } else { format!("{:.0}", x.round()) };
    let rows: Vec<Value> = c
        .online
        .changes
        .iter()
        .rev()
        .take(8)
        .map(|ch| {
            let title = match ch.kind {
                ChangeKind::Timetable => trf("%{name}: the map's new timetable", &[("name", &ch.name)]),
                ChangeKind::AddLine => trf("%{name}: line %{n} added", &[("name", &ch.name), ("n", &ch.number)]),
                ChangeKind::RemoveLine => trf("%{name}: line %{n} taken out", &[("name", &ch.name), ("n", &ch.number)]),
            };
            let say = trf("From %{date}: %{h} timetable hours a week (%{before} → %{after}), %{b} contract buses, %{s} stops.", &[("date", &day_label(&ch.from)), ("h", &sign(ch.hours_after - ch.hours_before)), ("before", &format!("{:.0}", ch.hours_before)), ("after", &format!("{:.0}", ch.hours_after)), ("b", &sign(ch.buses_after as f64 - ch.buses_before as f64)), ("s", &sign(ch.stops_after as f64 - ch.stops_before as f64))]);
            let ask = ch.declinable(&today);
            let state = match ch.state.as_str() {
                "applied" => tr("Applied"),
                "declined" => tr("Declined"),
                _ if ask => trf("You may decline it until %{d}", &[("d", &day_label(&ch.decline_by))]),
                _ => tr("To be run: plan your duties for it"),
            };
            json!({ "id": ch.id, "number": ch.number, "title": title, "say": say, "state": state, "ask": ask })
        })
        .collect();
    if rows.is_empty() {
        return Value::Null;
    }
    json!({ "title": tr("Changes of your contracts"), "rows": rows, "decline": tr("Decline") })
}

/// (NexiconDK, 3 November 2026) A server contract's term by the buses it is won with and the size
/// of its work (`concessions::server_days`).
fn term_text(t: &Tender) -> String {
    let days = cn::server_days(t.drive, cn::WorkSize::of_hours(cn::week_hours(t))).to_string();
    drive_days(t.drive, &days)
}

/// (3 November 2026) "Diesel · 14 days", "CNG · 17 days", "Electric · 21 days".
fn drive_days(d: Drive, days: &str) -> String {
    let key = match d {
        Drive::Electric => "Electric · %{n} days",
        Drive::Cng => "CNG · %{n} days",
        Drive::Diesel => "Diesel · %{n} days",
    };
    trf(key, &[("n", days)])
}

/// How a tender's contract is paid, in words.
fn per_text(p: Period) -> String {
    match p {
        Period::Day => tr("paid by the operating day"),
        Period::Week => tr("paid by the week"),
        Period::Month => tr("paid by the month"),
    }
}

/// The auction of the tender picked (`concessions::auction`). `amount` (euros): the bid slider's
/// sum the page holds.
fn auction(c: &Company, selected: Option<u32>, args: &Value) -> Value {
    let title = tr("Auction");
    let Some(t) = selected.and_then(|id| c.concessions.tenders.iter().find(|t| t.id == id)) else {
        return json!({ "title": title, "none": tr("Pick a tender on the left: its auction is shown here as it runs.") });
    };
    let now = ck::now(c);
    let (state, tone) = state_of(c, t);
    let mine = my_bid_text(c, t);
    let least = cn::min_bid(c, t);
    let advice = cn::advice(c, t);
    let hourly = t.market.is_some();
    // (NexiconDK, 31 October 2026: the map's tenders take sealed bids - `sealed`)
    let sealed = t.market.is_some();
    let sealed_open = sealed && t.open();
    let low = omsi_launcher_lib::company::sealed::low_line(t);
    let cap_tip = if hourly { tr("The most the authority pays a timetable hour: an ask above it does not count.") } else { tr("The most the authority pays a year: an ask above it does not count.") };
    // (NexiconDK, 3 November 2026: the advice never under the abnormally low line nor over the cap
    // - the box said the company's raw cost price, the line under the slider the one it may ask)
    let advised = if sealed { cn::recommended(c, t).0 } else { advice };
    let figures = if sealed {
        json!([
            { "label": tr("Advice").to_uppercase(), "value": money(t, advised), "tone": "ok", "tip": tr("The work's cost price at your own costs, with a 5 % margin: drivers' paid hours, fuel, the workshop, a bus and its fixed costs, less BSOG.") },
            { "label": tr("Lowest valid ask").to_uppercase(), "value": money(t, low), "tone": "warn", "tip": tr("85 % of the authority's cost price: a bid under it is abnormally low and is rejected when the bids are opened.") },
            { "label": tr("The cap").to_uppercase(), "value": money(t, t.reference), "tone": "text", "tip": tr("The authority's cost price by one and a half, at most the server's cap: an ask above it does not count.") },
            if t.open() { json!({ "label": tr("Deadline").to_uppercase(), "value": when(c, t.closes_at), "tone": "text" }) } else if let Some((_, days)) = event_of(t) { json!({ "label": tr("Term").to_uppercase(), "value": days, "tone": "text" }) } else { json!({ "label": tr("Term").to_uppercase(), "value": term_text(t), "tone": "text" }) },
        ])
    } else {
        json!([
        { "label": tr("The cap").to_uppercase(), "value": money(t, t.reference), "tone": "text", "tip": cap_tip },
        { "label": tr("Advice").to_uppercase(), "value": money(t, advice), "tone": "ok", "tip": tr("The work's cost price at your own costs, with a 5 % margin: drivers' paid hours, fuel, the workshop, a bus and its fixed costs, less BSOG.") },
        if t.open() { json!({ "label": tr("Ask at most now").to_uppercase(), "value": money(t, least), "tone": "text" }) } else if hourly { json!({ "label": tr("Term").to_uppercase(), "value": tr(if t.drive == Drive::Electric { "Electric · a month" } else { "Diesel · two weeks" }), "tone": "text" }) } else { json!({ "label": tr("Term").to_uppercase(), "value": trf("%{n} years", &[("n", &t.term_years().to_string())]), "tone": "text" }) },
    ])
    };
    // (NexiconDK: no line is bought outright on Bus Company Online's server)
    let mut figures = figures;
    if !c.online.on_server {
        if let Some(f) = figures.as_array_mut() {
            f.push(json!({ "label": tr("Buy outright").to_uppercase(), "value": eur(t.buy_out()), "tone": "accent2" }));
        }
    }
    let (_, ext) = bc::term(t.kind);
    let terms = t.scored().then(|| match t.market {
        // (NexiconDK: on the server its term is by the buses it is bid with)
        // (a special service: its event's days alone)
        Some(_) if event_of(t).is_some() => {
            let (event, days) = event_of(t).unwrap_or_default();
            trf("%{kind}, %{per}  ·  a special service for %{event}, its days alone: %{days}  ·  asks are a price a timetable hour, sealed until the close", &[("kind", &tr(t.kind.label())), ("per", &per_text(t.period)), ("event", &event), ("days", &days)])
        }
        Some(_) => trf("%{kind}, %{per}  ·  two weeks with diesel buses, a month with electric ones  ·  asks are a price a timetable hour, sealed until the close", &[("kind", &tr(t.kind.label())), ("per", &per_text(t.period))]),
        None => trf("%{kind}, %{per}  ·  %{years} years and up to %{ext} more  ·  asks are a price a year", &[("kind", &tr(t.kind.label())), ("per", &per_text(t.period)), ("years", &t.term_years().to_string()), ("ext", &ext.to_string())]),
    });
    // (and the buses the company bids with: its next bid carries them)
    // (3 November 2026: CNG too, and only the fuels the tender's requirements allow)
    let drive = (t.market.is_some() && t.open()).then(|| {
        use omsi_launcher_lib::company::server_buses as sb;
        let size = cn::WorkSize::of_hours(cn::week_hours(t));
        let allowed = |d: Drive| t.market.as_ref().and_then(|m| m.needs).is_none_or(|n| n.drive_ok(d));
        let options: Vec<Value> = [Drive::Diesel, Drive::Cng, Drive::Electric]
            .into_iter()
            .filter(|d| allowed(*d))
            .map(|d| {
                let label = if event_of(t).is_some() {
                    tr(match d {
                        Drive::Electric => "Electric buses",
                        Drive::Cng => "CNG buses",
                        Drive::Diesel => "Diesel buses",
                    })
                } else {
                    drive_days(d, &cn::server_days(d, size).to_string())
                };
                json!([sb::drive_key(d), label])
            })
            .collect();
        json!({
            "label": tr("Your buses"), "value": sb::drive_key(t.drive),
            "options": options,
            "say": tr(if event_of(t).is_some() { "Your next bid is made with these buses: a special service runs its event's days whichever you choose; with electric buses its tours are run with electric ones." } else { "Your next bid is made with these buses: the contract's days are by them, and with electric buses its tours are run with electric ones." }),
        })
    });
    let len = (t.closes_at - t.opens_at).max(1);
    let frac = if t.open() { ((now - t.opens_at) as f64 / len as f64).clamp(0.0, 1.0) } else { 1.0 };
    let times = if !t.open() {
        trf("Closed at %{when}", &[("when", &when(c, t.closes_at))])
    } else if now < t.opens_at {
        trf("Opens %{when}, runs %{len}", &[("when", &when(c, t.opens_at)), ("len", &span(len))])
    } else {
        trf("Closes at %{time}: %{left} left", &[("time", &ck::hhmm(t.closes_at)), ("left", &span(t.closes_at - now))])
    };
    // the bidders
    let placed = cn::bids(c, t, if t.open() { now } else { t.closes_at });
    let leader = placed.last().map(|p| p.who);
    // (the points: kept at the close, else as they stand now)
    let board = if t.open() { cn::standings(c, t, &placed) } else { t.standings.clone() };
    let points_of = |name: &str| board.iter().find(|s| s.name == name).map(|s| trf("%{p} price + %{q} quality = %{t} points", &[("p", &format!("{:.1}", s.price)), ("q", &format!("{:.1}", s.quality)), ("t", &format!("{:.1}", s.total))]));
    let mut bidders: Vec<Value> = t
        .bidders
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let r = cn::rival_of(c, t, *id)?;
            let last = placed.iter().rev().find(|p| p.who == Who::Rival(i)).map(|p| p.amount);
            let kind = points_of(&r.name).unwrap_or_else(|| format!("{}  ·  {}", kind_of(r), trf("quality %{q}", &[("q", &format!("{:.0}", r.quality))])));
            Some(json!({ "name": r.name, "kind": kind, "value": last.map(|a| money(t, a)).unwrap_or_else(|| tr("no bid")), "has": last.is_some(), "leads": leader == Some(Who::Rival(i)) }))
        })
        .collect();
    // (NexiconDK: the other companies of the map bidding in a tender of its market)
    let (humans, me) = cn::humans_of(c, t);
    let others: Vec<(usize, String, f64)> = humans.iter().enumerate().filter(|(k, _)| *k != me).map(|(k, h)| (k, h.1.clone(), h.2)).collect();
    for (k, name, q) in &others {
        let last = placed.iter().rev().find(|p| p.who == Who::Other(*k)).map(|p| p.amount);
        let kind = points_of(name).unwrap_or_else(|| format!("{}  ·  {}", tr("a company of the server"), trf("quality %{q}", &[("q", &format!("{q:.0}"))])));
        bidders.push(json!({ "name": name, "kind": kind, "value": last.map(|a| money(t, a)).unwrap_or_else(|| tr("no bid")), "has": last.is_some(), "leads": leader == Some(Who::Other(*k)) }));
    }
    let mine_last = placed.iter().rev().find(|p| p.who == Who::Player).map(|p| p.amount);
    let kind = points_of("").unwrap_or_else(|| trf("your quality %{q}", &[("q", &format!("{:.0}", cn::quality_in(c, t)))]));
    bidders.push(json!({ "name": tr("You"), "kind": kind, "value": mine_last.map(|a| money(t, a)).unwrap_or_else(|| tr("no bid")), "has": mine_last.is_some(), "leads": leader == Some(Who::Player) }));
    // (NexiconDK, 31 October 2026: sealed - nothing of the others' while it runs; at the close
    // every bid opened, best first, a rejected one marked)
    if sealed {
        bidders.clear();
    }
    let opened: Vec<Value> = if sealed && !t.open() {
        let operator = |name: &str| t.market.as_ref().and_then(|m| m.rivals.iter().find(|r| r.name == name)).map(|r| kind_of(r));
        t.standings
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let name = if s.name.is_empty() { tr("You") } else { s.name.clone() };
                let kind = if s.name.is_empty() { tr("your company") } else { operator(&s.name).unwrap_or_else(|| tr("a company of the server")) };
                let q = cn::quality_share(t, s.quality);
                let shown = money_parts(t, s.amount, s.bus_day);
                let (bid, tone) = if s.rejected { (trf("%{amount}, rejected", &[("amount", &shown)]), "danger") } else if k == 0 { (trf("%{amount}, won", &[("amount", &shown)]), "ok") } else { (shown.clone(), "text") };
                let points = if s.rejected { tr("rejected: abnormally low or above the cap") } else { trf("%{p} price + %{q} quality = %{t} points", &[("p", &format!("{:.1}", s.price)), ("q", &format!("{:.1}", s.quality)), ("t", &format!("{:.1}", s.total))]) };
                // (NexiconDK, 3 November 2026, K7: a company's share driven live, with its kind)
                let kind = if s.live > 0.0 { format!("{kind} · {}", trf("%{p} % driven live", &[("p", &format!("{:.0}", s.live * 100.0))])) } else { kind };
                json!({ "name": name, "kind": kind, "quality": q, "quality_text": if s.rejected { "–".to_string() } else { format!("{:.0}", q * 100.0) }, "bid": bid, "tone": tone, "leads": k == 0 && !s.rejected, "points": points })
            })
            .collect()
    } else {
        Vec::new()
    };
    // (Luc 2026-10-16: the rivals as a table - their kind, quality and where their bid stands)
    let mut rivals: Vec<Value> = t
        .bidders
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let r = cn::rival_of(c, t, *id)?;
            let last = placed.iter().rev().find(|p| p.who == Who::Rival(i)).map(|p| p.amount);
            let leads = leader == Some(Who::Rival(i));
            let (bid, tone) = match last {
                _ if !t.open() && !t.scored() && last.is_none() => (tr("no bid"), "dim"),
                Some(a) if leads => (trf("%{amount}, leads", &[("amount", &money(t, a))]), "warn"),
                Some(a) => (money(t, a), "text"),
                None if now < t.opens_at => (tr("waits for the opening"), "dim"),
                None => (tr("no bid"), "dim"),
            };
            Some(json!({ "name": r.name, "kind": kind_of(r), "quality": r.quality / 100.0, "quality_text": format!("{:.0}", r.quality), "bid": bid, "tone": tone, "leads": leads, "points": points_of(&r.name) }))
        })
        .collect();
    // (the other companies of the map: theirs among the rivals, as a company of the server)
    for (k, name, q) in &others {
        let last = placed.iter().rev().find(|p| p.who == Who::Other(*k)).map(|p| p.amount);
        let leads = leader == Some(Who::Other(*k));
        let (bid, tone) = match last {
            Some(a) if leads => (trf("%{amount}, leads", &[("amount", &money(t, a))]), "warn"),
            Some(a) => (money(t, a), "text"),
            None => (tr("no bid"), "dim"),
        };
        rivals.push(json!({ "name": name, "kind": tr("a company of the server"), "quality": q / 100.0, "quality_text": format!("{q:.0}"), "bid": bid, "tone": tone, "leads": leads, "points": points_of(name) }));
    }
    if sealed {
        rivals = opened;
    }
    let log: Vec<Value> = placed.iter().rev().take(30).map(|p| json!({ "at": ck::hhmm(p.at), "who": if p.who == Who::Player { tr("You") } else { cn::bidder_name(c, t, p.who) }, "amount": money(t, p.amount), "me": p.who == Who::Player })).collect();
    // what to do
    let actions = if t.running(now) {
        // (an ask: from half the reference up to the most that leads now - NexiconDK: on the
        // server by the timetable hour, `cn::per_hour`: from half the cap or half the cost price,
        // whichever is less, to the cent)
        // (3 November 2026: a bid in two parts - the bus part a contract bus a day, at most its
        // cap; the slider is the timetable hour's, the rest of the ask)
        let bus_cap = t.market.as_ref().map_or(0, |m| m.bus_cap);
        let bus_default = t.market.as_ref().map_or(0, |m| ((m.bus_cost as f64 * 1.05).round() as Cents).min(m.bus_cap));
        let bus_mine = args.get("bus_for").and_then(Value::as_u64) == Some(t.id as u64);
        let bus = if bus_cap > 0 { args.get("bus").and_then(Value::as_f64).filter(|_| bus_mine).map_or(bus_default, |b| (b * 100.0).round() as Cents).clamp(0, bus_cap) } else { 0 };
        let bus_year = cn::bus_year(t, bus);
        let unit = |year: f64| if hourly { cn::per_hour(t, (year.round() as Cents - bus_year).max(0)) / 100.0 } else { year / 100.0 };
        let year_of = |a: f64| if hourly { cn::of_hour(t, a * 100.0) + bus_year } else { (a * 100.0).round() as Cents };
        let (lo, hi) = if sealed {
            // (sealed: from the lowest valid ask to the cap)
            let hi = ((unit(t.reference as f64) * 100.0).floor() / 100.0).max(0.01);
            let lo = ((unit(low as f64) * 100.0).ceil() / 100.0).clamp(0.01, hi);
            (lo, hi)
        } else if hourly {
            let hi = ((unit(least as f64) * 100.0).floor() / 100.0).max(0.01);
            let lo = ((unit((t.reference.min(t.cost.max(1)) as f64) * 0.5) * 100.0).round() / 100.0).clamp(0.01, hi);
            (lo, hi)
        } else {
            let hi = (least as f64 / 100.0).max(100.0);
            (((t.reference as f64 * 0.5 / 100.0 / 100.0).round() * 100.0).min(hi - 100.0).max(100.0), hi)
        };
        let normal = c.game_mode == omsi_launcher_lib::company::GameMode::Normal;
        let (rec, rec_chance) = cn::recommended(c, t);
        // (the slider's sum when it is this tender's and not above the bid that leads now;
        // else the recommended ask in Normal, the most that leads otherwise)
        let mine = args.get("amount_for").and_then(Value::as_u64) == Some(t.id as u64);
        let start = if normal || sealed { unit(rec as f64) } else { hi };
        let want = args.get("amount").and_then(Value::as_f64).filter(|a| mine && *a <= hi).unwrap_or(start);
        let a = want.clamp(lo, hi);
        let amount = year_of(a).min(least.max(year_of(lo)));
        // (and what it comes to: a week, the contract's days by the buses bid with)
        let total = hourly.then(|| {
            let from = if t.market.is_some() { cn::server_start(t) } else { dates::add(&ck::date_of(t.closes_at), 1) };
            // (NexiconDK, 3 November 2026: a special service runs its event's days alone - it said
            // the contract's 14 days for a match day)
            let until = t.market.as_ref().filter(|m| !m.event.is_empty()).map(|m| m.until.clone()).filter(|u| dates::parse(u).is_some()).unwrap_or_else(|| cn::server_term(t.drive, cn::week_hours(t), &from));
            let days = dates::between(&from, &until) + 1;
            let hours = cn::hours_year(t) / 365.0 * days as f64;
            let rate = cn::hour_part(t, amount, bus);
            let buses = cn::contract_buses(t);
            let week = rate * cn::hours_year(t) / 52.0 + (bus * buses as Cents * 7) as f64;
            let term = rate * hours + (bus * buses as Cents) as f64 * days as f64;
            if bus > 0 {
                trf("About %{week} a week and %{term} for the contract's %{days} days: %{hours} timetable hours at %{rate} and %{buses} contract buses at %{bus} a day", &[("week", &eur(week.round() as Cents)), ("term", &eur(term.round() as Cents)), ("days", &days.to_string()), ("hours", &format!("{hours:.0}")), ("rate", &eur_cents(rate)), ("buses", &buses.to_string()), ("bus", &eur(bus))])
            } else {
                trf("About %{week} a week and %{term} for the contract's %{days} days: %{hours} timetable hours at %{rate}", &[("week", &eur(week.round() as Cents)), ("term", &eur(term.round() as Cents)), ("days", &days.to_string()), ("hours", &format!("{hours:.0}")), ("rate", &money(t, amount))])
            }
        });
        let chance = cn::chance(c, t, amount);
        let ctone = if sealed { "soft" } else if chance >= 0.7 { "ok" } else if chance >= 0.35 { "warn" } else { "danger" };
        let mut said = if sealed {
            trf("%{s} % of the cap  ·  sealed: nobody sees your bid, nor you theirs, till the close", &[("s", &format!("{:.0}", t.share(amount) * 100.0))])
        } else {
            trf("%{s} % of the cap  ·  chance to win, as it looks now: about %{p} %", &[("s", &format!("{:.0}", t.share(amount) * 100.0)), ("p", &format!("{:.0}", chance * 100.0))])
        };
        if !t.fee_paid && cn::fee(c, t) > 0 {
            said = format!("{said}  ·  {}", trf("taking part costs %{amount}", &[("amount", &eur(cn::fee(c, t)))]));
        }
        json!({
            "kind": "running", "id": t.id, "lo": lo, "hi": hi, "amount": a, "label": tr(if hourly { "Your ask a timetable hour" } else { "Your ask" }),
            "per_hour": hourly, "total": total,
            "bus": (bus_cap > 0).then(|| json!({ "label": trf("A contract bus a day (at most %{cap})", &[("cap", &eur(bus_cap))]), "value": bus as f64 / 100.0, "max": bus_cap as f64 / 100.0, "buses": cn::contract_buses(t) })),
            "chance": said, "chance_tone": ctone,
            "recommend": if sealed && advice < rec { Some(trf("Your cost price with a 5 % margin is %{own}: under the abnormally low line, so ask at least %{amount}", &[("own", &money_parts(t, advice, bus)), ("amount", &money_parts(t, rec, bus))])) } else if sealed { Some(trf("Your cost price with a 5 % margin: %{amount}", &[("amount", &money_parts(t, rec, bus))])) } else { normal.then(|| trf("Recommended: %{amount} (%{s} % of the cap), about %{p} % to win", &[("amount", &money(t, rec)), ("s", &format!("{:.0}", t.share(rec) * 100.0)), ("p", &format!("{:.0}", rec_chance * 100.0))])) },
            "recommend_amount": unit(rec as f64),
            "bid": if sealed && !t.offers.is_empty() { trf("Change your bid to %{amount}", &[("amount", &money_parts(t, amount, bus))]) } else { trf("Ask %{amount}", &[("amount", &money_parts(t, amount, bus))]) },
            "buy": (!c.online.on_server).then(|| trf("Buy outright, %{amount}", &[("amount", &eur(t.buy_out()))])), "buy_tip": tr("Certain: the auction ends at once in your favour"),
            // (NexiconDK: on Bus Company Online's server its clock runs for everybody: no step)
            "plus15": (!c.online.on_server).then(|| tr("+15 min")), "to_close": (!c.online.on_server).then(|| tr("To its close")),
        })
    } else if t.open() {
        json!({ "kind": "wait", "id": t.id, "label": (!c.online.on_server).then(|| tr("Simulate until it opens")) })
    } else {
        match &t.outcome {
            Some(Outcome::Lost { winner, amount, .. } | Outcome::NoBid { winner, amount }) if !winner.is_empty() => json!({ "kind": "said", "text": trf(if t.scored() { "%{who} won the line with an ask of %{amount}." } else { "%{who} won the line for %{amount}." }, &[("who", winner), ("amount", &money(t, *amount))]), "tone": "soft" }),
            Some(Outcome::Won { .. }) => {
                let text = match cn::of_line(c, &t.line) {
                    Some(h) if dates::between(&c.date, &h.from) > 0 => trf("The company runs the line from %{date}.", &[("date", &day_label(&h.from))]),
                    Some(h) => match bc::of_line(c, &h.line) {
                        Some(k) => trf("The line runs until %{date} on its contract: %{amount} a year.", &[("date", &day_label(&h.until)), ("amount", &eur(k.year_now(c.contract_index).round() as i64))]),
                        None => trf("The concession runs until %{date}.", &[("date", &day_label(&h.until))]),
                    },
                    None => String::new(),
                };
                json!({ "kind": "said", "text": text, "tone": "ok" })
            }
            _ => json!({ "kind": "none" }),
        }
    };
    // the timeline: open, bidding, award
    let stage = if !t.open() { 2 } else if now < t.opens_at { 0 } else { 1 };
    let timeline = json!([
        { "label": tr("Open"), "sub": when(c, t.opens_at), "state": if stage == 0 { "now" } else { "done" } },
        { "label": tr("Bidding"), "sub": trf("until %{time}", &[("time", &when(c, t.closes_at))]), "state": match stage { 0 => "next", 1 => "now", _ => "done" } },
        { "label": tr("Award"), "sub": match &t.outcome { Some(Outcome::Won { .. }) => tr("you won"), Some(_) => tr("decided"), None => tr("at the close") }, "state": if stage == 2 { "now" } else { "next" } },
    ]);
    // the score the company's ask would get (the slider's while it runs, else its last bid)
    let mine_amount = match actions.get("kind").and_then(Value::as_str) {
        Some("running") => actions.get("amount").and_then(Value::as_f64).map(|a| (a * 100.0).round() as i64),
        _ => mine_last,
    };
    let score = t.scored().then(|| {
        // (NexiconDK, 3 November 2026: a tender of the server's by its award - the Viborg model's
        // price 50 % and quality parts 50 %, or the lowest price)
        if let Some(award) = t.market.as_ref().map(|m| m.award) {
            return viborg_score(c, t, award, sealed_open);
        }
        // (sealed: the price points are known once the bids are opened)
        let me = mine_amount.filter(|_| !sealed_open).and_then(|a| {
            let mut with: Vec<Placed> = placed.iter().filter(|p| p.who != Who::Player).copied().collect();
            with.push(Placed { at: now.max(t.opens_at), who: Who::Player, amount: a });
            cn::standings(c, t, &with).into_iter().find(|s| s.name.is_empty())
        });
        let q = bc::quality_points(bc::scored_points(c));
        let (price, quality) = me.as_ref().map_or((0.0, q), |s| (s.price, s.quality));
        json!({
            "title": tr("How bids are scored"), "head": tr("70 % price + 30 % quality"),
            "price": price, "quality": quality, "has": me.is_some(),
            "price_label": trf("Price %{p} of 70", &[("p", &format!("{price:.1}"))]), "quality_label": trf("Quality %{q} of 30", &[("q", &format!("{quality:.1}"))]),
            "total": me.as_ref().map(|s| trf("Your expected score: %{t} of 100", &[("t", &format!("{:.1}", s.total))])).unwrap_or_else(|| tr(if sealed_open { "Your price points are known when the bids are opened: the lowest valid ask gets all 70." } else { "Set an ask to see your expected score." })),
            "lines": [
                tr("Price: the lowest ask gets all 70 points, every other ask lowest ÷ ask of them."),
                trf("Quality: your track record, 100 % less 2 % a default point - yours %{pts} points, %{q} of 30.", &[("pts", &num(bc::scored_points(c), 1)), ("q", &num(q, 1))]),
                tr(if sealed { "Sealed bids: opened together at the close. Under 85 % of the authority's cost price a bid is rejected as abnormally low; above the cap it does not count." } else { "Asks above the cap do not count; on equal totals the price decides, then quality." }),
            ],
        })
    });
    json!({
        "timeline": timeline, "score": score, "rivals": rivals,
        "work": t.market.is_some().then(|| work_tiles(c, t)),
        "rivals_title": tr(if sealed { "Opened bids" } else { "Rivals" }), "rivals_heads": [tr("Bidder"), tr("Kind"), tr("Quality"), tr("Bid")],
        "title": title, "id": t.id, "number": t.number, "caption": caption_of(t),
        "sealed": sealed_open.then(|| tr("Sealed tender: nobody sees another's bid until it closes - not the other companies, not the operators. You may change your bid until the deadline: your last counts. At the close every bid is opened and shown with its points.")),
        "running_tiles": if sealed { 3 } else { 2 },
        "state": state, "tone": tone, "mine": mine.as_ref().map(|m| &m.0), "mine_tone": mine.map(|m| m.1),
        "figures": figures, "terms": terms, "frac": frac, "times": times, "bidders": bidders, "drive": drive,
        "log_title": tr("Bids").to_uppercase(), "log": log, "no_log": tr("No bid yet."),
        "actions": actions,
        // (NexiconDK, 3 November 2026: a joint bid - a partner of the map for some of its lines,
        // H10 - while the bids are taken)
        "joint": (t.market.is_some() && t.open() && c.online.on_server).then(|| {
            let lines: Vec<Value> = match t.package.as_ref().filter(|p| p.lines.len() > 1) {
                Some(p) => p.lines.iter().map(|l| json!({ "line": l.line, "number": l.number })).collect(),
                None => vec![json!({ "line": t.line, "number": t.number })],
            };
            let partners: Vec<Value> = omsi_launcher_lib::company::store::list(&super::super::data(), "").into_iter().filter(|o| o.online.on_server && o.id != c.id && co_tenders::key_of(&o.map) == co_tenders::key_of(&c.map)).map(|o| json!({ "id": o.id, "name": o.name })).collect();
            json!({
                "title": tr("Bid together with another company"),
                "hint": tr("Your partner runs the lines you give it as your subcontractor from the contract's first day, at its price a timetable hour, if your bid wins: it accepts on its Concessions page before the award. Your bid is scored on your quality."),
                "lines": lines, "partners": partners, "button": tr("Invite"), "price_label": tr("A timetable hour"),
            })
        }).filter(|j| j["partners"].as_array().is_some_and(|a| !a.is_empty())),
        // (NexiconDK, 3 November 2026: a lost award complained of until the start - H11)
        "complain": omsi_launcher_lib::company::server_awards::may_complain(c, t).ok().map(|_| json!({
            "title": tr("Complain about the award"),
            "hint": trf("You may complain until the day before the start, %{d}: the authority looks at the bids and either lets the award stand or gives the contract to the next valid bidder.", &[("d", &day_label(&omsi_launcher_lib::company::server_awards::start_of(t)))]),
            "placeholder": tr("Why the award is wrong"),
            "button": tr("Send the complaint"),
        })),
    })
}

/// (NexiconDK, 3 November 2026) How the bids of a tender of the server's are scored, and the
/// company's quality points now by their parts (`viborg`): its price points are known once the
/// sealed bids are opened.
fn viborg_score(c: &core::company::model::Company, t: &Tender, award: Award, sealed_open: bool) -> Value {
    let parts = viborg::Parts::of(c, t);
    let split = parts.split(award);
    let quality = parts.points(award);
    let (pm, qm) = (award.price_most(), award.quality_most());
    let f = |x: f64| format!("{x:.0}");
    let g = |x: f64| format!("{x:.1}");
    // (3 November 2026: a fixed price - a rail replacement the admin ordered: every bid's)
    let mut lines = if award == Award::Fixed {
        vec![trf("The authority pays a fixed %{p} a timetable hour: every valid bid gets the %{w} price points, the quality decides.", &[("p", &eur((cn::per_hour(t, t.reference)).round() as i64)), ("w", &f(pm))])]
    } else {
        vec![trf("Price %{w} %: the lowest valid ask gets all %{w} points, every other ask lowest ÷ ask of them.", &[("w", &f(pm))])]
    };
    if award != Award::Lowest {
        lines.push(trf("Operating quality %{w} %: your track record, %{v} of 100 - %{p} points.", &[("w", &f(viborg::OPERATION)), ("v", &f(parts.operation)), ("p", &g(split[0]))]));
        lines.push(trf("Vehicles %{w} %: your buses' age and drive, %{v} of 100 - %{p} points.", &[("w", &f(viborg::VEHICLES)), ("v", &f(parts.vehicles)), ("p", &g(split[1]))]));
        lines.push(trf("Implementation plan %{w} %: room, buses and drivers for the work, %{v} of 100 - %{p} points.", &[("w", &f(viborg::PLAN)), ("v", &f(parts.plan)), ("p", &g(split[2]))]));
        lines.push(trf("Working environment %{w} %: your staff's satisfaction, less those off sick, %{v} of 100 - %{p} points.", &[("w", &f(viborg::WORK)), ("v", &f(parts.work)), ("p", &g(split[3]))]));
    }
    lines.push(tr("Sealed bids: opened together at the close. Under 85 % of the authority's cost price a bid is rejected as abnormally low; above the cap it does not count."));
    json!({
        "title": tr("How bids are scored"),
        "head": tr(match award { Award::Viborg => "The Viborg model: 50 % price + 50 % quality", Award::Lowest => "The lowest price wins", Award::Fixed => "A fixed price: the quality decides" }),
        "price": 0.0, "quality": quality, "has": false,
        "price_label": trf("Price up to %{p}", &[("p", &f(pm))]),
        "quality_label": trf("Quality %{q} of %{m}", &[("q", &g(quality)), ("m", &f(qm))]),
        "total": tr(if sealed_open { "Your price points are known when the bids are opened: the lowest valid ask gets them all." } else { "The bids are opened: their points are below." }),
        "lines": lines,
    })
}

/// (NexiconDK, 3 November 2026) How long a contract of tender `t` runs, by its work's size and the
/// buses (`concessions::server_days`).
fn lengths_text(t: &Tender) -> String {
    let hours = cn::week_hours(t);
    let size = cn::WorkSize::of_hours(hours);
    let key = match size {
        cn::WorkSize::Small => "Small work (%{h} timetable hours a week): the contract runs %{d} days with diesel buses, %{g} with CNG and %{e} with electric ones, every tour then run with an electric bus.",
        cn::WorkSize::Medium => "Medium work (%{h} timetable hours a week): the contract runs %{d} days with diesel buses, %{g} with CNG and %{e} with electric ones, every tour then run with an electric bus.",
        cn::WorkSize::Large => "Large work (%{h} timetable hours a week): the contract runs %{d} days with diesel buses, %{g} with CNG and %{e} with electric ones, every tour then run with an electric bus.",
    };
    let days = |d: Drive| cn::server_days(d, size).to_string();
    trf(key, &[("h", &grouped(hours.round())), ("d", &days(Drive::Diesel)), ("g", &days(Drive::Cng)), ("e", &days(Drive::Electric))])
}

/// (NexiconDK, 3 November 2026) What kind of operator a rival is, as the pages name it: a
/// competitor of the admin's its size and style (`competitors`), else its character.
fn kind_of(r: &cn::Rival) -> String {
    r.kinds().iter().map(|k| tr(k)).collect::<Vec<_>>().join(" · ")
}

pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Answer> {
    let id = u32_arg(args, "id");
    Some(match what {
        "tender_pick" => {
            more(s).tenders.selected = Some(id);
            Ok(json!({}))
        }
        "tender_bid" => {
            if !may_step(s) {
                return Some(Ok(json!({ "popup": busy(s) })));
            }
            let amount = args.get("amount").and_then(Value::as_i64).unwrap_or(0);
            // (3 November 2026: and a contract bus a day - a bid in two parts)
            let bus_day = args.get("bus_day").and_then(Value::as_i64).unwrap_or(0).max(0);
            // (NexiconDK: an ask by the timetable hour on the server - a year's of the tender's hours)
            let amount = match s.company.as_ref().and_then(|c| c.concessions.tenders.iter().find(|t| t.id == id)).filter(|t| t.market.is_some() && args.get("per_hour").and_then(Value::as_bool) == Some(true)) {
                Some(t) => cn::ask_of(t, amount as f64, if t.market.as_ref().is_some_and(|m| m.bus_cap > 0) { bus_day } else { 0 }),
                None => amount,
            };
            // (the least bid in the company's currency: the rules' own words say euros)
            if amount < omsi_launcher_lib::company::auction::ROUND && s.company.is_some() {
                let least = trf("Ask at least %{amount}.", &[("amount", &eur(omsi_launcher_lib::company::auction::ROUND))]);
                return Some(Ok(json!({ "popup": super::super::kit::Popup::new("error", "This cannot be done", least, "", None).json(false) })));
            }
            // (NexiconDK: a tender of the map's market on the server - the bid goes to every company)
            let data = super::super::data();
            match apply(s, |c| if c.online.on_server { co_tenders::bid_parts(&data, c, id, amount, bus_day) } else { cn::bid(c, id, amount) }) {
                Ok(_) => {
                    s.say(tr("Your bid is in. The rivals answer as the clock goes on."), false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        // (NexiconDK, 3 November 2026: subcontracts and joint bids - H10)
        "sub_propose" | "sub_answer" | "sub_end" | "joint_propose" => {
            use omsi_launcher_lib::company::subcontracts::{self as sc, Ask, AskKind, Joint};
            let s_ = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").trim().to_string();
            let lines: Vec<String> = args.get("lines").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();
            let hour = args.get("hour").and_then(Value::as_i64).unwrap_or(0);
            let yes = args.get("yes").and_then(Value::as_bool).unwrap_or(false);
            let sid = args.get("sub_id").and_then(Value::as_u64).unwrap_or(0) as u32;
            let data = super::super::data();
            let joint = (what == "joint_propose").then(|| s.company.as_ref().and_then(|c| c.concessions.tenders.iter().find(|t| t.id == id)).and_then(|t| t.market.as_ref().map(|m| Joint { key: m.key.clone(), tender: m.id, number: t.number.clone() }))).flatten();
            if what == "joint_propose" && joint.is_none() {
                return Some(Ok(json!({ "popup": super::super::kit::Popup::new("error", "This cannot be done", tr("Only a tender of the server's market takes a joint bid."), "", None).json(false) })));
            }
            let a = match what {
                "sub_propose" | "joint_propose" => Ask { kind: AskKind::Propose, sub: s_("sub"), lines, hour, from: s_("from"), joint, ..Ask::default() },
                "sub_answer" => Ask { kind: if yes { AskKind::Accept } else { AskKind::Decline }, id: sid, ..Ask::default() },
                _ => Ask { kind: AskKind::End, id: sid, ..Ask::default() },
            };
            match apply(s, |c| sc::ask(&data, c, &a)) {
                Ok(_) => {
                    s.say(tr("Sent to the server: it is taken at its next step."), false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        // (NexiconDK, 3 November 2026: a complaint about a lost award - H11)
        "tender_complain" => {
            let reason = args.get("reason").and_then(Value::as_str).unwrap_or("").to_string();
            let data = super::super::data();
            match apply(s, |c| omsi_launcher_lib::company::server_awards::complain(&data, c, id, &reason)) {
                Ok(_) => {
                    s.say(tr("Your complaint is with the authority."), false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        // (NexiconDK, 3 November 2026: a change of a contract declined while it may)
        "change_decline" => {
            let id = args.get("change").and_then(Value::as_u64).unwrap_or(0) as u32;
            let data = super::super::data();
            match apply(s, |c| omsi_launcher_lib::company::contract_events::decline(&data, c, id)) {
                Ok(_) => {
                    s.say(tr("The change is declined."), false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        // (NexiconDK, 3 November 2026: an extra bus the authority ordered - run it or not)
        "doubling_yes" | "doubling_no" => {
            let order = args.get("order").and_then(Value::as_u64).unwrap_or(0) as u32;
            match apply(s, |c| omsi_launcher_lib::company::doubling::answer(c, order, what == "doubling_yes")) {
                Ok(_) => {
                    s.say(tr(if what == "doubling_yes" { "The extra bus is on your duty board on its days." } else { "No extra bus: its full departures count as complaints against the quality bonus." }), false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        // (NexiconDK: the buses offered with a line won from another company - all or none)
        "takeover_yes" | "takeover_no" => {
            let offer = args.get("offer").and_then(Value::as_str).unwrap_or("").to_string();
            let data = super::super::data();
            match apply(s, |c| omsi_launcher_lib::company::takeover::answer(&data, c, &offer, what == "takeover_yes")) {
                Ok(_) => {
                    s.say(tr(if what == "takeover_yes" { "The buses are yours on the day your term begins." } else { "The buses stay with their company." }), false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        // (NexiconDK: the buses the company bids with on a tender of the map's market)
        "tender_drive" => {
            let drive = args.get("drive").and_then(Value::as_str).and_then(omsi_launcher_lib::company::server_buses::drive_of_key).unwrap_or(Drive::Diesel);
            match apply(s, |c| cn::set_drive(c, id, drive)) {
                Ok(_) => Ok(json!({})),
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        "tender_buy" => {
            if !may_step(s) {
                return Some(Ok(json!({ "popup": busy(s) })));
            }
            let number = s.company.as_ref().and_then(|c| c.concessions.tenders.iter().find(|t| t.id == id)).map(|t| t.number.clone()).unwrap_or_default();
            match apply(s, |c| cn::buy_out(c, id)) {
                Ok(_) => {
                    s.say(trf("Line %{n} is yours: plan its tours and start its service on the Planning page.", &[("n", &number)]), false);
                    Ok(json!({}))
                }
                Err(p) => Ok(json!({ "popup": p })),
            }
        }
        // the clock stepped for the auction: +15 min, to its close, until it opens
        "tender_step" => {
            if !may_step(s) {
                return Some(Ok(json!({ "popup": busy(s) })));
            }
            let Some(c) = s.company.clone() else { return Some(Ok(json!({}))) };
            let t = c.concessions.tenders.iter().find(|t| t.id == id);
            let to = match (args.get("to").and_then(Value::as_str).unwrap_or(""), t) {
                ("close", Some(t)) => t.closes_at,
                ("open", Some(t)) => t.opens_at,
                _ => ck::target(&c, Step::Minutes(15)),
            };
            simulate(s, to, false);
            Ok(json!({}))
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (NexiconDK, 3 November 2026) A tender of the map's market says in its contract's
    /// requirements when the term ends and when the line is out to tender again - by the work's
    /// size (3 November 2026: a small one 14 days with diesel buses, 21 with electric ones), the
    /// next term open from five days before it to two days before; a special service's, its
    /// event's last day.
    #[test]
    fn the_requirements_say_when_the_term_ends_and_the_next_tender() {
        let mut c = omsi_launcher_lib::company::found(&omsi_launcher_lib::company::Founding { name: "Umove".into(), date: "2024-03-04".into(), ..Default::default() }, "x");
        c.online.fines = Some(Default::default());
        let mut t: Tender = serde_json::from_value(json!({
            "id": 1, "line": "5", "number": "5", "caption": "", "offered": "2024-03-04", "closes": "2024-03-07",
            "weeks": 2, "day_tours": 4, "day_km": 300.0,
            "market": { "key": "grundorf", "id": 1, "from": "2024-03-10" },
        }))
        .unwrap();
        let rows = |t: &Tender| -> Vec<(String, String)> {
            let r = requirements(&c, t, None);
            // (3 November 2026: the work first, then the buses, the tender and the fines)
            serde_json::from_value(r["groups"][2]["rows"].clone()).unwrap()
        };
        let got = rows(&t);
        let of = |label: &str| got.iter().find(|(k, _)| k == label).map(|x| x.1.clone()).unwrap_or_default();
        // diesel: 10 to 23 March, the next term from the 24th - out from the 19th, decided the 22nd
        let diesel = of("Ends with diesel buses");
        assert!(diesel.contains("23 Mar 2024") && diesel.contains("19 Mar 2024") && diesel.contains("22 Mar 2024"), "{diesel}");
        // electric: 10 to 30 March, the next from the 31st - out from the 26th, decided the 29th
        let electric = of("Ends with electric buses");
        assert!(electric.contains("30 Mar 2024") && electric.contains("26 Mar 2024") && electric.contains("29 Mar 2024"), "{electric}");
        // (the work's size and its days)
        let work: Vec<(String, String)> = serde_json::from_value(requirements(&c, &t, None)["groups"][0]["rows"].clone()).unwrap();
        assert!(work.iter().any(|(k, v)| k == "Buses" && v.contains("contract buses")), "{work:?}");
        let buses: Vec<(String, String)> = serde_json::from_value(requirements(&c, &t, None)["groups"][1]["rows"].clone()).unwrap();
        let sizes = buses.iter().find(|(k, _)| k == "Diesel, CNG or electric").map(|x| x.1.clone()).unwrap_or_default();
        assert!(sizes.contains("14 days") && sizes.contains("17 with CNG") && sizes.contains("21 with electric"), "{sizes}");
        // (3 November 2026: CNG ends between them)
        let cng = of("Ends with CNG buses");
        assert!(cng.contains("26 Mar 2024"), "{cng}");
        // (no requirements set: none shown; set: each against the company's buses - it has none)
        assert!(!buses.iter().any(|(k, _)| k == "Floor"));
        if let Some(m) = t.market.as_mut() {
            m.needs = Some(omsi_launcher_lib::company::server_buses::Needs { screens: true, ..Default::default() });
        }
        let buses: Vec<(String, String)> = serde_json::from_value(requirements(&c, &t, None)["groups"][1]["rows"].clone()).unwrap();
        let floor = buses.iter().find(|(k, _)| k == "Floor").map(|x| x.1.clone()).unwrap_or_default();
        assert!(floor.starts_with("Low floor or low entry") && floor.contains("you have no buses yet"), "{floor}");
        assert!(buses.iter().any(|(k, v)| k == "Age" && v.contains("12 years")) && buses.iter().any(|(k, _)| k == "Passenger information"), "{buses:?}");
        if let Some(m) = t.market.as_mut() {
            m.needs = None;
        }
        assert_eq!(got.iter().position(|(k, _)| k == "Ends with diesel buses"), Some(2), "after the term's first day");
        // a special service: its event's last day
        if let Some(m) = t.market.as_mut() {
            m.event = "Match day".into();
            m.until = "2024-03-12".into();
        }
        let got = rows(&t);
        assert!(got.iter().any(|(k, v)| k == "The term ends" && v.contains("12 Mar 2024")));
        assert!(got.iter().any(|(k, v)| k == "Out to tender again" && v == co_tenders::EVENT_LINE));
        assert!(!got.iter().any(|(k, _)| k == "Ends with diesel buses"));
    }
}

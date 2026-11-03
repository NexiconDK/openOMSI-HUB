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

/// The company's own bid on a tender in words and its tone (`concessions::my_bid_text`).
pub fn my_bid_text(c: &Company, t: &Tender) -> Option<(String, &'static str)> {
    let b = cn::my_bid(c, t, ck::now(c))?;
    let bid = trf("Your bid: %{amount} at %{when}", &[("amount", &money(t, b.amount)), ("when", &when(c, b.at))]);
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
        "results": results(&c),
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
        row("Diesel or electric", tr("Bid with diesel buses: the contract runs two weeks. Bid with electric buses: it runs a month, and every tour is run with an electric bus.")),
    ];
    let from = t.market.as_ref().map(|m| m.from.clone()).filter(|f| !f.is_empty()).unwrap_or_else(|| dates::add(&ck::date_of(t.closes_at), 1));
    // (NexiconDK, 3 November 2026: the term's last day and the next tender's days - by the buses
    // the contract is won with; a special service's: its event's last day, out again with its next)
    let event_until = t.market.as_ref().filter(|m| !m.event.is_empty()).map(|m| m.until.clone()).filter(|u| dates::parse(u).is_some());
    let ends: Vec<Value> = match event_until {
        Some(until) => vec![row("The term ends", trf("%{date}, the event's last day", &[("date", &day_label(&until))])), row("Out to tender again", tr(co_tenders::EVENT_LINE))],
        None => [(Drive::Diesel, "Ends with diesel buses"), (Drive::Electric, "Ends with electric buses")]
            .into_iter()
            .map(|(d, label)| {
                let end = cn::server_term(d, &from);
                let next = dates::add(&end, 1);
                let (open, close) = (dates::add(&next, -co_tenders::RENEW_OPENS), dates::add(&next, -co_tenders::RENEW_DECIDED));
                row(label, trf("%{end} · out to tender again from %{open} to %{close}", &[("end", &day_label(&end)), ("open", &day_label(&open)), ("close", &day_label(&close))]))
            })
            .collect(),
    };
    let mut tender = vec![
        row("Bidding", trf("Three days: from %{open} to %{close}", &[("open", &when(c, t.opens_at)), ("close", &when(c, t.closes_at))])),
        row("The term", trf("From %{date}, without extension", &[("date", &day_label(&from))])),
        row("Renewal", tr("Six days before the term ends the line goes out to tender again, decided three days before it: the winner has three days to make ready.")),
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
        row("Leaving a stop early", trf("%{amount} a departure more than a minute early", &[("amount", &kr(f.early))])),
        row("Leaving a stop late", trf("%{amount} a departure more than 15 minutes late, unless the delay was reported to the control room: a player reports his on his duty, the dispatcher those of the company's drivers", &[("amount", &kr(f.late))])),
        row("A complaint about the driving", kr(f.driving)),
        row("A complaint about the temperature", kr(f.temperature)),
        row("A complaint about the route", kr(f.route)),
        row("A ticket sold wrong", kr(f.ticket)),
        row("A red light run", kr(f.red_light)),
    ];
    json!({
        "title": trf("Contract requirements: line %{n}", &[("n", &t.number)]),
        "groups": [
            { "caps": tr("The buses").to_uppercase(), "rows": buses },
            { "caps": tr("The tender").to_uppercase(), "rows": tender },
            { "caps": tr("The authority's fines").to_uppercase(), "rows": fines },
        ],
        "close": tr("Close"),
    })
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
            if t.open() { json!({ "label": tr("Deadline").to_uppercase(), "value": when(c, t.closes_at), "tone": "text" }) } else if let Some((_, days)) = event_of(t) { json!({ "label": tr("Term").to_uppercase(), "value": days, "tone": "text" }) } else { json!({ "label": tr("Term").to_uppercase(), "value": tr(if t.drive == Drive::Electric { "Electric · a month" } else { "Diesel · two weeks" }), "tone": "text" }) },
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
    let drive = (t.market.is_some() && t.open()).then(|| {
        json!({
            "label": tr("Your buses"), "value": if t.drive == Drive::Electric { "electric" } else { "diesel" },
            "options": if event_of(t).is_some() { json!([["diesel", tr("Diesel buses")], ["electric", tr("Electric buses")]]) } else { json!([["diesel", tr("Diesel · two weeks")], ["electric", tr("Electric · a month")]]) },
            "say": tr(if event_of(t).is_some() { "Your next bid is made with these buses: a special service runs its event's days whichever you choose; with electric buses its tours are run with electric ones." } else { "Your next bid is made with these buses: the contract runs two weeks with diesel buses and a month with electric ones, whose tours are then run with electric buses." }),
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
            let kind = points_of(&r.name).unwrap_or_else(|| format!("{}  ·  {}", tr(r.character.label()), trf("quality %{q}", &[("q", &format!("{:.0}", r.quality))])));
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
        let operator = |name: &str| t.market.as_ref().and_then(|m| m.rivals.iter().find(|r| r.name == name)).map(|r| tr(r.character.label()));
        t.standings
            .iter()
            .enumerate()
            .map(|(k, s)| {
                let name = if s.name.is_empty() { tr("You") } else { s.name.clone() };
                let kind = if s.name.is_empty() { tr("your company") } else { operator(&s.name).unwrap_or_else(|| tr("a company of the server")) };
                let q = cn::quality_share(t, s.quality);
                let (bid, tone) = if s.rejected { (trf("%{amount}, rejected", &[("amount", &money(t, s.amount))]), "danger") } else if k == 0 { (trf("%{amount}, won", &[("amount", &money(t, s.amount))]), "ok") } else { (money(t, s.amount), "text") };
                let points = if s.rejected { tr("rejected: abnormally low or above the cap") } else { trf("%{p} price + %{q} quality = %{t} points", &[("p", &format!("{:.1}", s.price)), ("q", &format!("{:.1}", s.quality)), ("t", &format!("{:.1}", s.total))]) };
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
            Some(json!({ "name": r.name, "kind": tr(r.character.label()), "quality": r.quality / 100.0, "quality_text": format!("{:.0}", r.quality), "bid": bid, "tone": tone, "leads": leads, "points": points_of(&r.name) }))
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
        let unit = |year: f64| if hourly { cn::per_hour(t, year.round() as Cents) / 100.0 } else { year / 100.0 };
        let year_of = |a: f64| if hourly { cn::of_hour(t, a * 100.0) } else { (a * 100.0).round() as Cents };
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
            let from = t.market.as_ref().map(|m| m.from.clone()).filter(|f| !f.is_empty()).unwrap_or_else(|| dates::add(&ck::date_of(t.closes_at), 1));
            // (NexiconDK, 3 November 2026: a special service runs its event's days alone - it said
            // the contract's 14 days for a match day)
            let until = t.market.as_ref().filter(|m| !m.event.is_empty()).map(|m| m.until.clone()).filter(|u| dates::parse(u).is_some()).unwrap_or_else(|| cn::server_term(t.drive, &from));
            let days = dates::between(&from, &until) + 1;
            let hours = cn::hours_year(t) / 365.0 * days as f64;
            let rate = cn::per_hour(t, amount);
            trf("About %{week} a week and %{term} for the contract's %{days} days: %{hours} timetable hours at %{rate}", &[("week", &eur((rate * cn::hours_year(t) / 52.0).round() as Cents)), ("term", &eur((rate * hours).round() as Cents)), ("days", &days.to_string()), ("hours", &format!("{hours:.0}")), ("rate", &money(t, amount))])
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
            "chance": said, "chance_tone": ctone,
            "recommend": if sealed && advice < rec { Some(trf("Your cost price with a 5 % margin is %{own}: under the abnormally low line, so ask at least %{amount}", &[("own", &money(t, advice)), ("amount", &money(t, rec))])) } else if sealed { Some(trf("Your cost price with a 5 % margin: %{amount}", &[("amount", &money(t, rec))])) } else { normal.then(|| trf("Recommended: %{amount} (%{s} % of the cap), about %{p} % to win", &[("amount", &money(t, rec)), ("s", &format!("{:.0}", t.share(rec) * 100.0)), ("p", &format!("{:.0}", rec_chance * 100.0))])) },
            "recommend_amount": unit(rec as f64),
            "bid": if sealed && !t.offers.is_empty() { trf("Change your bid to %{amount}", &[("amount", &money(t, amount))]) } else { trf("Ask %{amount}", &[("amount", &money(t, amount))]) },
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
        "rivals_title": tr(if sealed { "Opened bids" } else { "Rivals" }), "rivals_heads": [tr("Bidder"), tr("Kind"), tr("Quality"), tr("Bid")],
        "title": title, "id": t.id, "number": t.number, "caption": caption_of(t),
        "sealed": sealed_open.then(|| tr("Sealed tender: nobody sees another's bid until it closes - not the other companies, not the operators. You may change your bid until the deadline: your last counts. At the close every bid is opened and shown with its points.")),
        "running_tiles": if sealed { 3 } else { 2 },
        "state": state, "tone": tone, "mine": mine.as_ref().map(|m| &m.0), "mine_tone": mine.map(|m| m.1),
        "figures": figures, "terms": terms, "frac": frac, "times": times, "bidders": bidders, "drive": drive,
        "log_title": tr("Bids").to_uppercase(), "log": log, "no_log": tr("No bid yet."),
        "actions": actions,
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
    let mut lines = vec![trf("Price %{w} %: the lowest valid ask gets all %{w} points, every other ask lowest ÷ ask of them.", &[("w", &f(pm))])];
    if award == Award::Viborg {
        lines.push(trf("Operating quality %{w} %: your track record, %{v} of 100 - %{p} points.", &[("w", &f(viborg::OPERATION)), ("v", &f(parts.operation)), ("p", &g(split[0]))]));
        lines.push(trf("Vehicles %{w} %: your buses' age and drive, %{v} of 100 - %{p} points.", &[("w", &f(viborg::VEHICLES)), ("v", &f(parts.vehicles)), ("p", &g(split[1]))]));
        lines.push(trf("Implementation plan %{w} %: room, buses and drivers for the work, %{v} of 100 - %{p} points.", &[("w", &f(viborg::PLAN)), ("v", &f(parts.plan)), ("p", &g(split[2]))]));
        lines.push(trf("Working environment %{w} %: your staff's satisfaction, less those off sick, %{v} of 100 - %{p} points.", &[("w", &f(viborg::WORK)), ("v", &f(parts.work)), ("p", &g(split[3]))]));
    }
    lines.push(tr("Sealed bids: opened together at the close. Under 85 % of the authority's cost price a bid is rejected as abnormally low; above the cap it does not count."));
    json!({
        "title": tr("How bids are scored"),
        "head": tr(match award { Award::Viborg => "The Viborg model: 50 % price + 50 % quality", Award::Lowest => "The lowest price wins" }),
        "price": 0.0, "quality": quality, "has": false,
        "price_label": trf("Price up to %{p}", &[("p", &f(pm))]),
        "quality_label": trf("Quality %{q} of %{m}", &[("q", &g(quality)), ("m", &f(qm))]),
        "total": tr(if sealed_open { "Your price points are known when the bids are opened: the lowest valid ask gets them all." } else { "The bids are opened: their points are below." }),
        "lines": lines,
    })
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
            // (NexiconDK: an ask by the timetable hour on the server - a year's of the tender's hours)
            let amount = match s.company.as_ref().and_then(|c| c.concessions.tenders.iter().find(|t| t.id == id)).filter(|t| t.market.is_some() && args.get("per_hour").and_then(Value::as_bool) == Some(true)) {
                Some(t) => cn::of_hour(t, amount as f64),
                None => amount,
            };
            // (the least bid in the company's currency: the rules' own words say euros)
            if amount < omsi_launcher_lib::company::auction::ROUND && s.company.is_some() {
                let least = trf("Ask at least %{amount}.", &[("amount", &eur(omsi_launcher_lib::company::auction::ROUND))]);
                return Some(Ok(json!({ "popup": super::super::kit::Popup::new("error", "This cannot be done", least, "", None).json(false) })));
            }
            // (NexiconDK: a tender of the map's market on the server - the bid goes to every company)
            let data = super::super::data();
            match apply(s, |c| if c.online.on_server { co_tenders::bid(&data, c, id, amount) } else { cn::bid(c, id, amount) }) {
                Ok(_) => {
                    s.say(tr("Your bid is in. The rivals answer as the clock goes on."), false);
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
            let drive = if args.get("drive").and_then(Value::as_str) == Some("electric") { Drive::Electric } else { Drive::Diesel };
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
    /// requirements when the term ends and when the line is out to tender again - with diesel
    /// buses two weeks, with electric ones a month; a special service's, its event's last day.
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
            serde_json::from_value(r["groups"][1]["rows"].clone()).unwrap()
        };
        let got = rows(&t);
        let of = |label: &str| got.iter().find(|(k, _)| k == label).map(|x| x.1.clone()).unwrap_or_default();
        // diesel: 10 to 23 March, the next term from the 24th - out from the 18th, decided the 21st
        let diesel = of("Ends with diesel buses");
        assert!(diesel.contains("23 Mar 2024") && diesel.contains("18 Mar 2024") && diesel.contains("21 Mar 2024"), "{diesel}");
        // electric: 10 March to 9 April, the next from the 10th - out from the 4th, decided the 7th
        let electric = of("Ends with electric buses");
        assert!(electric.contains("9 Apr 2024") && electric.contains("4 Apr 2024") && electric.contains("7 Apr 2024"), "{electric}");
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

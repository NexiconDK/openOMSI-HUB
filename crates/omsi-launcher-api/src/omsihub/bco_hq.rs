//! (NexiconDK) Bus Company Online, phase 7 (docs/omsihub/BUS_COMPANY_ONLINE.md): a group's head
//! office in the launcher - `bco.js`'s head office page; the server's side is
//! `omsi_launcher_lib::company::server_hq`, asked through the door as the server page's questions
//! are (`bco::question`).
//!
//! * `bco_hq {group}` - the head office's page: the group's money (its treasury, the whole
//!   group's, the month, the debt), its depots, the requests waiting, its news, its books, its
//!   loans, the depots' rules and the founding of a depot;
//! * `bco_hq_do {group, act, args}` - its owner's act (`server_hq::act`, `found`), the money he
//!   typed in the group's currency (`MONEY`) sent in cents: the page again, or `{error}`.

use super::bco::{question, words_of};
use super::company::inbox;
use super::company::words::{self, day_label, eur, tr, trf};
use crate::Answer;
use omsi_launcher_lib::company::currency::Currency;
use omsi_launcher_lib::company::model::GameMode;
use serde_json::{json, Value};
use std::sync::Mutex;

/// The head office's page as the server sent it last: what the owner types is in its currency.
static LAST: Mutex<Option<Value>> = Mutex::new(None);

/// The arguments of an act that are money, typed in whole units of the group's currency.
const MONEY: [&str; 4] = ["amount", "sweep_over", "top_up_below", "start"];

/// The group's currency (the euro when it is not known yet).
fn currency_of(h: &Value) -> Currency {
    serde_json::from_value(h.get("currency").cloned().unwrap_or(Value::Null)).unwrap_or_default()
}

/// What the owner typed as cents in `cur`: a number, or text as he writes it ("250 000",
/// "£ 1,200"); nothing typed: none (a rule's way off).
fn cents_of(v: &Value, cur: Currency) -> Value {
    let units = match v {
        Value::Number(n) => n.as_f64(),
        Value::String(t) if !t.trim().is_empty() => omsi_launcher_lib::company::currency::parse(t),
        _ => None,
    };
    // (nothing below nothing, nor over what any amount on the server could be)
    units.filter(|u| u.is_finite()).map_or(Value::Null, |u| json!(cur.cents(u.clamp(0.0, 1e11))))
}

/// A command of the head office's page, or None when `cmd` is not one.
pub fn call(cmd: &str, args: &Value) -> Option<Answer> {
    words::set_language(args.get("lang").and_then(Value::as_str).unwrap_or(""));
    if !super::bco::enabled() {
        return Some(Ok(json!({ "off": true })));
    }
    let group =args.get("group").and_then(Value::as_str).unwrap_or("").to_string();
    Some(Ok(match cmd {
        "bco_hq" => match question("hq", json!({ "group": group })) {
            Ok(v) => shown(v),
            Err((e, a)) => json!({ "error": words_of(&e, &a) }),
        },
        "bco_hq_do" => {
            let act = args.get("act").and_then(Value::as_str).unwrap_or("").to_string();
            let cur = LAST.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(currency_of).unwrap_or_default();
            let mut sent = args.get("args").cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
            for k in MONEY {
                if let Some(v) = sent.get(k).cloned() {
                    sent[k] = cents_of(&v, cur);
                }
            }
            match question("hq_do", json!({ "group": group, "act": act, "args": sent })) {
                Ok(v) => shown(v),
                Err((e, a)) => json!({ "error": words_of(&e, &a) }),
            }
        }
        _ => return None,
    }))
}

/// The server's answer kept and shown.
fn shown(v: Value) -> Value {
    let h = v.get("hq").cloned().unwrap_or(Value::Null);
    *LAST.lock().unwrap_or_else(|e| e.into_inner()) = Some(h.clone());
    let cur = currency_of(&h);
    // (its money in the group's currency, the thread's own afterwards)
    let was = words::currency();
    words::set_currency(cur);
    let out = view(&h, cur);
    words::set_currency(was);
    out
}

fn tone(c: i64) -> &'static str {
    if c < 0 {
        "danger"
    } else {
        "ok"
    }
}

/// A month of the group's books: income, expenses, result and its kinds.
fn month_rows(m: &Value) -> Value {
    let n = |k: &str| m.get(k).and_then(Value::as_i64).unwrap_or(0);
    let kinds: Vec<Value> = m.get("kinds").and_then(Value::as_array).cloned().unwrap_or_default().iter().map(|k| {
        let a = k.get("amount").and_then(Value::as_i64).unwrap_or(0);
        json!({ "label": tr(k.get("kind").and_then(Value::as_str).unwrap_or("")), "value": eur(a), "tone": tone(a) })
    }).collect();
    json!({
        "label": words::month_label(m.get("month").and_then(Value::as_str).unwrap_or("")),
        "income": eur(n("income")), "expenses": eur(-n("expenses")), "result": eur(n("result")), "tone": tone(n("result")),
        "kinds": kinds,
    })
}

/// The page: its words, the group's figures, its depots, and what its owner may do.
fn view(h: &Value, cur: Currency) -> Value {
    let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_i64).unwrap_or(0);
    let s = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let units = |c: i64| (cur.units(c as f64)).round() as i64;
    let depots_raw = h.get("depots").and_then(Value::as_array).cloned().unwrap_or_default();
    let month = &h["month"];
    let last = &h["last_month"];
    let credit = &h["credit"];
    let mode: GameMode = serde_json::from_value(h.get("mode").cloned().unwrap_or(Value::Null)).unwrap_or_default();
    let depots: Vec<Value> = depots_raw
        .iter()
        .map(|d| {
            let rule = &d["rule"];
            let (over, below) = (rule.get("sweep_over").and_then(Value::as_i64), rule.get("top_up_below").and_then(Value::as_i64));
            let mut says = Vec::new();
            if let Some(o) = over {
                says.push(trf("Over %{amount} goes home at the day's close", &[("amount", &eur(o))]));
            }
            if let Some(b) = below {
                says.push(trf("Topped up below %{amount}", &[("amount", &eur(b))]));
            }
            let red = n(d, "red_days");
            let pending = n(d, "pending");
            let runs = d.get("runs").and_then(Value::as_bool).unwrap_or(false);
            json!({
                "id": s(d, "id"), "name": s(d, "name"), "map": s(d, "map"), "short": s(d, "short"), "colours": d.get("colours").cloned().unwrap_or(json!([])),
                "cash": eur(n(d, "cash")), "cash_tone": tone(n(d, "cash")),
                "result": trf("this month %{amount}", &[("amount", &eur(n(d, "result")))]), "result_tone": tone(n(d, "result")),
                "facts": trf("%{lines} lines · %{buses} buses · %{staff} staff", &[("lines", &n(d, "lines").to_string()), ("buses", &n(d, "buses").to_string()), ("staff", &n(d, "staff").to_string())]),
                "runs": runs, "state": if runs { tr("Open") } else { tr("Starting on the server…") },
                "red": (red > 0).then(|| trf("%{n} days closed in the red", &[("n", &red.to_string())])),
                "pending": (pending > 0).then(|| trf("On its way to it: %{amount}", &[("amount", &eur(pending))])),
                "rule": if says.is_empty() { tr("No rule: money moves by hand.") } else { says.join(" · ") },
                "sweep_over": over.map(units), "top_up_below": below.map(units),
            })
        })
        .collect();
    let inbox: Vec<Value> = h
        .get("inbox")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let args: Vec<(String, String)> = serde_json::from_value(m.get("args").cloned().unwrap_or(Value::Null)).unwrap_or_default();
            let level = s(m, "level");
            json!({ "date": day_label(&s(m, "date")), "title": inbox::words(&s(m, "title"), &args), "text": inbox::words(&s(m, "text"), &args), "tone": match level.as_str() { "bad" => "danger", "warn" => "warn", "good" => "ok", _ => "text" } })
        })
        .collect();
    let asks: Vec<Value> = h
        .get("asks")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|a| json!({ "id": a["id"], "text": trf("%{name} asks to spend up to %{amount}: %{reason}", &[("name", &s(a, "name")), ("amount", &eur(n(a, "amount"))), ("reason", &s(a, "reason"))]) }))
        .collect();
    let loans: Vec<Value> = h
        .get("loans")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|l| {
            json!({
                "id": l["id"], "left": units(n(l, "remaining")),
                "text": trf("%{purpose}: %{amount} left, %{monthly} a month for %{n} months", &[("purpose", &tr(&s(l, "purpose"))), ("amount", &eur(n(l, "remaining"))), ("monthly", &eur(n(l, "monthly"))), ("n", &n(l, "months_left").to_string())]),
            })
        })
        .collect();
    let ledger: Vec<Value> = h
        .get("ledger")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|b| json!({ "date": day_label(&s(b, "date")), "kind": tr(&s(b, "kind")), "text": s(b, "text"), "amount": eur(n(b, "amount")), "tone": tone(n(b, "amount")) }))
        .collect();
    // (step 3: each depot's buses and staff, what is on its way, the orders waiting)
    let fleet: Vec<Value> = h
        .get("fleet")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|d| {
            let arr = |k: &str| d.get(k).and_then(Value::as_array).cloned().unwrap_or_default();
            let buses: Vec<Value> = arr("buses")
                .iter()
                .map(|v| {
                    let owned = v.get("owned").and_then(Value::as_bool).unwrap_or(false);
                    let note = if !owned {
                        tr("Leased or rented: it stays")
                    } else if v.get("workshop").and_then(Value::as_bool) == Some(true) {
                        tr("In the workshop")
                    } else {
                        String::new()
                    };
                    json!({ "id": v["id"], "label": format!("{} {}", s(v, "number"), s(v, "name")), "kind": tr(&s(v, "kind")), "owned": owned, "note": note })
                })
                .collect();
            let staff: Vec<Value> = arr("staff")
                .iter()
                .map(|e| {
                    let note = if let Some(l) = e.get("lent").filter(|x| !x.is_null()) {
                        trf("Lent by %{other} until %{day}", &[("other", &s(l, "from")), ("day", &day_label(&s(l, "until")))])
                    } else if let Some(l) = e.get("lend").filter(|x| !x.is_null()) {
                        trf("Goes to %{other} tomorrow for %{n} days", &[("other", &s(l, "to")), ("n", &n(l, "days").to_string())])
                    } else {
                        String::new()
                    };
                    json!({ "id": e["id"], "label": s(e, "name"), "role": tr(&s(e, "role")), "free": e.get("free").and_then(Value::as_bool).unwrap_or(false), "note": note })
                })
                .collect();
            let moving: Vec<String> = arr("on_way").iter().map(|x| trf("%{bus} to %{other}, there on %{day}", &[("bus", &s(x, "bus")), ("other", &s(x, "to")), ("day", &day_label(&s(x, "day")))])).collect();
            let orders: Vec<String> = arr("orders")
                .iter()
                .map(|o| match s(o, "do").as_str() {
                    "move_bus" => trf("Ordered: %{what} to %{other}", &[("what", &s(o, "what")), ("other", &s(o, "to"))]),
                    _ => trf("Ordered: %{what} lent to %{other} for %{n} days", &[("what", &s(o, "what")), ("other", &s(o, "to")), ("n", &n(o, "days").to_string())]),
                })
                .collect();
            json!({ "id": s(d, "id"), "name": s(d, "name"), "buses": buses, "staff": staff, "moving": moving, "orders": orders })
        })
        .collect();
    // (step 4: the group's brand and liveries; step 5: its people)
    let bus_of = |file: &str| file.rsplit(['/', '\\']).next().unwrap_or(file).trim_end_matches(".bus").to_string();
    let liveries: Vec<Value> = h.get("liveries").and_then(Value::as_array).cloned().unwrap_or_default().iter().map(|l| json!({ "name": s(l, "name"), "bus": bus_of(&s(l, "bus")) })).collect();
    let depot_liveries: Vec<Value> = h
        .get("depot_liveries")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|l| json!({ "depot": s(l, "depot"), "depot_name": s(l, "depot_name"), "name": s(l, "name"), "bus": bus_of(&s(l, "bus")) }))
        .collect();
    let behind: Vec<String> = h.get("brand_behind").and_then(Value::as_array).cloned().unwrap_or_default().iter().filter_map(|x| x.as_str().map(str::to_string)).collect();
    let members: Vec<Value> = h
        .get("members")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let role = if s(m, "role") == "depot" { trf("Manager of %{depot}", &[("depot", &s(m, "depot_name"))]) } else { tr("Management") };
            json!({ "player": s(m, "player"), "name": s(m, "name"), "role": role })
        })
        .collect();
    let maps: Vec<Value> = h.get("maps").and_then(Value::as_array).cloned().unwrap_or_default().iter().map(|m| json!({ "key": m, "label": m })).collect();
    let rate = credit.get("rate").and_then(Value::as_f64).unwrap_or(0.0);
    json!({
        "id": s(h, "id"), "name": s(h, "name"), "short": s(h, "short"), "colours": h.get("colours").cloned().unwrap_or(json!([])),
        "time": format!("{} {}", day_label(&s(h, "date")), s(h, "time")),
        "under": trf("Head office · %{n} depots · %{mode}", &[("n", &depots.len().to_string()), ("mode", &tr(mode.label()))]),
        "figures": [
            { "label": tr("The treasury"), "value": eur(n(h, "cash")), "tone": tone(n(h, "cash")), "under": trf("the head office's own: %{amount} owed", &[("amount", &eur(n(h, "debt")))]) },
            { "label": tr("The whole group"), "value": eur(n(h, "group_cash")), "tone": tone(n(h, "group_cash")), "under": tr("the head office and its depots together") },
            { "label": tr("This month"), "value": eur(n(month, "result")), "tone": tone(n(month, "result")), "under": trf("last month %{amount}", &[("amount", &eur(n(last, "result")))]) },
            { "label": tr("The group's debt"), "value": eur(n(h, "group_debt")), "tone": if n(h, "group_debt") > 0 { "warn" } else { "text" }, "under": trf("the bank lends %{amount} more", &[("amount", &eur(n(credit, "left")))]) },
        ],
        "depots": depots, "asks": asks, "inbox": inbox, "loans": loans, "ledger": ledger,
        "month": month_rows(month), "last_month": month_rows(last),
        "credit": trf("The bank lends the group on its buses and what it earns: up to %{limit}, %{left} more, at %{rate} % a year.", &[("limit", &eur(n(credit, "limit"))), ("left", &eur(n(credit, "left"))), ("rate", &format!("{:.1}", rate * 100.0))]),
        "borrow_amount": units(n(credit, "smallest").max(n(credit, "left").min(100_000_00))),
        "send_amount": units(100_000_00), "start_amount": units(500_000_00),
        "maps": maps,
        "fleet": fleet,
        "brand": {
            "name": s(h, "name"), "short": s(h, "short"), "colours": h.get("colours").cloned().unwrap_or(json!(["#f28c28", "#1d2b44"])),
            "logo": h.get("logo").and_then(Value::as_bool).unwrap_or(false).then(|| tr("The group's mark has a logo picture: a depot's.")),
            "liveries": liveries, "depot_liveries": depot_liveries,
            "behind": (!behind.is_empty()).then(|| trf("Not taken yet by: %{names}", &[("names", &behind.join(", "))])),
        },
        "is_owner": h.get("is_owner").and_then(Value::as_bool).unwrap_or(false),
        "owner_line": trf("%{name} owns the group and gives its roles.", &[("name", &s(h, "owner"))]),
        "members": members,
        "players": h.get("players").cloned().unwrap_or(json!([])),
        "move_say": trf("A bus of the group's own moves to another depot for %{price} and is away %{n} days; a leased or rented bus stays with its contract. The depot does the order at its next step.", &[("price", &eur(n(h, "bus_move_price"))), ("n", &n(h, "bus_move_days").to_string())]),
        "lend_say": tr("A lent employee goes the next day and works there for the days you choose, paid by the company that borrows him, then comes back."),
        "found_say": trf("A depot runs the group on another map of the server: its own lines, buses, staff and money there. It costs %{price} of the treasury, besides the starting money you give it.", &[("price", &eur(n(h, "depot_price")))]),
        "found_none": maps.is_empty().then(|| tr("The group has a depot on every map of the server.")),
        "words": {
            "back": tr("The server's companies"), "refresh": tr("Refresh"),
            "tabs": [["overview", tr("Overview")], ["money", tr("Money and books")], ["fleet", tr("Fleet and staff")], ["brand", tr("Brand")], ["roles", tr("Roles")]],
            "brand_caps": tr("The group's name and mark"),
            "brand_say": tr("Every depot takes the group's name, short name, colours and logo within a minute, named by the group and its map."),
            "name": tr("Name"), "short": tr("Short name"), "colours": tr("Colours"),
            "mark_of_caps": tr("A depot's mark for the whole group"),
            "mark_of_say": tr("Choose the logo picture and the mark in a depot's settings, then make them the group's here."),
            "mark_of": tr("Use its mark"),
            "liveries_caps": tr("The group's liveries"),
            "liveries_say": tr("A livery a depot made in the livery studio, shared with the whole group: every depot paints its buses in it in the workshop, its design paid once."),
            "share": tr("Share with the group"), "unshare": tr("Stop sharing"), "no_liveries": tr("No liveries shared yet."), "depot_liveries": tr("The depots' own liveries"),
            "roles_caps": tr("The group's people"),
            "roles_say": tr("The management runs the head office with the owner and is a co-owner of every depot; a depot's manager is a co-owner of his depot. Each depot takes the roles within a minute."),
            "management": tr("Management"), "depot_manager": tr("Depot manager"), "remove": tr("Remove"),
            "player": tr("Player"), "role": tr("Role"), "give": tr("Give the role"), "no_members": tr("Nobody yet: the owner runs the group alone."),
            "buses": tr("Buses"), "staff": tr("Staff"), "move": tr("Move the bus"), "lend": tr("Lend"), "days": tr("Days"),
            "on_way": tr("On the way"), "orders": tr("Orders waiting for the depot"), "no_buses": tr("No buses."), "no_staff": tr("No staff."),
            "depots": tr("Depots"), "asks": tr("Waiting for your approval"), "approve": tr("Approve"), "refuse": tr("Refuse"),
            "news": tr("News"), "no_news": tr("Nothing new."),
            "books": tr("The group's books"), "income": tr("Income"), "expenses": tr("Expenses"), "result": tr("Result"),
            "send_caps": tr("Money to a depot"), "send": tr("Send money"), "to": tr("To"), "amount": tr("Amount"),
            "rules": tr("The depots' rules"),
            "rules_say": tr("At its day's close a depot sends home what it has over the amount it keeps; below the top-up the head office tops it up from the treasury, as far as the treasury goes. Leave a field empty for none."),
            "keep": tr("Keep at most"), "top_up": tr("Top up below"), "save": tr("Save"),
            "loans": tr("Loans"), "borrow": tr("Borrow"), "repay": tr("Pay back"), "no_loans": tr("The head office has no loans."),
            "found": tr("Found a depot"), "map": tr("The map"), "start": tr("Its starting money"),
            "ledger": tr("The treasury's bookings"), "no_ledger": tr("No bookings yet."),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use omsi_launcher_lib::company::{self as co, server_depots, server_hq, store, Founding};

    /// The page of a group as the server's view has it: a head office with two depots, a bus and
    /// a driver, money, a rule, a loan, a request waiting - every part the page shows in the
    /// page's words. (`BCO_HQ_MOCK=<file>` writes it there: the launcher's mock of `bco_hq`.)
    #[test]
    fn the_head_offices_page_from_the_servers_view() {
        let dir = std::env::temp_dir().join(format!("openomsi-bco-hq-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("omsi");
        for m in ["Grundorf", "Ahlheim"] {
            std::fs::create_dir_all(root.join("maps").join(m)).unwrap();
            std::fs::write(root.join("maps").join(m).join("global.cfg"), "").unwrap();
        }
        let data = dir.join("bco");
        co::online::server_clock(&data, 1_800_000_000, false, ("2024-03-04".into(), 6 * 3600)).unwrap();
        let mut c = co::found(&Founding { name: "Umove".into(), short: "UMO".into(), date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "x");
        c.id = "umove".into();
        c.online.on_server = true;
        c.online.owner = "Mathias".into();
        c.online.owner_id = "p1".into();
        c.cash += 2_000_000_00;
        store::save(&data, &c).unwrap();
        let id = server_depots::found(&data, &root, &mut c, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        c.online.big_purchase = Some(i64::MAX / 4);
        let b = co::market::MarketBus { file: "Vehicles/MB_Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
        co::market::buy_new(&mut c, &b, co::market::Payment::Cash, "").unwrap();
        let a = co::staff::applicants(&c).into_iter().find(|a| a.role == co::model::Role::Driver).expect("a driver");
        co::staff::hire(&mut c, &a).unwrap();
        store::save(&data, &c).unwrap();
        let mut d = store::load(&data, &id).unwrap();
        server_depots::ask_approval(&data, &mut d, 700_000_00, "two articulated buses", 1_800_000_000).unwrap();
        let mut g = store::load(&data, "umove.hq").unwrap();
        server_depots::send(&data, &mut c, "umove.hq", 400_000_00, 1).unwrap();
        server_hq::step(&data, &mut g, "2024-03-04", 2);
        server_hq::act(&data, &mut g, ("p1", "Mathias"), "rule", &json!({ "depot": id, "sweep_over": 300_000_00, "top_up_below": 50_000_00 }), 3).unwrap();
        server_hq::act(&data, &mut g, ("p1", "Mathias"), "borrow", &json!({ "amount": 50_000_00 }), 3).unwrap();
        server_hq::act(&data, &mut g, ("p1", "Mathias"), "role", &json!({ "player": "p2", "name": "Luc", "role": "depot", "depot": id }), 3).unwrap();
        let around = server_hq::Around { running: vec![c.id.clone()], depot_price: 250_000_00, maps: vec![], time: "06:00".into(), bus_move: (5000_00, 2), players: vec![("p1".into(), "Mathias".into()), ("p2".into(), "Luc".into())] };
        let raw = server_hq::view(&data, &g, "p1", &around);
        let page = shown(json!({ "ok": true, "hq": raw }));
        assert_eq!(page["depots"].as_array().map(Vec::len), Some(2));
        assert_eq!(page["asks"].as_array().map(Vec::len), Some(1));
        assert_eq!(page["loans"].as_array().map(Vec::len), Some(1));
        assert_eq!(page["members"].as_array().map(Vec::len), Some(1));
        assert_eq!(page["words"]["tabs"].as_array().map(Vec::len), Some(5));
        assert!(page["fleet"].as_array().is_some_and(|f| f.iter().any(|x| x["buses"].as_array().is_some_and(|b| !b.is_empty()))));
        assert!(page["depots"].as_array().unwrap().iter().any(|x| x["rule"].as_str().is_some_and(|r| r.contains("300"))));
        if let Ok(path) = std::env::var("BCO_HQ_MOCK") {
            std::fs::write(path, serde_json::to_vec_pretty(&page).unwrap()).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Money typed in the group's currency goes to the server in euro cents; nothing typed: none.
    #[test]
    fn money_typed_in_the_groups_currency() {
        assert_eq!(cents_of(&json!(250), Currency::Eur), json!(25_000));
        assert_eq!(cents_of(&json!("1 200"), Currency::Eur), json!(120_000));
        assert_eq!(cents_of(&json!(""), Currency::Eur), Value::Null);
        assert_eq!(cents_of(&Value::Null, Currency::Eur), Value::Null);
        assert_eq!(currency_of(&json!({})), Currency::Eur);
        assert_eq!(cents_of(&json!("99999999999999999999"), Currency::Eur), json!(10_000_000_000_000i64), "no overflow");
        assert_eq!(cents_of(&json!(-5), Currency::Eur), json!(0));
    }
}

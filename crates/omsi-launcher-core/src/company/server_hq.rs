//! (NexiconDK) Bus Company Online, phase 7 (docs/omsihub/BUS_COMPANY_ONLINE.md): a group's head
//! office - the koncern-HQ.
//!
//! A company that runs on more than one map of the server is a group: a head office with no map,
//! no lines, no buses and no process of its own, and its departments - one on each map, every
//! one a depot ("Umove Ahlheim" beside "Umove Grundorf"), each with its own money, lines, buses,
//! staff and books (`server_depots`). A company on its own becomes one when it founds its first
//! depot (`server_depots::found`: the head office made with it, the company its first depot); a
//! head office of before phase 7 - a company with depots - is split so when the main server
//! opens its folder (`split_head_offices`).
//!
//! The head office is a company's file all the same: `<companies>/<slug>.hq.json`, its id
//! `<slug>.hq` - with a dot, which no look into the companies takes for a company
//! (`online::company_ids`: no process, no tenders, no world), while the depots' files that name
//! it (`server_depots`' transfers, notices and approvals) reach it as they reached a head office.
//! Its books are the group's treasury: the group's own money and its loans - the bank lends on
//! the whole group (`credit`) - and the money that comes and goes between it and the depots, by
//! hand or by the depots' rules (`Rule`: a depot's surplus over an amount sent home at its day's
//! close, `sweep`; a depot below an amount topped up from the treasury, `top_ups`). The main
//! server keeps it (`step`: what came, its days closed - the bank's emergency loan, its end - and
//! the top-ups); its owner runs it from his launcher (`view`, `act`, through `server_api`).

use super::main_server::map_name;
use super::model::{BookingKind, Cents, Company, Loan};
use super::online::{company_ids, Online};
use super::server_depots::{self, Red};
use super::{dates, finance, store, Founding};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;

/// The end of a head office's id (and of its file's name before `.json`).
pub const SUFFIX: &str = ".hq";

/// The group of a head office (`Online::group`, on its file only).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Group {
    /// The depots' money rules, a depot each at most.
    pub rules: Vec<Rule>,
    /// (Step 4) The brand's version: a depot takes the head office's mark, colours and liveries
    /// when this is newer than its own (`Online::brand`).
    pub brand: u32,
    /// (Step 3) The orders given to the depots so far (`order`: their ids).
    pub orders: u64,
    /// (Step 5) The group's people besides its owner: its management, and each depot's manager.
    pub members: Vec<Member>,
}

/// A player of the server in the group (step 5): its management (`MANAGEMENT`: runs the head
/// office with its owner, a co-owner of every depot) or a depot's manager (`DEPOT_MANAGER`, its
/// `depot`: a co-owner of it).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Member {
    /// The server's id for him, and his name.
    pub player: String,
    pub name: String,
    pub role: String,
    pub depot: String,
}

/// The group's roles (`Member::role`).
pub const MANAGEMENT: &str = "management";
pub const DEPOT_MANAGER: &str = "depot";

/// A depot's money rule, the group's owner's: at its day's close what it has over `sweep_over`
/// goes to the head office (`sweep`); below `top_up_below` the head office tops it up to that
/// (`top_ups`). None: not that way.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Rule {
    pub depot: String,
    pub sweep_over: Option<Cents>,
    pub top_up_below: Option<Cents>,
}

/// Why something of the head office's cannot be done (English: keys of the translations).
pub const NOT_MADE: &str = "The group's head office could not be made: the server says why in its log.";
pub const NOT_A_GROUP: &str = "This is not a group's head office.";
pub const NOT_ITS_DEPOT: &str = "That is not a depot of the group.";
pub const BAD_RULE: &str = "Top a depot up to less than it keeps: topped up over its surplus, it would send the money straight back.";
pub const NOT_YOURS: &str = "Only the group's owner and its management run its head office.";
pub const NO_LOAN: &str = "The bank does not lend that much.";
pub const SMALL_LOAN: &str = "That is less than the bank's smallest loan.";
pub const NO_SUCH_LOAN: &str = "There is no such loan.";
pub const UNKNOWN_ACT: &str = "The server did not understand that: both need the same version of Omsi-Hub.";

/// Every text of this file (for the translations' check).
pub fn texts() -> Vec<&'static str> {
    let mut v = vec![NOT_MADE, NOT_A_GROUP, NOT_ITS_DEPOT, BAD_RULE, NOT_YOURS, NO_LOAN, SMALL_LOAN, NO_SUCH_LOAN, UNKNOWN_ACT, SAME_DEPOT, NOT_SENT_ORDER];
    v.extend([ORDER_MOVED, ORDER_LENT, ORDER_HOME, ORDER_FAILED, BRAND_TAKEN, CALLED_IN].iter().flat_map(|t| [t.0, t.1]));
    v.extend([BAD_SHORT, BAD_COLOUR, NO_SUCH_DESIGN, NO_NAME, NOT_OWNER, NO_SUCH_PLAYER, BAD_ROLE, IS_OWNER]);
    v
}

/// The smallest amount the rules move (`server_depots::send`'s least).
pub const LEAST: Cents = 100_00;

/// The most a rule names (a thousand million): what is typed over it is that.
pub const MOST: Cents = 1_000_000_000_00;

/// Whether company `c` is a group's head office.
pub fn is_group(c: &Company) -> bool {
    c.online.group.is_some()
}

/// Whether `id` is a head office's.
pub fn is_group_id(id: &str) -> bool {
    id.strip_suffix(SUFFIX).is_some_and(|base| !base.is_empty() && !base.contains('.'))
}

/// The head offices of the server's folder `data`, by id.
pub fn group_ids(data: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(store::dir(data)) else { return Vec::new() };
    let mut ids: Vec<String> = rd.flatten().filter_map(|e| e.file_name().to_string_lossy().strip_suffix(".json").filter(|id| is_group_id(id)).map(str::to_string)).collect();
    ids.sort();
    ids
}

/// Whether nothing of the server's folder is under head office id `id` yet: no file, and nothing
/// sent or told to one of that id before (a company of the id removed: `store::delete`).
fn free_id(data: &Path, id: &str) -> bool {
    !store::path_of(data, id).exists() && server_depots::files_of(data, id).iter().all(|p| !p.exists())
}

/// An id for the head office of company `c`'s group: `<its id>.hq`, or (taken) the next free
/// one of its name.
fn unused_id(data: &Path, c: &Company) -> String {
    let own = format!("{}{SUFFIX}", c.id);
    if is_group_id(&own) && free_id(data, &own) {
        return own;
    }
    let base = super::slug(&c.name);
    let mut id = format!("{base}{SUFFIX}");
    let mut n = 2;
    while !free_id(data, &id) {
        id = format!("{base}-{n}{SUFFIX}");
        n += 1;
    }
    id
}

/// A head office for company `c`'s group (not kept yet: `join_group`): its name, mark, colours,
/// game mode, country and currency, its owner; no money, no map.
pub fn new_group(data: &Path, c: &Company) -> Company {
    let f = Founding {
        name: c.name.clone(),
        short: c.short.clone(),
        colours: c.colours.clone(),
        logo: c.logo.clone(),
        date: c.date.clone(),
        game_mode: c.game_mode,
        company_mode: c.company_mode,
        country: c.country.clone(),
        currency: c.currency,
        ..Default::default()
    };
    let mut g = super::found(&f, &c.profile);
    g.id = unused_id(data, c);
    g.founded = c.founded.clone();
    g.cash = 0;
    g.ledger.clear();
    g.months.clear();
    g.online = Online { on_server: true, owner: c.online.owner.clone(), owner_id: c.online.owner_id.clone(), share: c.online.share, group: Some(Group::default()), ..Default::default() };
    g
}

/// Company `c` joins the group of head office `hq` (kept now when it is new) as a depot: tied to
/// it, named by its map ("Umove Ahlheim"). The caller keeps `c`.
pub fn join_group(data: &Path, hq: &Company, c: &mut Company) -> Result<(), &'static str> {
    if !store::path_of(data, &hq.id).is_file() {
        store::save(data, hq).map_err(|_| NOT_MADE)?;
    }
    c.online.hq = hq.id.clone();
    let place = map_name(&c.map);
    if !place.is_empty() && !c.name.to_lowercase().ends_with(&place.to_lowercase()) {
        c.name = format!("{} {place}", c.name.trim());
    }
    Ok(())
}

/// Company `c`'s group made: its head office (kept) and it the group's first depot. The head
/// office.
pub fn make_group(data: &Path, c: &mut Company) -> Result<Company, &'static str> {
    let g = new_group(data, c);
    join_group(data, &g, c)?;
    Ok(g)
}

/// (Decided 26 October 2026) Every head office of before phase 7 - a company of the server with
/// depots - split into its group's head office and a depot: the head office made (the company
/// keeps its money and its loans; only what it is in the red and the bank's emergency loan are
/// the group's - the head office's), the company its first depot named by its map, its depots
/// tied to the new head office, and the requests for approval waiting for it the head office's.
/// Kept in that order - the head office, the depots, the company last - so a split cut short is
/// done again at the next opening, with the same head office (`<its id>.hq`). Only while no
/// company's process runs (the main server's opening). What was split, for the log.
pub fn split_head_offices(data: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for id in company_ids(data) {
        let Ok(mut c) = store::load(data, &id) else { continue };
        if !c.online.on_server || !c.online.hq.is_empty() || is_group(&c) {
            continue;
        }
        let depots = server_depots::depots_of(data, &c.id);
        if depots.is_empty() {
            continue;
        }
        let old_name = c.name.clone();
        // (begun before and cut short: its head office made already)
        let again = format!("{}{SUFFIX}", c.id);
        let (mut g, fresh) = match store::load(data, &again) {
            Ok(g) if is_group(&g) => (g, false),
            _ => (new_group(data, &c), true),
        };
        // (what its depots asked it and it has not answered: the head office's now)
        let mut asks = std::mem::take(&mut c.online.asks);
        asks.extend(server_depots::asks_after(data, &c.id, c.online.asks_read));
        for a in asks {
            if !g.online.asks.iter().any(|x| x.id == a.id) {
                g.online.asks.push(a);
            }
        }
        c.online.asks_read = 0;
        // (the red and the bank's emergency loan are the group's: its treasury takes them - the
        // head office's side once, when it is made)
        if c.cash < 0 {
            let deficit = c.cash;
            if fresh {
                g.book(BookingKind::Capital, deficit, format!("{}: its deficit taken over", map_name(&c.map)), false);
            }
            c.book(BookingKind::Capital, -deficit, "Deficit taken over by the group's head office".to_string(), false);
            c.online.red_days = 0;
        }
        let emergency: Vec<Loan> = c.loans.iter().filter(|l| l.purpose == server_depots::EMERGENCY_PURPOSE).cloned().collect();
        c.loans.retain(|l| l.purpose != server_depots::EMERGENCY_PURPOSE);
        if fresh {
            for mut l in emergency {
                g.counters.loan += 1;
                l.id = g.counters.loan;
                g.loans.push(l);
            }
        }
        if join_group(data, &g, &mut c).is_err() || store::save(data, &g).is_err() {
            continue;
        }
        let tied = depots.iter().all(|d| {
            store::load(data, &d.id).is_ok_and(|mut x| {
                x.online.hq = g.id.clone();
                store::save(data, &x).is_ok()
            })
        });
        if !tied || store::save(data, &c).is_err() {
            out.push(format!("{old_name}: its split into a group was cut short - done again at the next start"));
            continue;
        }
        out.push(format!("{old_name}: a group now - its head office {} ({}), {} a depot as its {} others", g.id, g.name, c.name, depots.len()));
    }
    out
}

// --- the main server keeps it -------------------------------------------------------------------

/// What a step of a head office came to.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stepped {
    /// Something changed: it is to be kept.
    pub changed: bool,
    /// Its days closed.
    pub closed: u32,
    /// It went bankrupt: its depots told to close; its file is the caller's to remove.
    pub bankrupt: bool,
}

/// Head office `g` a step on, to the server's day `date` (`real`: Unix seconds): the money, the
/// news and the requests its depots sent (`server_depots`), its days closed up to `date` - in
/// the red the bank's emergency loan, then warned, then bankrupt (`server_depots::day_closed`) -
/// its loans' rates at a new month, and the depots below their rule topped up (`top_ups`).
pub fn step(data: &Path, g: &mut Company, date: &str, real: i64) -> Stepped {
    let mut s = Stepped::default();
    if !is_group(g) {
        return s;
    }
    if server_depots::take_transfers(data, g) != 0 {
        s.changed = true;
    }
    if server_depots::take_notices(data, g).0 {
        s.changed = true;
    }
    if server_depots::approvals_step(data, g) {
        s.changed = true;
    }
    if dates::parse(date).is_some() {
        // (a server down for long: a year of days at most, at once)
        while dates::between(&g.date, date) > 0 && s.closed < 366 {
            if day_closed(data, g) == Red::Bankrupt {
                server_depots::bankrupt(data, g);
                s.bankrupt = true;
                s.changed = true;
                return s;
            }
            let next = dates::add(&g.date, 1);
            let new_month = dates::month_of(&next) != dates::month_of(&g.date);
            g.date = next;
            if new_month {
                finance::pay_rates(g);
            }
            s.closed += 1;
            s.changed = true;
        }
    }
    if top_ups(data, g, real) {
        s.changed = true;
    }
    s
}

/// What the head office tells of the treasury's money called in from its depots.
pub const CALLED_IN: (&str, &str) = ("The treasury called in %{amount}", "The treasury closed the day short: %{amount} called in from the depots with money (%{depots}), sent home at their next step.");

/// Head office `g`'s day closed. The treasury short while the whole group is not in the red
/// (`consolidated`): what it lacks is called in from its depots with money - the richest first,
/// each keeping what its rule tops it up to - by an order each (`send_home`), what is on its way
/// home already counted; the day is fine. The whole group in the red: as a head office's
/// (`server_depots::day_closed`: the bank's emergency loan, then warned, then bankrupt).
pub fn day_closed(data: &Path, g: &mut Company) -> Red {
    if g.cash >= 0 {
        g.online.red_days = 0;
        return Red::Fine;
    }
    let (all, depots) = consolidated(data, g);
    if all.cash < 0 {
        return server_depots::day_closed(data, g);
    }
    let coming = server_depots::pending_to(data, &g.id, g.online.transfers_read) + depots.iter().flat_map(|d| orders_waiting(data, d)).filter(|o| o.hq == g.id && o.what == "send_home").map(|o| o.amount).sum::<Cents>();
    let mut need = (-g.cash).saturating_sub(coming);
    let mut rich: Vec<(Cents, &Company)> = depots.iter().map(|d| (d.cash - rule_of(g, &d.id).and_then(|r| r.top_up_below).unwrap_or(0).max(0), d)).filter(|(spare, _)| *spare >= LEAST).collect();
    rich.sort_by_key(|(spare, _)| -spare);
    let (mut called, mut from) = (0, Vec::new());
    for (spare, d) in rich {
        if need < LEAST {
            break;
        }
        let amount = spare.min(need).max(LEAST);
        let o = Order { what: "send_home".into(), to: g.id.clone(), amount, ..Default::default() };
        if order(data, g, "", &d.id, o).is_ok() {
            need -= amount;
            called += amount;
            from.push(d.name.clone());
        }
    }
    g.online.red_days = 0;
    if called > 0 {
        let m = super::inbox::Message::new(super::inbox::Kind::Finance, CALLED_IN.0, CALLED_IN.1).arg("amount", called.to_string()).arg("depots", from.join(", ")).level(super::clock::Level::Warn);
        super::inbox::post(g, m);
    }
    Red::Fine
}

/// The rule of depot `depot` in head office `g`'s group.
pub fn rule_of<'a>(g: &'a Company, depot: &str) -> Option<&'a Rule> {
    g.online.group.as_ref()?.rules.iter().find(|r| r.depot == depot)
}

/// The depots of head office `g` below their rule's top-up topped up from its money, as far as
/// it goes: each to its amount - what it has, as its file says, and what is on its way to it
/// counted (`server_depots::pending_to`). Whether any money went.
pub fn top_ups(data: &Path, g: &mut Company, real: i64) -> bool {
    let rules = g.online.group.as_ref().map(|x| x.rules.clone()).unwrap_or_default();
    let mut any = false;
    for r in rules {
        let Some(below) = r.top_up_below.filter(|b| *b >= 0) else { continue };
        let Ok(d) = store::load(data, &r.depot) else { continue };
        if d.online.hq != g.id {
            continue;
        }
        let has = d.cash.saturating_add(server_depots::pending_to(data, &d.id, d.online.transfers_read));
        let amount = below.saturating_sub(has).min(g.cash);
        if has >= below || amount < LEAST {
            continue;
        }
        if server_depots::send(data, g, &d.id, amount, real).is_ok() {
            any = true;
        }
    }
    any
}

/// A depot's day closed (its process, `online::Server::tick_in`): what it has over its rule's
/// surplus sent to its head office. How much went.
pub fn sweep(data: &Path, c: &mut Company, real: i64) -> Cents {
    if !is_group_id(&c.online.hq) {
        return 0;
    }
    let Ok(g) = store::load(data, &c.online.hq) else { return 0 };
    let Some(over) = rule_of(&g, &c.id).and_then(|r| r.sweep_over) else { return 0 };
    let surplus = c.cash.saturating_sub(over.max(0));
    if surplus < LEAST {
        return 0;
    }
    match server_depots::send(data, c, &g.id, surplus, real) {
        Ok(()) => surplus,
        Err(_) => 0,
    }
}

/// Depot `depot`'s rule set in head office `g`'s group (both None: none): its surplus over
/// `sweep_over` home at each day's close, below `top_up_below` topped up - the top-up not over
/// the surplus kept.
pub fn set_rule(data: &Path, g: &mut Company, depot: &str, sweep_over: Option<Cents>, top_up_below: Option<Cents>) -> Result<(), &'static str> {
    if !is_group(g) {
        return Err(NOT_A_GROUP);
    }
    if !server_depots::depots_of(data, &g.id).iter().any(|d| d.id == depot) {
        return Err(NOT_ITS_DEPOT);
    }
    let (sweep_over, top_up_below) = (sweep_over.map(|x| x.clamp(0, MOST)), top_up_below.map(|x| x.clamp(0, MOST)));
    if let (Some(s), Some(t)) = (sweep_over, top_up_below) {
        if t > s {
            return Err(BAD_RULE);
        }
    }
    let Some(group) = g.online.group.as_mut() else { return Err(NOT_A_GROUP) };
    group.rules.retain(|r| r.depot != depot);
    if sweep_over.is_some() || top_up_below.is_some() {
        group.rules.push(Rule { depot: depot.to_string(), sweep_over, top_up_below });
        group.rules.sort_by(|a, b| a.depot.cmp(&b.depot));
    }
    Ok(())
}

// --- the group's books ----------------------------------------------------------------------------

/// The group as one company: the head office with every depot's money, buses, staff, lines and
/// loans added, their months summed (what went between them is capital: no part of a result) -
/// and the depots.
pub fn consolidated(data: &Path, g: &Company) -> (Company, Vec<Company>) {
    let depots: Vec<Company> = server_depots::depots_of(data, &g.id).iter().filter_map(|d| store::load(data, &d.id).ok()).collect();
    let mut all = g.clone();
    for d in &depots {
        all.cash += d.cash;
        all.fleet.extend(d.fleet.iter().cloned());
        all.staff.extend(d.staff.iter().cloned());
        all.lines.extend(d.lines.iter().cloned());
        all.loans.extend(d.loans.iter().cloned());
        for m in &d.months {
            let k = match all.months.iter().position(|x| x.month == m.month) {
                Some(k) => k,
                None => {
                    all.months.push(super::model::Month { month: m.month.clone(), by_kind: Vec::new() });
                    all.months.len() - 1
                }
            };
            for (kind, cents) in &m.by_kind {
                match all.months[k].by_kind.iter_mut().find(|x| x.0 == *kind) {
                    Some(x) => x.1 += cents,
                    None => all.months[k].by_kind.push((*kind, *cents)),
                }
            }
        }
    }
    all.months.sort_by(|a, b| a.month.cmp(&b.month));
    (all, depots)
}

/// What the bank lends the group's head office: on the whole group (`consolidated`) - its
/// buses, what it earns, less what all of it owes.
pub fn credit(data: &Path, g: &Company) -> finance::Credit {
    finance::credit(&consolidated(data, g).0, 0)
}

/// A loan to the head office (the bank's terms for the group: `finance::loan_terms`), into its
/// treasury. Its id.
pub fn borrow(data: &Path, g: &mut Company, amount: Cents) -> Result<u32, &'static str> {
    if !is_group(g) {
        return Err(NOT_A_GROUP);
    }
    if amount < finance::SMALLEST_LOAN {
        return Err(SMALL_LOAN);
    }
    if amount > credit(data, g).left {
        return Err(NO_LOAN);
    }
    let (monthly, months, rate) = finance::loan_terms(g, amount);
    g.counters.loan += 1;
    let id = g.counters.loan;
    let purpose = "Group loan".to_string();
    g.loans.push(Loan { id, taken: g.date.clone(), principal: amount, remaining: amount, rate, monthly, months_left: months, purpose: purpose.clone() });
    g.book(BookingKind::Loan, amount, purpose, false);
    Ok(id)
}

// --- step 3: the group's fleet and staff -----------------------------------------------------------

/// An order of the head office's to one of its depots (`orders_file`), done by the depot's own
/// process (`take_orders`): its bus moved to another depot of the group (`move_bus`), or an
/// employee lent to one (`lend`, for `days`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Order {
    pub id: u64,
    /// The head office that gave it (a depot takes only its own's).
    pub hq: String,
    /// Who gave it (the server's name for him).
    pub by: String,
    /// `move_bus` or `lend`.
    #[serde(rename = "do")]
    pub what: String,
    pub bus: u32,
    pub employee: u32,
    pub to: String,
    pub days: i64,
    /// (`send_home`) The money the treasury calls in (`day_closed`).
    pub amount: Cents,
}

/// The head office's orders to depot `id`, a JSON line each.
pub fn orders_file(data: &Path, id: &str) -> std::path::PathBuf {
    data.join("hq-orders").join(format!("{id}.jsonl"))
}

/// Why the head office cannot give an order.
pub const SAME_DEPOT: &str = "Choose another depot to send it to.";
pub const NOT_SENT_ORDER: &str = "The order could not be given: the server says why in its log.";

/// What a depot tells of the head office's orders (its inbox; the head office's when it could
/// not).
pub const ORDER_MOVED: (&str, &str) = ("The head office sent %{bus} to %{other}", "By the head office's order %{bus} is on its way to %{other}, there on %{day}.");
pub const ORDER_LENT: (&str, &str) = ("The head office lent %{name} to %{other}", "By the head office's order %{name} goes to %{other} tomorrow for %{n} days.");
pub const ORDER_HOME: (&str, &str) = ("The head office called in %{amount}", "The group's treasury was short at its day's close: %{amount} sent home.");
pub const ORDER_FAILED: (&str, &str) = ("%{depot} could not do the order", "%{depot} could not do the head office's order (%{what}): %{why}");

/// Head office `g` orders depot `from` (by `by`): `what` is `move_bus` (its bus `bus` to depot
/// `to`) or `lend` (its employee `employee` to `to` for `days`). Done by the depot's process at its
/// next step; what it could not do it tells the head office.
pub fn order(data: &Path, g: &mut Company, by: &str, from: &str, mut o: Order) -> Result<(), &'static str> {
    if !is_group(g) {
        return Err(NOT_A_GROUP);
    }
    let depots = server_depots::depots_of(data, &g.id);
    let home = o.what == "send_home";
    if !depots.iter().any(|d| d.id == from) || (!home && !depots.iter().any(|d| d.id == o.to)) {
        return Err(NOT_ITS_DEPOT);
    }
    if from == o.to {
        return Err(SAME_DEPOT);
    }
    let d = store::load(data, from).map_err(|_| NOT_ITS_DEPOT)?;
    match o.what.as_str() {
        // (the treasury's money called in: `day_closed`)
        "send_home" => {
            if o.amount < LEAST || o.to != g.id {
                return Err(UNKNOWN_ACT);
            }
        }
        "move_bus" => {
            let v = d.vehicle(o.bus).ok_or(server_depots::NO_SUCH_BUS)?;
            if !matches!(v.tenure, super::model::Tenure::Owned { .. }) {
                return Err(server_depots::NOT_OWNED);
            }
        }
        "lend" => {
            if !(1..=90).contains(&o.days) {
                return Err(server_depots::BAD_DAYS);
            }
            let e = d.staff.iter().find(|e| e.id == o.employee).ok_or(server_depots::NO_SUCH_EMPLOYEE)?;
            if e.lent.is_some() {
                return Err(server_depots::BORROWED);
            }
        }
        _ => return Err(UNKNOWN_ACT),
    }
    let group = g.online.group.as_mut().ok_or(NOT_A_GROUP)?;
    group.orders += 1;
    o.id = group.orders;
    o.hq = g.id.clone();
    o.by = by.to_string();
    let line = serde_json::to_string(&o).map_err(|_| NOT_SENT_ORDER)?;
    server_depots::append_line(&orders_file(data, from), &line).map_err(|_| NOT_SENT_ORDER)
}

/// The real time (Unix seconds).
fn real_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// An amount as a message's place says it (whole units, for the head office's log).
fn eur_text(c: Cents) -> String {
    format!("{:.0}", c as f64 / 100.0)
}

/// The head office's orders to depot `c` not taken yet.
pub fn orders_waiting(data: &Path, c: &Company) -> Vec<Order> {
    server_depots::lines_after(&orders_file(data, &c.id), c.online.orders_read).iter().filter_map(|(l, _)| serde_json::from_str(l).ok()).collect()
}

/// The head office's orders to depot `c` since its last look, done by its process (`move_bus`,
/// `lend` with the server's price and days of a move: `bus_move_price`, `bus_move_days`), each
/// told in its inbox; what it could not do told to the head office. Whether any came.
pub fn take_orders(data: &Path, c: &mut Company) -> bool {
    use super::inbox::{self, Kind, Message};
    let lines = server_depots::lines_after(&orders_file(data, &c.id), c.online.orders_read);
    if lines.is_empty() {
        return false;
    }
    let cfg = super::main_server::MainCfg::parse(&std::fs::read_to_string(data.join(super::main_server::CFG_FILE)).unwrap_or_default());
    for (line, end) in lines {
        c.online.orders_read = end;
        let Ok(o) = serde_json::from_str::<Order>(&line) else { continue };
        if o.hq.is_empty() || o.hq != c.online.hq {
            continue;
        }
        let other = store::load(data, &o.to).map(|x| x.name).unwrap_or_else(|_| o.to.clone());
        let (what, done) = match o.what.as_str() {
            "move_bus" => {
                let bus = c.vehicle(o.bus).map(|v| format!("{} {}", v.number, v.name)).unwrap_or_else(|| o.bus.to_string());
                let r = server_depots::move_bus(data, c, o.bus, &o.to, cfg.bus_move_price, cfg.bus_move_days).map(|day| {
                    Message::new(Kind::Finance, ORDER_MOVED.0, ORDER_MOVED.1).arg("bus", bus.clone()).arg("other", other.clone()).arg("day", day)
                });
                (bus, r)
            }
            "lend" => {
                let name = c.staff.iter().find(|e| e.id == o.employee).map(|e| e.name.clone()).unwrap_or_else(|| o.employee.to_string());
                let r = server_depots::lend(data, c, o.employee, &o.to, o.days).map(|_| {
                    Message::new(Kind::Finance, ORDER_LENT.0, ORDER_LENT.1).arg("name", name.clone()).arg("other", other.clone()).arg("n", o.days.to_string())
                });
                (name, r)
            }
            "send_home" => {
                let amount = o.amount.min(c.cash);
                let r = if amount < LEAST { Err(server_depots::NO_CASH_SEND) } else { server_depots::send(data, c, &o.hq, amount, real_now()) };
                let r = r.map(|_| Message::new(Kind::Finance, ORDER_HOME.0, ORDER_HOME.1).arg("amount", amount.to_string()));
                (eur_text(o.amount), r)
            }
            _ => continue,
        };
        match done {
            Ok(m) => inbox::post(c, m),
            Err(why) => {
                let n = server_depots::Notice {
                    title: ORDER_FAILED.0.into(),
                    text: ORDER_FAILED.1.into(),
                    args: vec![("depot".into(), c.name.clone()), ("what".into(), what), ("why".into(), why.into())],
                    bad: true,
                    ..Default::default()
                };
                let _ = server_depots::tell(data, &o.hq, &n);
            }
        }
    }
    true
}

// --- step 4: the group's brand ---------------------------------------------------------------------

/// How often a depot's process looks at its head office for the group's brand (real seconds).
pub const BRAND_EVERY: i64 = 20;

/// What a depot tells of the group's brand taken.
pub const BRAND_TAKEN: (&str, &str) = ("The group's brand changed", "The head office set the group's name, mark, colours and liveries: the depot wears them from now on. Its buses are painted in a livery of the group in the workshop, as in one of its own.");

/// Why the brand cannot be set.
pub const BAD_SHORT: &str = "A short name has two to four letters or digits.";
pub const BAD_COLOUR: &str = "A colour is written #rrggbb.";
pub const NO_SUCH_DESIGN: &str = "The depot has no such livery.";
pub const NO_NAME: &str = "Give the group a name.";

fn colour_ok(c: &str) -> bool {
    let h = c.trim().trim_start_matches('#');
    h.len() == 6 && h.chars().all(|x| x.is_ascii_hexdigit())
}

/// The group's brand a version on: every depot takes it at its next look (`take_brand`).
fn brand_changed(g: &mut Company) {
    if let Some(gr) = g.online.group.as_mut() {
        gr.brand += 1;
    }
}

/// The group's name, short name and colours set (`name`, `short`, `colours`; the depots named
/// "<name> <map>").
pub fn set_brand(g: &mut Company, name: &str, short: &str, colours: [&str; 2]) -> Result<(), &'static str> {
    if !is_group(g) {
        return Err(NOT_A_GROUP);
    }
    let short = short.trim().to_uppercase();
    if !(2..=4).contains(&short.chars().count()) || !short.chars().all(|c| c.is_alphanumeric()) {
        return Err(BAD_SHORT);
    }
    if !colours.iter().all(|c| colour_ok(c)) {
        return Err(BAD_COLOUR);
    }
    if name.trim().is_empty() {
        return Err(NO_NAME);
    }
    g.name = name.trim().to_string();
    g.short = short;
    g.colours = colours.map(|c| format!("#{}", c.trim().trim_start_matches('#').to_lowercase()));
    brand_changed(g);
    Ok(())
}

/// Depot `depot`'s mark - its logo picture, short name and colours, as its settings have them -
/// the whole group's.
pub fn mark_of_depot(data: &Path, g: &mut Company, depot: &str) -> Result<(), &'static str> {
    if !is_group(g) {
        return Err(NOT_A_GROUP);
    }
    if !server_depots::depots_of(data, &g.id).iter().any(|d| d.id == depot) {
        return Err(NOT_ITS_DEPOT);
    }
    let d = store::load(data, depot).map_err(|_| NOT_ITS_DEPOT)?;
    g.short = d.short;
    g.colours = d.colours;
    g.logo = d.logo;
    brand_changed(g);
    Ok(())
}

/// Depot `depot`'s livery `name` (`Company::designs`, made in its livery studio) the whole
/// group's: every depot has it to paint its buses in, its design paid once.
pub fn share_livery(data: &Path, g: &mut Company, depot: &str, name: &str) -> Result<(), &'static str> {
    if !is_group(g) {
        return Err(NOT_A_GROUP);
    }
    if !server_depots::depots_of(data, &g.id).iter().any(|d| d.id == depot) {
        return Err(NOT_ITS_DEPOT);
    }
    let d = store::load(data, depot).map_err(|_| NOT_ITS_DEPOT)?;
    let design = d.designs.iter().find(|x| x.name.trim().eq_ignore_ascii_case(name.trim())).ok_or(NO_SUCH_DESIGN)?;
    g.designs.retain(|x| !(x.name.trim().eq_ignore_ascii_case(&design.name) && x.bus == design.bus));
    g.designs.push(super::livery::Design { paid: 0, ..design.clone() });
    brand_changed(g);
    Ok(())
}

/// The group's livery `name` no longer handed to the depots (those that have it keep it).
pub fn unshare_livery(g: &mut Company, name: &str) -> Result<(), &'static str> {
    if !is_group(g) {
        return Err(NOT_A_GROUP);
    }
    g.designs.retain(|x| !x.name.trim().eq_ignore_ascii_case(name.trim()));
    brand_changed(g);
    Ok(())
}

/// A depot of a group takes from its head office (its process, every `BRAND_EVERY`) what is the
/// group's: its brand (`take_brand`) and its roles (`take_roles`). Whether it changed.
pub fn take_group(data: &Path, c: &mut Company) -> bool {
    if !is_group_id(&c.online.hq) {
        return false;
    }
    let Ok(g) = store::load(data, &c.online.hq) else { return false };
    if !is_group(&g) {
        return false;
    }
    let brand = take_brand(c, &g);
    take_roles(c, &g) || brand
}

/// Depot `c` takes its group's brand when head office `g`'s is newer than the one it took
/// (`Group::brand`): its name ("<group> <map>"), short name, colours and logo the group's, the
/// group's liveries added to its own (their design not paid again); told in its inbox. Whether it
/// changed.
pub fn take_brand(c: &mut Company, g: &Company) -> bool {
    let Some(gr) = g.online.group.as_ref() else { return false };
    if gr.brand == c.online.brand {
        return false;
    }
    c.online.brand = gr.brand;
    let place = map_name(&c.map);
    c.name = if place.is_empty() { g.name.clone() } else { format!("{} {place}", g.name) };
    (c.short, c.colours, c.logo) = (g.short.clone(), g.colours.clone(), g.logo.clone());
    for d in &g.designs {
        if !c.designs.iter().any(|x| x.name.trim().eq_ignore_ascii_case(d.name.trim()) && x.bus == d.bus) {
            c.designs.push(super::livery::Design { paid: 0, ..d.clone() });
        }
    }
    super::inbox::post(c, super::inbox::Message::new(super::inbox::Kind::Finance, BRAND_TAKEN.0, BRAND_TAKEN.1));
    true
}

// --- step 5: the group's roles ---------------------------------------------------------------------

/// Why a role cannot be given.
pub const NOT_OWNER: &str = "Only the group's owner gives the group's roles.";
pub const NO_SUCH_PLAYER: &str = "The server has no such player.";
pub const BAD_ROLE: &str = "That is no role of the group.";
pub const IS_OWNER: &str = "The group's owner has every role already.";

/// Player `player` (`name`) given role `role` in head office `g`'s group - `MANAGEMENT`, or
/// `DEPOT_MANAGER` of depot `depot` (a depot has one: the one before loses it) - or, `role`
/// empty, none: every depot takes it at its next look (`take_roles`).
pub fn set_member(data: &Path, g: &mut Company, player: &str, name: &str, role: &str, depot: &str) -> Result<(), &'static str> {
    if !is_group(g) {
        return Err(NOT_A_GROUP);
    }
    if player.trim().is_empty() {
        return Err(NO_SUCH_PLAYER);
    }
    if player == g.online.owner_id {
        return Err(IS_OWNER);
    }
    let depot = match role {
        MANAGEMENT | "" => String::new(),
        DEPOT_MANAGER => {
            if !server_depots::depots_of(data, &g.id).iter().any(|d| d.id == depot) {
                return Err(NOT_ITS_DEPOT);
            }
            depot.to_string()
        }
        _ => return Err(BAD_ROLE),
    };
    let Some(gr) = g.online.group.as_mut() else { return Err(NOT_A_GROUP) };
    gr.members.retain(|m| m.player != player && !(role == DEPOT_MANAGER && m.role == DEPOT_MANAGER && m.depot == depot));
    if !role.is_empty() {
        gr.members.push(Member { player: player.to_string(), name: name.to_string(), role: role.to_string(), depot });
    }
    Ok(())
}

/// The players head office `g` makes co-owners of depot `id`: its management and the depot's
/// manager (id, name).
fn co_owners_of(g: &Company, id: &str) -> Vec<(String, String)> {
    g.online.group.as_ref().map_or_else(Vec::new, |gr| gr.members.iter().filter(|m| m.role == MANAGEMENT || (m.role == DEPOT_MANAGER && m.depot == id)).map(|m| (m.player.clone(), m.name.clone())).collect())
}

/// Depot `c` takes its group's roles from head office `g`: its management and its manager
/// co-owners of it (players of it from now on, as a player who joined), and who the group made one
/// and no longer does a driver again (`Online::group_roles`). Whether it changed.
pub fn take_roles(c: &mut Company, g: &Company) -> bool {
    use super::together::{roles, Role};
    let want = co_owners_of(g, &c.id);
    let mut changed = false;
    for (id, name) in &want {
        let Some(no) = c.together.guest_of(id, name) else { continue };
        if c.together.guest(no).is_some_and(|x| x.role != Role::CoOwner || x.banned) {
            c.together.set_role(no, Role::CoOwner, &roles::effective(Role::CoOwner, &[], &[]));
            if let Some(x) = c.together.guest_mut(no) {
                x.banned = false;
            }
            changed = true;
        }
        if !c.online.group_roles.contains(id) {
            c.online.group_roles.push(id.clone());
            changed = true;
        }
    }
    let gone: Vec<String> = c.online.group_roles.iter().filter(|id| !want.iter().any(|(w, _)| w == *id)).cloned().collect();
    for id in gone {
        if let Some(no) = c.together.guests.iter().find(|x| x.id == id).map(|x| x.no) {
            c.together.set_role(no, Role::Driver, &roles::effective(Role::Driver, &[], &[]));
        }
        c.online.group_roles.retain(|x| *x != id);
        changed = true;
    }
    changed
}

// --- its owner's page -------------------------------------------------------------------------------

/// What the head office's page asks the main server for, besides the head office: which depots'
/// processes run, the price of a depot, the maps a new one may go on, the server's time.
pub struct Around {
    pub running: Vec<String>,
    pub depot_price: Cents,
    pub maps: Vec<String>,
    pub time: String,
    /// (Step 3) What moving a bus to another map costs, and the days it is away.
    pub bus_move: (Cents, i64),
    /// (Step 5) The server's players, to give the group's roles to (id, name).
    pub players: Vec<(String, String)>,
}

/// A month of the group's books for the page: its kinds (capital left out), income, expenses and
/// result.
fn month_json(c: &Company, month: &str) -> Value {
    let m = c.month(month);
    let mut kinds: Vec<(BookingKind, Cents)> = m.by_kind.iter().filter(|x| !x.0.is_capital() && x.1 != 0).copied().collect();
    kinds.sort_by_key(|x| -x.1.abs());
    json!({
        "month": month, "result": m.result(), "income": m.income(), "expenses": m.expenses(),
        "kinds": kinds.iter().map(|(k, v)| json!({ "kind": k.label(), "amount": v })).collect::<Vec<_>>(),
    })
}

/// The head office's page for player `me` (the server's id for him): the group's money - its
/// treasury, its loans, the bank's credit, the whole group's - its depots with their money, month
/// and rule, the group's books of this month and the last, the treasury's last bookings, its
/// news, the requests waiting, and what founding a depot asks.
pub fn view(data: &Path, g: &Company, me: &str, around: &Around) -> Value {
    let (all, depots) = consolidated(data, g);
    let month = dates::month_of(&g.date);
    let last = dates::month_of(&dates::add(&format!("{month}-01"), -1));
    let cr = finance::credit(&all, 0);
    let depots_json: Vec<Value> = depots
        .iter()
        .map(|d| {
            let r = rule_of(g, &d.id);
            json!({
                "id": d.id, "name": d.name, "map": map_name(&d.map), "short": d.short, "colours": d.colours,
                "cash": d.cash, "result": d.month(&month).result(), "last": d.month(&last).result(),
                "lines": d.lines.len(), "buses": d.fleet.len(), "staff": d.staff.len(), "debt": d.debt(),
                "runs": around.running.iter().any(|x| *x == d.id), "red_days": d.online.red_days,
                "pending": server_depots::pending_to(data, &d.id, d.online.transfers_read),
                "rule": { "sweep_over": r.and_then(|r| r.sweep_over), "top_up_below": r.and_then(|r| r.top_up_below) },
            })
        })
        .collect();
    // (step 3: each depot's buses and staff, what is on its way between them, the orders waiting)
    let names: std::collections::HashMap<String, String> = depots.iter().map(|d| (d.id.clone(), d.name.clone())).collect();
    let fleet: Vec<Value> = depots
        .iter()
        .map(|d| {
            let buses: Vec<Value> = d.fleet.iter().map(|v| json!({ "id": v.id, "number": v.number, "name": v.name, "kind": v.kind.label(), "owned": matches!(v.tenure, super::model::Tenure::Owned { .. }), "workshop": v.workshop_until.is_some() })).collect();
            let staff: Vec<Value> = d
                .staff
                .iter()
                .map(|e| {
                    json!({
                        "id": e.id, "name": e.name, "role": e.role.label(),
                        "free": e.notice_until.is_none() && e.lent.is_none() && e.lend.is_none(),
                        "lent": e.lent.as_ref().map(|l| json!({ "from": l.name, "until": l.until })),
                        "lend": e.lend.as_ref().map(|l| json!({ "to": l.name, "days": l.days })),
                    })
                })
                .collect();
            let on_way: Vec<Value> = server_depots::buses_on_the_way(data, d).into_iter().filter(|x| x.3).map(|(bus, other, arrives, _)| json!({ "bus": bus, "to": other, "day": arrives })).collect();
            let orders: Vec<Value> = orders_waiting(data, d)
                .iter()
                .filter(|o| o.hq == g.id)
                .map(|o| {
                    let what = match o.what.as_str() {
                        "move_bus" => d.vehicle(o.bus).map(|v| format!("{} {}", v.number, v.name)).unwrap_or_default(),
                        _ => d.staff.iter().find(|e| e.id == o.employee).map(|e| e.name.clone()).unwrap_or_default(),
                    };
                    json!({ "do": o.what, "what": what, "to": names.get(&o.to).cloned().unwrap_or_else(|| o.to.clone()), "days": o.days })
                })
                .collect();
            json!({ "id": d.id, "name": d.name, "buses": buses, "staff": staff, "on_way": on_way, "orders": orders })
        })
        .collect();
    let ledger: Vec<Value> = g.ledger.iter().rev().take(40).map(|b| json!({ "date": b.date, "kind": b.kind.label(), "amount": b.amount, "text": b.text })).collect();
    let inbox: Vec<Value> = g.inbox.messages.iter().rev().take(30).map(|m| json!({ "date": m.date, "title": m.title, "text": m.text, "args": m.args, "level": m.level })).collect();
    json!({
        "id": g.id, "name": g.name, "short": g.short, "colours": g.colours, "date": g.date, "time": around.time,
        "mode": g.game_mode, "currency": g.currency,
        "owner": g.online.owner, "mine": g.online.owner_id == me,
        "cash": g.cash, "debt": g.debt(), "group_cash": all.cash, "group_debt": all.debt(),
        "depots": depots_json,
        "month": month_json(&all, &month), "last_month": month_json(&all, &last),
        "credit": { "limit": cr.limit, "debt": cr.debt, "left": cr.left, "rate": cr.rate, "smallest": finance::SMALLEST_LOAN },
        "loans": g.loans.iter().map(|l| json!({ "id": l.id, "purpose": l.purpose, "remaining": l.remaining, "monthly": l.monthly, "months_left": l.months_left, "rate": l.rate })).collect::<Vec<_>>(),
        "ledger": ledger, "inbox": inbox,
        "asks": g.online.asks.iter().map(|a| json!({ "id": a.id, "depot": a.depot, "name": a.name, "amount": a.amount, "reason": a.reason })).collect::<Vec<_>>(),
        "maps": around.maps, "depot_price": around.depot_price,
        "fleet": fleet, "bus_move_price": around.bus_move.0, "bus_move_days": around.bus_move.1,
        // (step 4: the group's brand - its liveries, and every depot's to share; which depots
        // wear it already)
        "logo": g.logo.is_some(),
        "liveries": g.designs.iter().map(|x| json!({ "name": x.name, "bus": x.bus })).collect::<Vec<_>>(),
        "depot_liveries": depots.iter().flat_map(|d| d.designs.iter().filter(|x| !g.designs.iter().any(|y| y.name.trim().eq_ignore_ascii_case(x.name.trim()) && y.bus == x.bus)).map(|x| json!({ "depot": d.id, "depot_name": d.name, "name": x.name, "bus": x.bus }))).collect::<Vec<_>>(),
        // (step 5: the group's roles - its owner gives them)
        "is_owner": g.online.owner_id == me,
        "members": g.online.group.as_ref().map(|gr| gr.members.iter().map(|m| json!({ "player": m.player, "name": m.name, "role": m.role, "depot": m.depot, "depot_name": names.get(&m.depot).cloned().unwrap_or_default() })).collect::<Vec<_>>()).unwrap_or_default(),
        "players": around.players.iter().filter(|(id, _)| *id != g.online.owner_id).map(|(id, name)| json!({ "id": id, "name": name })).collect::<Vec<_>>(),
        "brand_behind": depots.iter().filter(|d| d.online.brand != g.online.group.as_ref().map_or(0, |x| x.brand)).map(|d| d.name.clone()).collect::<Vec<_>>(),
    })
}

/// May player `me` run head office `g`: its owner, and (step 5) its management.
pub fn may_run(g: &Company, me: &str) -> bool {
    !me.is_empty() && (g.online.owner_id == me || g.online.group.as_ref().is_some_and(|gr| gr.members.iter().any(|m| m.player == me && m.role == MANAGEMENT)))
}

/// What the head office's owner does on its page (`act` with its `args`; `by`: who), for the main
/// server to keep: money to a depot (`send {to, amount}`), a depot's rule (`rule {depot,
/// sweep_over, top_up_below}`), a loan (`borrow {amount}`) or a loan paid back early (`repay
/// {loan, amount}`), a depot's request answered (`answer {id, yes}`); (step 3) a depot's bus
/// moved to another (`move_bus {from, bus, to}`), its employee lent to one (`lend {from,
/// employee, to, days}`). What was done, for the log; an error: why not.
pub fn act(data: &Path, g: &mut Company, by: (&str, &str), what: &str, args: &Value, real: i64) -> Result<String, &'static str> {
    let (by_id, by) = by;
    let s = |k: &str| args.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let n = |k: &str| args.get(k).and_then(Value::as_i64);
    match what {
        "send" => {
            let (to, amount) = (s("to"), n("amount").unwrap_or(0));
            if !server_depots::depots_of(data, &g.id).iter().any(|d| d.id == to) {
                return Err(NOT_ITS_DEPOT);
            }
            server_depots::send(data, g, &to, amount, real)?;
            Ok(format!("{:.2} sent to {to}", amount as f64 / 100.0))
        }
        "rule" => {
            let depot = s("depot");
            set_rule(data, g, &depot, n("sweep_over"), n("top_up_below"))?;
            Ok(format!("the rule of {depot}: surplus over {:?}, topped up below {:?}", n("sweep_over"), n("top_up_below")))
        }
        "borrow" => {
            let amount = n("amount").unwrap_or(0);
            borrow(data, g, amount)?;
            Ok(format!("borrowed {:.2}", amount as f64 / 100.0))
        }
        "repay" => {
            let (id, amount) = (n("loan").unwrap_or(0) as u32, n("amount").unwrap_or(0));
            if !g.loans.iter().any(|l| l.id == id) {
                return Err(NO_SUCH_LOAN);
            }
            finance::repay(g, id, amount)?;
            Ok(format!("paid back {:.2} of loan {id}", amount as f64 / 100.0))
        }
        "answer" => {
            let yes = args.get("yes").and_then(Value::as_bool).unwrap_or(false);
            let a = server_depots::answer_approval(data, g, args.get("id").and_then(Value::as_u64).unwrap_or(0), yes)?;
            Ok(format!("{} {} up to {:.2}", if yes { "approved" } else { "refused" }, a.name, a.amount as f64 / 100.0))
        }
        "move_bus" | "lend" => {
            let (from, to) = (s("from"), s("to"));
            let o = Order { what: what.to_string(), bus: n("bus").unwrap_or(0) as u32, employee: n("employee").unwrap_or(0) as u32, to: to.clone(), days: n("days").unwrap_or(0), ..Default::default() };
            order(data, g, by, &from, o)?;
            Ok(format!("ordered {from}: {what} to {to}"))
        }
        // (step 4: the group's brand)
        "brand" => {
            let colours = args.get("colours").and_then(Value::as_array).cloned().unwrap_or_default();
            let c = |k: usize| colours.get(k).and_then(Value::as_str).unwrap_or("").to_string();
            set_brand(g, &s("name"), &s("short"), [&c(0), &c(1)])?;
            Ok(format!("the brand: {} ({}) {} {}", g.name, g.short, g.colours[0], g.colours[1]))
        }
        "mark_of" => {
            mark_of_depot(data, g, &s("depot"))?;
            Ok(format!("the mark of {}", s("depot")))
        }
        "share_livery" => {
            share_livery(data, g, &s("depot"), &s("name"))?;
            Ok(format!("livery {} of {} the group's", s("name"), s("depot")))
        }
        "unshare_livery" => {
            unshare_livery(g, &s("name"))?;
            Ok(format!("livery {} no longer the group's", s("name")))
        }
        // (step 5: the group's roles - its owner's)
        "role" => {
            if by_id != g.online.owner_id {
                return Err(NOT_OWNER);
            }
            set_member(data, g, &s("player"), &s("name"), &s("role"), &s("depot"))?;
            let role = s("role");
            Ok(format!("{} ({}): {} {}", s("name"), s("player"), if role.is_empty() { "no role" } else { &role }, s("depot")))
        }
        _ => Err(UNKNOWN_ACT),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::online;

    /// A server's folder with three maps and its clock.
    fn folder(name: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("openomsi-server-hq-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let root = dir.join("omsi");
        for m in ["Grundorf", "Ahlheim", "Spandau"] {
            std::fs::create_dir_all(root.join("maps").join(m)).unwrap();
            std::fs::write(root.join("maps").join(m).join("global.cfg"), "").unwrap();
        }
        let data = dir.join("bco");
        online::server_clock(&data, 1_800_000_000, false, ("2024-03-04".into(), 6 * 3600)).unwrap();
        (dir, root, data)
    }

    fn company(data: &Path, id: &str, name: &str, map: &str) -> Company {
        let mut c = super::super::found(&Founding { name: name.into(), short: "UMO".into(), date: "2024-03-04".into(), map: format!("maps/{map}/global.cfg"), ..Default::default() }, "x");
        c.id = id.into();
        c.online.on_server = true;
        c.online.owner = "Mathias".into();
        c.online.owner_id = "p1".into();
        c.cash += 2_000_000_00;
        store::save(data, &c).unwrap();
        c
    }

    /// A company on its own founds its first depot: its group is made with it - a head office of
    /// its name with no map and no money, which no look into the companies takes for one - the
    /// company its first depot named by its map, the new depot tied to the head office too; the
    /// head office founds the next from its own money.
    #[test]
    fn the_first_depot_makes_a_group_with_its_head_office() {
        let (dir, root, data) = folder("first");
        let mut c = company(&data, "umove", "Umove", "Grundorf");
        let cash = c.cash;
        let id = server_depots::found(&data, &root, &mut c, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        store::save(&data, &c).unwrap();
        assert_eq!((c.name.as_str(), c.online.hq.as_str(), c.cash), ("Umove Grundorf", "umove.hq", cash - 350_000_00));
        let g = store::load(&data, "umove.hq").unwrap();
        assert!(is_group(&g) && g.map.is_empty() && g.cash == 0 && g.fleet.is_empty() && g.online.owner_id == "p1");
        assert_eq!((g.name.as_str(), g.short.as_str()), ("Umove", "UMO"));
        assert_eq!(group_ids(&data), ["umove.hq"]);
        assert!(!company_ids(&data).contains(&"umove.hq".to_string()), "no process, no tenders");
        assert!(store::list(&data, "").iter().all(|x| !is_group(x)));
        let d = store::load(&data, &id).unwrap();
        assert_eq!((d.name.as_str(), d.online.hq.as_str()), ("Umove Ahlheim", "umove.hq"));
        let mut names: Vec<String> = server_depots::depots_of(&data, "umove.hq").into_iter().map(|d| d.name).collect();
        names.sort();
        assert_eq!(names, ["Umove Ahlheim", "Umove Grundorf"]);
        // (a depot founds no depot; the head office does, from its treasury)
        assert_eq!(server_depots::found(&data, &root, &mut c, "Spandau", 0, 0, 1_800_000_000), Err(server_depots::DEPOT_OF_DEPOT));
        let mut g = g;
        assert_eq!(server_depots::found(&data, &root, &mut g, "Spandau", 0, 250_000_00, 1_800_000_000), Err(server_depots::NO_CASH));
        assert_eq!(server_depots::found(&data, &root, &mut g, "Ahlheim", 0, 0, 1_800_000_000), Err(server_depots::HAS_DEPOT));
        g.cash = 300_000_00;
        let third = server_depots::found(&data, &root, &mut g, "Spandau", 50_000_00, 250_000_00, 1_800_000_000).unwrap();
        assert_eq!(g.cash, 0);
        assert_eq!(store::load(&data, &third).unwrap().name, "Umove Spandau");
        assert_eq!(server_depots::depots_of(&data, "umove.hq").len(), 3);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// (Decided 26 October 2026) A head office of before phase 7 is split when the main server
    /// opens: its group's head office made, it a depot named by its map, its depots tied to the
    /// new head office, and the requests waiting for it the head office's - once.
    #[test]
    fn a_head_office_of_before_is_split_into_its_group() {
        let (dir, _root, data) = folder("split");
        let mut c = company(&data, "umove", "Umove", "Ahlheim");
        let mut d = company(&data, "umove-grundorf", "Umove Grundorf", "Grundorf");
        d.online.hq = "umove".into();
        store::save(&data, &d).unwrap();
        // (a request it has, and one in its file not taken yet)
        server_depots::ask_approval(&data, &mut d, 700_000_00, "two buses", 1_800_000_000).unwrap();
        server_depots::approvals_step(&data, &mut c);
        server_depots::ask_approval(&data, &mut d, 800_000_00, "two more", 1_800_000_100).unwrap();
        store::save(&data, &c).unwrap();
        store::save(&data, &d).unwrap();
        let lone = company(&data, "alone", "Alone", "Spandau");
        let cash = c.cash;
        let said = split_head_offices(&data);
        assert_eq!(said.len(), 1, "{said:?}");
        let g = store::load(&data, "umove.hq").unwrap();
        let c = store::load(&data, "umove").unwrap();
        let d = store::load(&data, "umove-grundorf").unwrap();
        assert_eq!((g.name.as_str(), c.name.as_str(), c.online.hq.as_str(), d.online.hq.as_str()), ("Umove", "Umove Ahlheim", "umove.hq", "umove.hq"));
        assert_eq!(g.online.asks.iter().map(|a| a.reason.as_str()).collect::<Vec<_>>(), ["two buses", "two more"]);
        assert!(c.online.asks.is_empty() && c.cash == cash && g.cash == 0, "the company keeps its money");
        let alone = store::load(&data, "alone").unwrap();
        assert!(alone.name == lone.name && alone.online.hq.is_empty() && group_ids(&data) == ["umove.hq"], "a company on its own stays one");
        assert!(split_head_offices(&data).is_empty(), "once");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The group's treasury: money sent to a depot and home again, a depot's surplus over its
    /// rule home at its day's close, a depot below its rule topped up - what is on its way to it
    /// counted, never more than the treasury has, never over the surplus it keeps.
    #[test]
    fn the_treasury_and_the_depots_rules() {
        let (dir, root, data) = folder("money");
        let mut c = company(&data, "umove", "Umove", "Grundorf");
        let id = server_depots::found(&data, &root, &mut c, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        store::save(&data, &c).unwrap();
        let mut g = store::load(&data, "umove.hq").unwrap();
        // (by hand: the depot sends home, the head office takes it at its step)
        server_depots::send(&data, &mut c, "umove.hq", 500_000_00, 1).unwrap();
        store::save(&data, &c).unwrap();
        assert!(step(&data, &mut g, "2024-03-04", 1).changed);
        assert_eq!(g.cash, 500_000_00);
        // (a rule: no top-up over the surplus kept; not for a stranger)
        assert_eq!(set_rule(&data, &mut g, &id, Some(100_000_00), Some(200_000_00)), Err(BAD_RULE));
        assert_eq!(set_rule(&data, &mut g, "other", None, Some(1)), Err(NOT_ITS_DEPOT));
        set_rule(&data, &mut g, &id, Some(300_000_00), Some(150_000_00)).unwrap();
        set_rule(&data, &mut g, &c.id, Some(1_000_000_00), None).unwrap();
        // (the depots read the rules from the head office's file)
        store::save(&data, &g).unwrap();
        // (the depot has 100 000: topped up to 150 000, once - what is on its way counts)
        assert!(top_ups(&data, &mut g, 2));
        assert_eq!(g.cash, 450_000_00);
        assert!(!top_ups(&data, &mut g, 3), "on its way");
        let mut d = store::load(&data, &id).unwrap();
        server_depots::take_transfers(&data, &mut d);
        assert_eq!(d.cash, 150_000_00);
        store::save(&data, &d).unwrap();
        assert!(!top_ups(&data, &mut g, 4));
        // (its surplus home at its day's close)
        d.cash = 420_000_00;
        assert_eq!(sweep(&data, &mut d, 5), 120_000_00);
        assert_eq!(d.cash, 300_000_00);
        assert_eq!(sweep(&data, &mut d, 6), 0);
        step(&data, &mut g, "2024-03-04", 7);
        assert_eq!(g.cash, 570_000_00);
        // (no more than the treasury has)
        d.cash = -1_000_000_00;
        store::save(&data, &d).unwrap();
        assert!(top_ups(&data, &mut g, 8));
        assert_eq!(g.cash, 0);
        // (the rule gone)
        set_rule(&data, &mut g, &id, None, None).unwrap();
        assert!(rule_of(&g, &id).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The group's books: the depots' months summed with the head office's, their money moved
    /// between them no part of the result; the bank lends on the whole group; the head office's
    /// days close on the server's - in the red the bank's emergency loan - and its loans' rates
    /// are paid at a new month.
    #[test]
    fn the_groups_books_its_loans_and_its_days() {
        let (dir, root, data) = folder("books");
        let mut c = company(&data, "umove", "Umove", "Grundorf");
        let id = server_depots::found(&data, &root, &mut c, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        c.book(BookingKind::Fares, 30_000_00, "fares", false);
        store::save(&data, &c).unwrap();
        let mut d = store::load(&data, &id).unwrap();
        d.book(BookingKind::Wages, -10_000_00, "wages", false);
        store::save(&data, &d).unwrap();
        let mut g = store::load(&data, "umove.hq").unwrap();
        let (all, depots) = consolidated(&data, &g);
        assert_eq!(depots.len(), 2);
        assert_eq!(all.month("2024-03").result(), c.month("2024-03").result() + d.month("2024-03").result());
        assert_eq!(all.cash, c.cash + d.cash);
        // (the bank: not under its least, not over the group's credit)
        assert_eq!(borrow(&data, &mut g, 5_000_00), Err(SMALL_LOAN));
        let left = credit(&data, &g).left;
        assert!(left >= finance::SMALLEST_LOAN, "{left}");
        assert_eq!(borrow(&data, &mut g, left + 1), Err(NO_LOAN));
        borrow(&data, &mut g, finance::SMALLEST_LOAN).unwrap();
        assert_eq!((g.cash, g.debt()), (finance::SMALLEST_LOAN, finance::SMALLEST_LOAN));
        // (its days to the server's: a new month pays the rates)
        let s = step(&data, &mut g, "2024-04-02", 9);
        assert_eq!((s.closed, s.bankrupt, g.date.as_str()), (29, false, "2024-04-02"));
        assert!(g.debt() < finance::SMALLEST_LOAN, "a rate paid");
        // (the treasury in the red, the group not: called in from the richest depot, the day fine)
        g.cash = -10_000_00;
        step(&data, &mut g, "2024-04-03", 10);
        assert!(!server_depots::emergency_running(&g) && g.online.red_days == 0);
        assert!(g.inbox.messages.iter().any(|m| m.title == CALLED_IN.0));
        let mut c = store::load(&data, "umove").unwrap();
        assert_eq!(orders_waiting(&data, &c).iter().map(|o| (o.what.as_str(), o.amount)).collect::<Vec<_>>(), [("send_home", 10_000_00)]);
        step(&data, &mut g, "2024-04-04", 11);
        assert_eq!(orders_waiting(&data, &c).len(), 1, "what is called in already not called again");
        assert!(take_orders(&data, &mut c));
        store::save(&data, &c).unwrap();
        step(&data, &mut g, "2024-04-04", 12);
        assert_eq!(g.cash, 0, "sent home");
        // (the whole group in the red: the bank's emergency loan)
        for id in ["umove", id.as_str()] {
            let mut x = store::load(&data, id).unwrap();
            x.cash = -5_000_000_00;
            store::save(&data, &x).unwrap();
        }
        g.cash = -10_000_00;
        step(&data, &mut g, "2024-04-05", 13);
        assert!(server_depots::emergency_running(&g) && g.cash > 0);
        let view = view(&data, &g, "p1", &Around { running: vec![id.clone()], depot_price: 250_000_00, maps: vec!["Spandau".into()], time: "06:00".into(), bus_move: (5000_00, 2), players: Vec::new() });
        assert_eq!((view["depots"].as_array().unwrap().len(), view["mine"].as_bool()), (2, Some(true)));
        assert!(view["depots"].as_array().unwrap().iter().any(|x| x["runs"] == json!(true)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The owner's acts on the page: money to a depot (not to a stranger), a loan paid back, a
    /// depot's request answered; anything else not understood.
    #[test]
    fn the_owners_acts() {
        let (dir, root, data) = folder("acts");
        let mut c = company(&data, "umove", "Umove", "Grundorf");
        let id = server_depots::found(&data, &root, &mut c, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        store::save(&data, &c).unwrap();
        let mut g = store::load(&data, "umove.hq").unwrap();
        g.cash = 200_000_00;
        assert!(may_run(&g, "p1") && !may_run(&g, "p2") && !may_run(&g, ""));
        act(&data, &mut g, ("p1", "Mathias"), "send", &json!({ "to": id, "amount": 50_000_00 }), 1).unwrap();
        assert_eq!(g.cash, 150_000_00);
        assert_eq!(act(&data, &mut g, ("p1", "Mathias"), "send", &json!({ "to": "other", "amount": 50_000_00 }), 1), Err(NOT_ITS_DEPOT));
        act(&data, &mut g, ("p1", "Mathias"), "borrow", &json!({ "amount": finance::SMALLEST_LOAN }), 2).unwrap();
        let loan = g.loans[0].id;
        act(&data, &mut g, ("p1", "Mathias"), "repay", &json!({ "loan": loan, "amount": 5_000_00 }), 2).unwrap();
        assert_eq!(act(&data, &mut g, ("p1", "Mathias"), "repay", &json!({ "loan": 99, "amount": 5_000_00 }), 2), Err(NO_SUCH_LOAN));
        let mut d = store::load(&data, &id).unwrap();
        server_depots::ask_approval(&data, &mut d, 700_000_00, "buses", 1_800_000_000).unwrap();
        step(&data, &mut g, "2024-03-04", 3);
        let ask = g.online.asks[0].id;
        act(&data, &mut g, ("p1", "Mathias"), "answer", &json!({ "id": ask, "yes": true }), 4).unwrap();
        assert!(g.online.asks.is_empty());
        assert_eq!(act(&data, &mut g, ("p1", "Mathias"), "nothing", &json!({}), 5), Err(UNKNOWN_ACT));
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// (Step 3) The head office orders a depot: its bus to another depot, its driver lent to one -
    /// done by the depot at its next step and told in its inbox, once; an order it can no longer
    /// do told to the head office; no order to the depot itself, nor to a stranger.
    #[test]
    fn the_head_office_moves_buses_and_lends_staff_between_its_depots() {
        use super::super::market::{self, MarketBus, Payment};
        use super::super::model::{BusKind, BusSize, Drive};
        let (dir, root, data) = folder("orders");
        let mut c = company(&data, "umove", "Umove", "Grundorf");
        let id = server_depots::found(&data, &root, &mut c, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        c.online.big_purchase = Some(Cents::MAX / 4);
        let b = MarketBus { file: "Vehicles/MB_Citaro/Citaro.bus".into(), name: "Citaro".into(), kind: BusKind { size: BusSize::Solo, drive: Drive::Diesel }, ..Default::default() };
        let bus = market::buy_new(&mut c, &b, Payment::Cash, "").unwrap();
        let other_bus = market::buy_new(&mut c, &b, Payment::Cash, "").unwrap();
        let a = super::super::staff::applicants(&c).into_iter().find(|a| a.role == super::super::model::Role::Driver).expect("a driver");
        let man = super::super::staff::hire(&mut c, &a).unwrap();
        store::save(&data, &c).unwrap();
        let mut g = store::load(&data, "umove.hq").unwrap();
        let order_of = |what: &str, to: &str| json!({ "from": "umove", "bus": bus, "employee": man, "to": to, "days": 3, "do": what });
        assert_eq!(act(&data, &mut g, ("p1", "Mathias"), "move_bus", &order_of("move_bus", "umove"), 1), Err(SAME_DEPOT));
        assert_eq!(act(&data, &mut g, ("p1", "Mathias"), "move_bus", &order_of("move_bus", "other"), 1), Err(NOT_ITS_DEPOT));
        assert_eq!(act(&data, &mut g, ("p1", "Mathias"), "move_bus", &json!({ "from": "umove", "bus": 4711, "to": id }), 1), Err(server_depots::NO_SUCH_BUS));
        act(&data, &mut g, ("p1", "Mathias"), "move_bus", &order_of("move_bus", &id), 1).unwrap();
        act(&data, &mut g, ("p1", "Mathias"), "lend", &order_of("lend", &id), 1).unwrap();
        assert_eq!(orders_waiting(&data, &c).len(), 2);
        let v = view(&data, &g, "p1", &Around { running: vec![], depot_price: 0, maps: vec![], time: String::new(), bus_move: (5000_00, 2), players: Vec::new() });
        assert_eq!(v["fleet"].as_array().unwrap().iter().map(|d| d["orders"].as_array().unwrap().len()).sum::<usize>(), 2);
        // (the depot does them at its next step: the bus on its way, the driver to go tomorrow)
        let n = c.fleet.len();
        assert!(take_orders(&data, &mut c));
        assert!(!take_orders(&data, &mut c), "once");
        assert_eq!(c.fleet.len(), n - 1);
        assert!(c.staff.iter().find(|e| e.id == man).is_some_and(|e| e.lend.is_some()));
        assert!(c.inbox.messages.iter().any(|m| m.title == ORDER_MOVED.0) && c.inbox.messages.iter().any(|m| m.title == ORDER_LENT.0));
        assert_eq!(server_depots::buses_on_the_way(&data, &c).len(), 1);
        // (one it can no longer do - the bus sold meanwhile - told to the head office)
        store::save(&data, &c).unwrap();
        act(&data, &mut g, ("p1", "Mathias"), "move_bus", &json!({ "from": "umove", "bus": other_bus, "to": id }), 2).unwrap();
        c.fleet.retain(|v| v.id != other_bus);
        assert!(take_orders(&data, &mut c));
        step(&data, &mut g, "2024-03-04", 3);
        assert!(g.inbox.messages.iter().any(|m| m.title == ORDER_FAILED.0));
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// (Step 4) The group's brand: its name, short name and colours set at the head office, or a
    /// depot's mark made the group's; a depot's livery shared - every depot takes them at its next
    /// look, named "<group> <map>", the livery's design not paid again - once.
    #[test]
    fn the_groups_brand_reaches_every_depot() {
        use super::super::livery::Design;
        let (dir, root, data) = folder("brand");
        let mut c = company(&data, "umove", "Umove", "Grundorf");
        let id = server_depots::found(&data, &root, &mut c, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        c.designs.push(Design { name: "Umove Orange".into(), bus: "Vehicles/MB_Citaro/Citaro.bus".into(), made: c.date.clone(), paid: 2_500_00 });
        store::save(&data, &c).unwrap();
        let mut g = store::load(&data, "umove.hq").unwrap();
        assert_eq!(set_brand(&mut g, "Umove", "U", ["#ff0000", "#000000"]), Err(BAD_SHORT));
        assert_eq!(set_brand(&mut g, "Umove", "UMV", ["red", "#000000"]), Err(BAD_COLOUR));
        assert_eq!(set_brand(&mut g, " ", "UMV", ["#ff0000", "#000000"]), Err(NO_NAME));
        act(&data, &mut g, ("p1", "Mathias"), "brand", &json!({ "name": "Umove Gruppe", "short": "umv", "colours": ["#FF0000", "000000"] }), 1).unwrap();
        assert_eq!((g.name.as_str(), g.short.as_str(), g.colours[0].as_str(), g.colours[1].as_str()), ("Umove Gruppe", "UMV", "#ff0000", "#000000"));
        assert_eq!(share_livery(&data, &mut g, &id, "Umove Orange"), Err(NO_SUCH_DESIGN));
        act(&data, &mut g, ("p1", "Mathias"), "share_livery", &json!({ "depot": "umove", "name": "umove orange" }), 2).unwrap();
        store::save(&data, &g).unwrap();
        let mut d = store::load(&data, &id).unwrap();
        assert!(take_group(&data, &mut d));
        assert!(!take_group(&data, &mut d), "once");
        assert_eq!((d.name.as_str(), d.short.as_str(), d.colours[0].as_str()), ("Umove Gruppe Ahlheim", "UMV", "#ff0000"));
        assert_eq!(d.designs.iter().map(|x| (x.name.as_str(), x.paid)).collect::<Vec<_>>(), [("Umove Orange", 0)]);
        assert!(d.inbox.messages.iter().any(|m| m.title == BRAND_TAKEN.0));
        // (a depot's mark the group's; the depot that made the livery keeps it once)
        d.short = "UA".into();
        d.logo = Some("logo.png".into());
        store::save(&data, &d).unwrap();
        act(&data, &mut g, ("p1", "Mathias"), "mark_of", &json!({ "depot": id }), 3).unwrap();
        store::save(&data, &g).unwrap();
        assert!(take_group(&data, &mut c));
        assert_eq!((c.short.as_str(), c.logo.as_deref(), c.name.as_str()), ("UA", Some("logo.png"), "Umove Gruppe Grundorf"));
        assert_eq!(c.designs.len(), 1, "its own livery not twice");
        store::save(&data, &c).unwrap();
        let v = view(&data, &g, "p1", &Around { running: vec![], depot_price: 0, maps: vec![], time: String::new(), bus_move: (0, 0), players: vec![("p1".into(), "Mathias".into()), ("p2".into(), "Luc".into())] });
        assert_eq!((v["liveries"].as_array().unwrap().len(), v["brand_behind"].as_array().unwrap().len()), (1, 1));
        unshare_livery(&mut g, "Umove Orange").unwrap();
        assert!(g.designs.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// (Step 5) The group's roles, its owner's to give: its management runs the head office and
    /// is a co-owner of every depot, a depot's manager of his depot - each depot takes them at
    /// its next look, and makes a driver again who the group no longer makes a co-owner.
    #[test]
    fn the_groups_roles_reach_its_depots() {
        use super::super::together::Role;
        let (dir, root, data) = folder("roles");
        let mut c = company(&data, "umove", "Umove", "Grundorf");
        let id = server_depots::found(&data, &root, &mut c, "Ahlheim", 100_000_00, 250_000_00, 1_800_000_000).unwrap();
        store::save(&data, &c).unwrap();
        let mut g = store::load(&data, "umove.hq").unwrap();
        let role = |player: &str, name: &str, role: &str, depot: &str| json!({ "player": player, "name": name, "role": role, "depot": depot });
        assert_eq!(act(&data, &mut g, ("p2", "Luc"), "role", &role("p3", "Jens", MANAGEMENT, ""), 1), Err(NOT_OWNER));
        assert_eq!(act(&data, &mut g, ("p1", "Mathias"), "role", &role("p1", "Mathias", MANAGEMENT, ""), 1), Err(IS_OWNER));
        assert_eq!(act(&data, &mut g, ("p1", "Mathias"), "role", &role("p2", "Luc", "boss", ""), 1), Err(BAD_ROLE));
        assert_eq!(act(&data, &mut g, ("p1", "Mathias"), "role", &role("p2", "Luc", DEPOT_MANAGER, "other"), 1), Err(NOT_ITS_DEPOT));
        assert!(!may_run(&g, "p2"));
        act(&data, &mut g, ("p1", "Mathias"), "role", &role("p2", "Luc", MANAGEMENT, ""), 1).unwrap();
        act(&data, &mut g, ("p1", "Mathias"), "role", &role("p3", "Jens", DEPOT_MANAGER, &id), 1).unwrap();
        assert!(may_run(&g, "p2") && !may_run(&g, "p3"));
        store::save(&data, &g).unwrap();
        // (each depot takes them: the management everywhere, the manager at his depot)
        let mut d = store::load(&data, &id).unwrap();
        assert!(take_group(&data, &mut d));
        assert!(!take_group(&data, &mut d), "once");
        let role_of = |c: &Company, id: &str| c.together.guests.iter().find(|x| x.id == id).map(|x| x.role);
        assert_eq!((role_of(&d, "p2"), role_of(&d, "p3")), (Some(Role::CoOwner), Some(Role::CoOwner)));
        assert!(take_group(&data, &mut c));
        assert_eq!((role_of(&c, "p2"), role_of(&c, "p3")), (Some(Role::CoOwner), None));
        // (a new manager of the depot: the one before a driver again there)
        act(&data, &mut g, ("p1", "Mathias"), "role", &role("p4", "Ole", DEPOT_MANAGER, &id), 2).unwrap();
        act(&data, &mut g, ("p1", "Mathias"), "role", &role("p2", "Luc", "", ""), 2).unwrap();
        store::save(&data, &g).unwrap();
        assert!(take_group(&data, &mut d));
        assert_eq!((role_of(&d, "p2"), role_of(&d, "p3"), role_of(&d, "p4")), (Some(Role::Driver), Some(Role::Driver), Some(Role::CoOwner)));
        assert!(!may_run(&g, "p2"));
        let v = view(&data, &g, "p1", &Around { running: vec![], depot_price: 0, maps: vec![], time: String::new(), bus_move: (0, 0), players: vec![("p1".into(), "Mathias".into()), ("p4".into(), "Ole".into())] });
        assert_eq!((v["is_owner"].as_bool(), v["members"].as_array().unwrap().len(), v["players"].as_array().unwrap().len()), (Some(true), 1, 1));
        let _ = std::fs::remove_dir_all(&dir);
    }
    /// (The review of 26 October 2026) A head office of before in the red: its deficit and the
    /// bank's emergency loan the group's treasury's at the split; a split cut short is done again
    /// with the same head office, once. No bus nor employee goes to a head office, a depot's start
    /// money cannot overflow, and a company removed takes what was sent to it along: its id
    /// founded again starts afresh.
    #[test]
    fn the_split_takes_the_red_along_and_mistakes_are_refused() {
        let (dir, root, data) = folder("redsplit");
        let mut c = company(&data, "umove", "Umove", "Ahlheim");
        let mut d = company(&data, "umove-grundorf", "Umove Grundorf", "Grundorf");
        d.online.hq = "umove".into();
        store::save(&data, &d).unwrap();
        c.cash = -20_000_00;
        super::super::finance::emergency_loan(&mut c, 30_000_00, 0.1, 12, server_depots::EMERGENCY_PURPOSE);
        c.cash = -20_000_00;
        store::save(&data, &c).unwrap();
        // (another, cut short before: its head office kept, the company not)
        let b = company(&data, "bravo", "Bravo", "Spandau");
        let mut bd = company(&data, "bravo-ahlheim", "Bravo Ahlheim", "Ahlheim");
        bd.online.hq = "bravo".into();
        store::save(&data, &bd).unwrap();
        let mut half = b.clone();
        assert_eq!(make_group(&data, &mut half).unwrap().id, "bravo.hq");
        let said = split_head_offices(&data);
        assert_eq!(said.len(), 2, "{said:?}");
        assert_eq!(group_ids(&data), ["bravo.hq", "umove.hq"], "the same head office again");
        assert_eq!(store::load(&data, "bravo-ahlheim").unwrap().online.hq, "bravo.hq");
        let g = store::load(&data, "umove.hq").unwrap();
        let c = store::load(&data, "umove").unwrap();
        assert_eq!((c.cash, c.online.hq.as_str(), server_depots::emergency_running(&c)), (0, "umove.hq", false));
        assert!(server_depots::emergency_running(&g) && g.cash == -20_000_00, "{}", g.cash);
        assert!(split_head_offices(&data).is_empty(), "once");
        // (no bus, no employee to a head office; no overflow of a depot's start)
        let mut c = c;
        assert_eq!(server_depots::move_bus(&data, &mut c, 1, "umove.hq", 0, 1), Err(server_depots::NOT_RELATED));
        assert_eq!(server_depots::lend(&data, &mut c, 1, "umove.hq", 3), Err(server_depots::NOT_RELATED));
        let mut g = g;
        g.cash = 0;
        assert_eq!(server_depots::found(&data, &root, &mut g, "Spandau", Cents::MAX, 250_000_00, 1), Err(server_depots::NO_CASH));
        // (removed, the files that name it go with it)
        c.cash = 1_000_00;
        server_depots::send(&data, &mut c, "umove-grundorf", 100_00, 1).unwrap();
        assert!(server_depots::transfers_file(&data, "umove-grundorf").exists());
        store::delete(&data, "umove-grundorf").unwrap();
        assert!(!server_depots::transfers_file(&data, "umove-grundorf").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

//! (NexiconDK, ticket inspection round V, 12 November 2026) A player as one of his bus company's
//! ticket inspectors in a map's world of Bus Company Online (`--patrol`).
//!
//! His game starts on foot at a stop of the patrol's line (`duty_start::place_on_patrol`), his
//! walker in the company's inspector's uniform - the others draw him in it too (`INFO`'s figure
//! carries the clothes, `omsi_net::figure_clothes`). He walks into a bus (G by its door) and K by
//! a rider checks their ticket: the nearest rider not checked yet (`PeopleSim::nearest_rider`).
//! Whether they have one is the word of the game that runs them (`PeopleSim::check_ticket`),
//! asked over the session's commands (`LanSession::command`):
//!
//! ```text
//! ticket patrol <company>;<line>      inspector → host, every `HELLO_EVERY` s: on patrol
//! ticket check <person>               inspector → host: that rider (the id his game knows)
//! ticket ask <inspector>;<pid>;<id>   host → the driver whose bus the rider is in
//! ticket told <inspector>;<id>;<r>    that driver → host
//! ticket is <person>;<r>              host → inspector: valid, caught, checked, gone
//! ticket no <person>                  host → inspector: the world knows no patrol of his
//! ticket end                          inspector → host: his patrol is over
//! ```
//!
//! The host - the map's world - keeps what each inspector found (`HostBook`): every bus he
//! checked in with its line, the riders checked and caught, and his time on patrol, written for
//! his company (`company_server::patrol_seen`) as he leaves a bus, every half hour, and when he
//! goes. His company books the checks and pays his time (`inspection_patrol::book_seen`).

use crate::humans::BusId;
use crate::App;
use omsi_sim::people::ticket::Ticket;
use std::collections::{HashMap, HashSet};

/// The share of a timetable bus's riders without a ticket (as the company's model reckons on
/// an unchecked line, `dodgers::BASE`).
pub(crate) const AI_SHARE: f32 = 0.08;
/// How near a rider must be (m) for K to check them.
const REACH: f32 = 2.6;
/// Seconds an answer is waited for, and between two hellos to the host.
const WAIT: f32 = 5.0;
const HELLO_EVERY: f32 = 20.0;
/// Seconds out of a bus before what he found in it is told.
const OUT_FOR: f32 = 2.5;
/// Host: an inspector unheard of this long is gone (s); a bus not checked in this long is left
/// (s); his time is written this often (s).
const GONE_AFTER: f64 = 120.0;
const RIDE_IDLE: f64 = 600.0;
const TIME_EVERY: f64 = 1800.0;
/// The id a client's person has in the other games from here up (`lan_world::RELAYED`).
const RELAYED_FROM: u32 = 0xC0_0000;

/// The patrol as the launcher gave it (`--patrol <company>|<line>|<tour>|<until>|<fine>|<clothes>`:
/// `until` minutes of the day, `fine` as the company writes it).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Spec {
    pub company: String,
    pub line: String,
    pub tour: String,
    pub until: i32,
    pub fine: String,
    pub clothes: String,
}

impl Spec {
    pub(crate) fn parse(s: &str) -> Option<Spec> {
        let f: Vec<&str> = s.split('|').map(str::trim).collect();
        let spec = Spec {
            company: f.first()?.to_string(),
            line: f.get(1)?.to_string(),
            tour: f.get(2).unwrap_or(&"").to_string(),
            until: f.get(3).and_then(|x| x.parse().ok()).unwrap_or(24 * 60),
            fine: f.get(4).unwrap_or(&"").to_string(),
            clothes: f.get(5).filter(|c| c.starts_with("~Inspector_")).unwrap_or(&"").to_string(),
        };
        (!spec.company.is_empty() && !spec.line.is_empty()).then_some(spec)
    }
}

/// What he found in the bus he is in.
#[derive(Debug, Clone, Default)]
struct Ride {
    bus: Option<BusId>,
    checked: u32,
    caught: u32,
}

/// The inspector's side (his game), and the host's book of the inspectors (the world).
#[derive(Default)]
pub(crate) struct PatrolState {
    pub(crate) spec: Option<Spec>,
    read: bool,
    hello: f32,
    /// Riders asked about (their ids) and the seconds since.
    pending: Vec<(u32, f32)>,
    /// The riders of the bus he is in already checked or asked about.
    asked: HashSet<u32>,
    ride: Ride,
    out: f32,
    pub(crate) checked: u32,
    pub(crate) caught: u32,
    over: bool,
    /// The drivers' games told (`ticket ask`): the inspectors in their bus, said once each.
    told_driver: HashSet<u32>,
    pub(crate) host: HostBook,
}

impl PatrolState {
    /// On patrol (his game).
    pub(crate) fn on(&self) -> bool {
        self.spec.is_some()
    }

    /// The clothes his walker wears (empty: none).
    pub(crate) fn clothes(&self) -> String {
        self.spec.as_ref().map(|s| s.clothes.clone()).unwrap_or_default()
    }
}

fn tr(s: &str) -> String {
    omsi_ui::tr(s).into_owned()
}

fn hhmm(m: i32) -> String {
    format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(60))
}

impl App {
    /// The figure the others draw him as, with the uniform after it (`INFO`).
    pub(crate) fn patrol_figure(&self) -> Option<std::path::PathBuf> {
        let clothes = self.net.patrol.clothes();
        if clothes.is_empty() {
            return None;
        }
        let f = self.session.on_foot.as_ref()?;
        let h = self.session.humans.as_ref()?;
        let n = h.types.len().max(1) as u64;
        let t = h.types.get((f.kind % n) as usize)?;
        Some(std::path::PathBuf::from(format!("{}#{clothes}", t.def.path.display())))
    }

    /// On foot at the patrol's stop (`args.patrol_at`), in the uniform.
    pub(crate) fn patrol_begin(&mut self, at: Option<[f64; 4]>) {
        let Some(spec) = self.args.patrol.as_deref().and_then(Spec::parse) else { return };
        if let Some(at) = at {
            let z = self.world.as_ref().and_then(|w| w.walk_height_near(at[0], at[1], at[2])).unwrap_or(at[2]);
            self.start_on_foot(glam::DVec3::new(at[0], at[1], z), at[3]);
        }
        if !spec.clothes.is_empty() {
            let prefer = self.session.on_foot.as_ref().map(|f| f.kind);
            let n = self.session.humans.as_ref().map(|h| h.types.len().max(1) as u64).unwrap_or(1);
            let kind = self.session.humans.as_ref().and_then(|h| h.type_wearing(&spec.clothes, prefer.map(|k| (k % n) as usize)));
            if let Some(f) = self.session.on_foot.as_mut() {
                if let Some(k) = kind {
                    f.kind = k as u64;
                }
                f.clothes = spec.clothes.clone();
            }
        }
        log::info!("patrol: ticket inspection on line {} for {} until {}", spec.line, spec.company, hhmm(spec.until));
        self.service_msg = Some((tr("Ticket inspection on line %{line} until %{time}: get into a bus, then K by a passenger checks the ticket.").replace("%{line}", &spec.line).replace("%{time}", &hhmm(spec.until)), 12.0));
        self.net.patrol.spec = Some(spec);
        self.net.patrol.read = true;
    }

    /// K on foot: the ticket of the rider nearest him in the bus he is in.
    pub(crate) fn patrol_check(&mut self) {
        if !self.net.patrol.on() {
            log::info!("patrol: K, but no ticket-inspection duty (--patrol {:?})", self.args.patrol);
            return;
        }
        if self.net.patrol.over {
            self.service_msg = Some((tr("Your ticket-inspection duty is over."), 5.0));
            return;
        }
        let aboard = self.session.humans.as_ref().and_then(|h| h.avatar_aboard(crate::on_foot::AVATAR_KEY));
        log::info!("patrol: K - the walker aboard {:?}, inside {:?}, seated {:?}", aboard.map(|a| a.0), self.session.on_foot.as_ref().and_then(|f| f.inside.map(|x| x.0)), self.session.on_foot.as_ref().and_then(|f| f.seat.map(|x| x.0)));
        let Some((bus, at)) = aboard else {
            self.service_msg = Some((tr("Get into a bus first: K by a passenger checks the ticket."), 4.0));
            return;
        };
        if self.net.patrol.ride.bus != Some(bus) {
            self.patrol_ride_over();
            self.net.patrol.ride.bus = Some(bus);
        }
        let but: Vec<u32> = self.net.patrol.asked.iter().copied().collect();
        let Some(id) = self.session.humans.as_ref().and_then(|h| h.nearest_rider(bus, at, REACH, &but)) else {
            self.service_msg = Some((tr("Nobody to check within reach: walk up to a passenger."), 3.0));
            return;
        };
        self.net.patrol.asked.insert(id);
        let client = self.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client);
        if client {
            if let Some(l) = self.net.lan.as_mut() {
                l.command(1, &format!("ticket check {id}"));
            }
            self.net.patrol.pending.push((id, 0.0));
            self.service_msg = Some((tr("Ticket, please…"), 2.0));
        } else {
            let r = self.session.humans.as_mut().map(|h| h.check_ticket(id, AI_SHARE)).unwrap_or(Ticket::Gone);
            self.patrol_result(id, r);
        }
    }

    /// What checking rider `id` found, said and counted.
    fn patrol_result(&mut self, id: u32, r: Ticket) {
        let p = &mut self.net.patrol;
        p.pending.retain(|x| x.0 != id);
        let fine = p.spec.as_ref().map(|s| s.fine.clone()).unwrap_or_default();
        match r {
            Ticket::Valid => {
                p.ride.checked += 1;
                p.checked += 1;
            }
            Ticket::Caught => {
                p.ride.checked += 1;
                p.ride.caught += 1;
                p.checked += 1;
                p.caught += 1;
            }
            Ticket::Checked | Ticket::Gone => {}
        }
        let (n, c) = (p.ride.checked.to_string(), p.ride.caught.to_string());
        let text = match r {
            Ticket::Valid => tr("Valid ticket. (%{n} checked in this bus)").replace("%{n}", &n),
            Ticket::Caught => tr("No ticket! A penalty fare of %{fine} is written. (%{n} checked, %{c} without a ticket)").replace("%{fine}", &fine).replace("%{n}", &n).replace("%{c}", &c),
            Ticket::Checked => tr("Checked already."),
            Ticket::Gone => tr("That passenger is no longer there."),
        };
        self.service_msg = Some((text, 4.0));
    }

    /// Out of the bus: what he found in it said, and the next bus starts afresh.
    fn patrol_ride_over(&mut self) {
        let p = &mut self.net.patrol;
        if p.ride.checked > 0 {
            self.service_msg = Some((tr("In this bus: %{n} checked, %{c} without a ticket. Today: %{t} checked, %{a} without.").replace("%{n}", &p.ride.checked.to_string()).replace("%{c}", &p.ride.caught.to_string()).replace("%{t}", &p.checked.to_string()).replace("%{a}", &p.caught.to_string()), 6.0));
        }
        p.ride = Ride::default();
        p.asked.clear();
        p.pending.clear();
    }

    /// Once a frame (his game): the hello to the host, answers that did not come, out of a bus,
    /// the patrol's end.
    pub(crate) fn patrol_tick(&mut self, dt: f32) {
        // (no stop to start at: on foot where the game started him, once he is)
        if !self.net.patrol.read {
            if self.args.patrol.is_none() || self.args.patrol_at.is_some() || self.session.on_foot.is_none() {
                return;
            }
            self.patrol_begin(None);
            self.net.patrol.read = true;
        }
        let Some(spec) = self.net.patrol.spec.clone() else { return };
        let client = self.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client);
        let p = &mut self.net.patrol;
        p.hello -= dt;
        if client && p.hello <= 0.0 && !p.over {
            p.hello = HELLO_EVERY;
            if let Some(l) = self.net.lan.as_mut() {
                l.command(1, &format!("ticket patrol {};{}", spec.company, spec.line));
            }
        }
        let p = &mut self.net.patrol;
        let mut lost = Vec::new();
        for x in p.pending.iter_mut() {
            x.1 += dt;
            if x.1 > WAIT {
                lost.push(x.0);
            }
        }
        if !lost.is_empty() {
            p.pending.retain(|x| x.1 <= WAIT);
            for id in &lost {
                p.asked.remove(id);
            }
            self.service_msg = Some((tr("No answer about that ticket: try again."), 3.0));
        }
        let aboard = self.session.humans.as_ref().and_then(|h| h.avatar_aboard(crate::on_foot::AVATAR_KEY)).map(|x| x.0);
        let p = &mut self.net.patrol;
        if p.ride.bus.is_some() && aboard != p.ride.bus {
            p.out += dt;
            if p.out > OUT_FOR {
                p.out = 0.0;
                self.patrol_ride_over();
            }
        } else {
            self.net.patrol.out = 0.0;
        }
        // (the end of his duty: past midnight a duty ending after 24:00 goes on)
        let now = (self.clock.time / 60.0) as i32;
        let mut left = spec.until - now;
        if left < -720 {
            left += 1440;
        }
        if left <= 0 && !self.net.patrol.over {
            self.net.patrol.over = true;
            if let Some(l) = self.net.lan.as_mut() {
                l.command(1, "ticket end");
            }
            self.service_msg = Some((tr("Your ticket-inspection duty is over: thank you. You may end the game now."), 12.0));
        }
    }

    /// A `ticket` command for his game, or for a driver's game (not the host's: `HostBook`).
    pub(crate) fn ticket_client_command(&mut self, from: u32, rest: &str) {
        if from != 1 {
            return;
        }
        let (verb, args) = rest.split_once(' ').unwrap_or((rest, ""));
        let f: Vec<&str> = args.split(';').map(str::trim).collect();
        match verb {
            "is" => {
                let (Some(id), Some(r)) = (f.first().and_then(|x| x.parse::<u32>().ok()), f.get(1).and_then(|x| Ticket::of(x))) else { return };
                if self.net.patrol.pending.iter().any(|x| x.0 == id) {
                    self.patrol_result(id, r);
                }
            }
            "no" => {
                self.net.patrol.pending.clear();
                self.service_msg = Some((tr("The world does not know your ticket-inspection duty: start it again from the launcher."), 6.0));
            }
            // (a driver: an inspector checks a rider of his bus - his game says whether they have
            // a ticket)
            "ask" => {
                let (Some(insp), Some(pid), Some(id)) = (f.first().and_then(|x| x.parse::<u32>().ok()), f.get(1).and_then(|x| x.parse::<u32>().ok()), f.get(2).and_then(|x| x.parse::<u32>().ok())) else { return };
                let r = match self.session.humans.as_mut() {
                    Some(h) => match h.riders_of(BusId::Player).into_iter().find(|p| crate::lan_world::wire_up(*p) == pid) {
                        Some(local) => h.check_ticket(local, AI_SHARE),
                        None => Ticket::Gone,
                    },
                    None => Ticket::Gone,
                };
                if let Some(l) = self.net.lan.as_mut() {
                    l.command(1, &format!("ticket told {insp};{id};{}", r.key()));
                }
                if self.net.patrol.told_driver.insert(insp) {
                    let name = self.net.lan.as_ref().and_then(|l| l.peers().find(|p| p.pose.id == insp).map(|p| p.pose.name.clone())).unwrap_or_default();
                    self.service_msg = Some((tr("Ticket inspector %{name} checks the tickets in your bus.").replace("%{name}", &name), 6.0));
                }
            }
            _ => {}
        }
    }
}

// --- the host: the world's book of its inspectors ---------------------------------------------

/// A bus an inspector checks in (the host's): its line and what he found.
#[derive(Debug, Clone, Default)]
struct HostRide {
    /// The bus: a timetable bus of the host's (`false`, its id) or a player's (`true`, his id).
    key: (bool, u64),
    line: String,
    checked: u32,
    caught: u32,
    minute: i32,
    last: f64,
}

/// An inspector on patrol, as the host keeps him.
#[derive(Debug, Clone, Default)]
struct HostInspector {
    name: String,
    company: String,
    line: String,
    /// Since when his time is not written yet, and when he was last heard (Unix seconds).
    since: f64,
    last: f64,
    ride: Option<HostRide>,
}

/// What the host writes for an inspector's company: `kind` "ride" or "time".
pub(crate) type Seen = omsi_launcher_lib::company::inspection_patrol::PatrolSeen;

/// The world's inspectors on patrol.
#[derive(Default)]
pub(crate) struct HostBook {
    by: HashMap<u32, HostInspector>,
    /// What to write for their companies (`take_seen`): the player's name and what he did.
    out: Vec<(String, Seen)>,
}

fn unix_now() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

impl HostBook {
    fn flush_ride(&mut self, id: u32) {
        let Some(i) = self.by.get_mut(&id) else { return };
        if let Some(r) = i.ride.take().filter(|r| r.checked > 0) {
            self.out.push((i.name.clone(), Seen { kind: "ride".into(), company: i.company.clone(), line: r.line, checked: r.checked, caught: r.caught, minute: r.minute, ..Default::default() }));
        }
    }

    fn flush_time(&mut self, id: u32) {
        let Some(i) = self.by.get_mut(&id) else { return };
        let minutes = ((i.last - i.since) / 60.0).max(0.0);
        if minutes >= 1.0 {
            self.out.push((i.name.clone(), Seen { kind: "time".into(), company: i.company.clone(), line: i.line.clone(), minutes, ..Default::default() }));
            i.since = i.last;
        }
    }

    fn count(&mut self, id: u32, key: (bool, u64), line: String, r: Ticket, minute: i32) {
        if self.by.get(&id).is_some_and(|i| i.ride.as_ref().is_some_and(|x| x.key != key)) {
            self.flush_ride(id);
        }
        let Some(i) = self.by.get_mut(&id) else { return };
        let now = unix_now();
        let ride = i.ride.get_or_insert_with(|| HostRide { key, line, minute, ..Default::default() });
        ride.last = now;
        match r {
            Ticket::Valid => ride.checked += 1,
            Ticket::Caught => {
                ride.checked += 1;
                ride.caught += 1;
            }
            _ => {}
        }
    }

    /// A `ticket` command from player `from` (the host's): the answers to send. `day_secs`: the
    /// world's time of day.
    pub(crate) fn command(&mut self, from: u32, text: &str, humans: Option<&mut crate::humans::Humans>, traffic: Option<&crate::traffic::Traffic>, lan_world: &crate::lan_world::LanWorld, lan: &omsi_net::LanSession, day_secs: f64) -> Vec<(u32, String)> {
        let mut out = Vec::new();
        let Some(rest) = text.strip_prefix("ticket ") else { return out };
        let (verb, args) = rest.split_once(' ').unwrap_or((rest, ""));
        let f: Vec<&str> = args.split(';').map(str::trim).collect();
        let minute = (day_secs / 60.0) as i32;
        let now = unix_now();
        let name_of = |id: u32| lan.peers().find(|p| p.pose.id == id).map(|p| p.pose.name.clone()).unwrap_or_default();
        match verb {
            "patrol" => {
                let (company, line) = (f.first().copied().unwrap_or("").to_string(), f.get(1).copied().unwrap_or("").to_string());
                if company.is_empty() {
                    return out;
                }
                let name = name_of(from);
                let i = self.by.entry(from).or_insert_with(|| {
                    log::info!("patrol: {name} ({from}) checks tickets for {company} on line {line}");
                    HostInspector { name: name.clone(), company: company.clone(), line: line.clone(), since: now, last: now, ride: None }
                });
                i.last = now;
            }
            "check" => {
                let Some(id) = f.first().and_then(|x| x.parse::<u32>().ok()) else { return out };
                if !self.by.contains_key(&from) {
                    out.push((from, format!("ticket no {id}")));
                    return out;
                }
                if let Some(i) = self.by.get_mut(&from) {
                    i.last = now;
                }
                if id < RELAYED_FROM {
                    // (a rider of ours: a timetable bus's, or the host player's own bus's)
                    let Some(h) = humans else {
                        out.push((from, format!("ticket is {id};gone")));
                        return out;
                    };
                    let bus = h.rider_bus(id);
                    let r = h.check_ticket(id, AI_SHARE);
                    let (key, line) = match bus {
                        Some(BusId::Ai(car)) => ((false, car), traffic.and_then(|t| t.cars.iter().find(|c| c.id == car)).map(|c| crate::lan_world::car_display(&c.vehicle).0).unwrap_or_default()),
                        Some(BusId::Player) => ((true, lan.my_id as u64), String::new()),
                        None => ((false, 0), String::new()),
                    };
                    self.count(from, key, line, r, minute);
                    out.push((from, format!("ticket is {id};{}", r.key())));
                } else {
                    // (a rider of a player's bus: his game knows)
                    match lan_world.relayed_owner(id) {
                        Some((owner, pid)) => out.push((owner, format!("ticket ask {from};{pid};{id}"))),
                        None => out.push((from, format!("ticket is {id};gone"))),
                    }
                }
            }
            "told" => {
                let (Some(insp), Some(id), Some(r)) = (f.first().and_then(|x| x.parse::<u32>().ok()), f.get(1).and_then(|x| x.parse::<u32>().ok()), f.get(2).and_then(|x| Ticket::of(x))) else { return out };
                if !self.by.contains_key(&insp) {
                    return out;
                }
                let line = lan.peers().find(|p| p.pose.id == from).map(|p| p.pose.line.clone()).unwrap_or_default();
                self.count(insp, (true, from as u64), line, r, minute);
                out.push((insp, format!("ticket is {id};{}", r.key())));
            }
            "end" => {
                if let Some(i) = self.by.get_mut(&from) {
                    i.last = now;
                }
                self.flush_ride(from);
                self.flush_time(from);
                if let Some(i) = self.by.remove(&from) {
                    log::info!("patrol: {} ended his ticket inspection", i.name);
                }
            }
            _ => {}
        }
        out
    }

    /// Now and then (the host): inspectors gone, buses left, their time written.
    pub(crate) fn tick(&mut self, lan: &omsi_net::LanSession) {
        let now = unix_now();
        let here: HashSet<u32> = lan.peers().map(|p| p.pose.id).collect();
        let ids: Vec<u32> = self.by.keys().copied().collect();
        for id in ids {
            let Some(i) = self.by.get(&id) else { continue };
            let gone = !here.contains(&id) || now - i.last > GONE_AFTER;
            if gone {
                self.flush_ride(id);
                self.flush_time(id);
                if let Some(i) = self.by.remove(&id) {
                    log::info!("patrol: {} left his ticket inspection", i.name);
                }
                continue;
            }
            if i.ride.as_ref().is_some_and(|r| now - r.last > RIDE_IDLE) {
                self.flush_ride(id);
            }
            if self.by.get(&id).is_some_and(|i| i.last - i.since >= TIME_EVERY) {
                self.flush_time(id);
            }
        }
    }

    /// What to write for the inspectors' companies since.
    pub(crate) fn take_seen(&mut self) -> Vec<(String, Seen)> {
        std::mem::take(&mut self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_launcher_s_patrol_is_read() {
        let s = Spec::parse("c17|Linie5|3|840|750 kr.|~Inspector_A_umove").unwrap();
        assert_eq!((s.company.as_str(), s.line.as_str(), s.tour.as_str(), s.until, s.fine.as_str(), s.clothes.as_str()), ("c17", "Linie5", "3", 840, "750 kr.", "~Inspector_A_umove"));
        assert_eq!(Spec::parse("c17|Linie5|3|840|x|party").unwrap().clothes, "");
        assert!(Spec::parse("|Linie5").is_none());
    }

    #[test]
    fn the_host_counts_each_bus_an_inspector_checks_in() {
        let mut b = HostBook::default();
        b.by.insert(7, HostInspector { name: "Urban".into(), company: "c17".into(), line: "Linie5".into(), since: 0.0, last: 0.0, ride: None });
        b.count(7, (false, 3), "5".into(), Ticket::Valid, 600);
        b.count(7, (false, 3), "5".into(), Ticket::Caught, 601);
        b.count(7, (false, 3), "5".into(), Ticket::Checked, 601);
        // (another bus: the first one written)
        b.count(7, (true, 9), "14".into(), Ticket::Valid, 610);
        let seen = b.take_seen();
        assert_eq!(seen.len(), 1);
        assert_eq!((seen[0].0.as_str(), seen[0].1.kind.as_str(), seen[0].1.line.as_str(), seen[0].1.checked, seen[0].1.caught, seen[0].1.minute), ("Urban", "ride", "5", 2, 1, 600));
        // his time, when he ends
        b.by.get_mut(&7).unwrap().last = 3600.0;
        b.flush_ride(7);
        b.flush_time(7);
        let seen = b.take_seen();
        assert_eq!(seen.iter().map(|s| s.1.kind.as_str()).collect::<Vec<_>>(), ["ride", "time"]);
        assert_eq!(seen[1].1.minutes, 60.0);
    }
}

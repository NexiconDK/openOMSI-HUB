//! (NexiconDK, ticket inspection round V, 12 November 2026) A player as one of his bus company's
//! ticket inspectors in a map's world of Bus Company Online (`--patrol`).
//!
//! His game starts on foot at a stop of the patrol's line (`duty_start::place_on_patrol`), his
//! walker in the company's inspector's uniform - the others draw him in it too (`INFO`'s figure
//! carries the clothes, `omsi_net::figure_clothes`). He walks into a bus (G by its door) and K by
//! a rider checks their ticket: the one he faces, not checked yet (`PeopleSim::rider_ahead`) - one
//! a press. The company's inspector of the day's patrol goes with him (`Mate`): in the uniform he
//! follows him in and out of the buses and checks riders near him himself, counted as his.
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
const REACH: f32 = 3.5;
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

/// The patrol as the launcher gave it (`--patrol <company>|<line>|<tour>|<until>|<fine>|<clothes>|<partner>`:
/// `until` minutes of the day, `fine` as the company writes it, `partner` the company's inspector
/// who goes with him - none: alone).
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Spec {
    pub company: String,
    pub line: String,
    pub tour: String,
    pub until: i32,
    pub fine: String,
    pub clothes: String,
    pub partner: String,
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
            partner: f.get(6).unwrap_or(&"").to_string(),
        };
        (!spec.company.is_empty() && !spec.line.is_empty()).then_some(spec)
    }
}

/// (14 November 2026) The inspector's card on the screen (`ui::TicketCard`): a headline, a line
/// under it, its tone (0 information, 1 a valid ticket, 2 none), how long it stays (s).
#[derive(Debug, Clone, Default)]
pub(crate) struct Card {
    pub title: String,
    pub line: String,
    pub tone: u8,
    left: f32,
    total: f32,
}

impl Card {
    /// How far it shows (0 to 1): in quickly, out in the last half second.
    pub(crate) fn alpha(&self) -> f32 {
        ((self.total - self.left) / 0.15).min(self.left / 0.5).clamp(0.0, 1.0)
    }
}

/// The avatar key of his partner (the walker's is `on_foot::AVATAR_KEY`, the others' from
/// `on_foot::REMOTE_KEY`, a relief driver's `relief::DRIVER_KEY`).
pub(crate) const MATE_KEY: u32 = 3_000_000;
/// How far behind him the partner walks (m, along his way), how near a rider must be to him, and
/// the seconds between two of his checks.
const MATE_GAP: f64 = 1.4;
const MATE_REACH: f32 = 2.6;
const MATE_EVERY: f32 = 7.0;

/// Where someone is: on the ground (the world), or aboard a bus (its cabin's point).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Spot {
    Ground(glam::DVec3),
    Bus(BusId, glam::Vec3),
}

impl Spot {
    /// How far apart (m, over the floor); not the same bus or ground: none (infinite).
    fn dist(&self, o: &Spot) -> f64 {
        match (self, o) {
            (Spot::Ground(a), Spot::Ground(b)) => (*a - *b).truncate().length(),
            (Spot::Bus(a, x), Spot::Bus(b, y)) if a == b => (*x - *y).truncate().length() as f64,
            _ => f64::INFINITY,
        }
    }
}

/// (14 November 2026) His partner, the company's inspector: walks his way behind him (his trail),
/// into the bus he goes into, and checks the riders near him there now and then.
#[derive(Default)]
struct Mate {
    /// Where the walker has been, oldest first (every 15 cm).
    trail: std::collections::VecDeque<Spot>,
    at: Option<Spot>,
    /// Facing: in the world (on the ground), in the bus's frame (aboard).
    heading: f64,
    speed: f64,
    kind: Option<u64>,
    /// Seconds to his next check.
    next: f32,
    /// The riders he asked about (their ids), and the one he faces with the seconds left.
    ids: HashSet<u32>,
    face: Option<(u32, f32)>,
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
    /// What the screen shows him now.
    pub(crate) card: Option<Card>,
    /// His partner (`Spec::partner`).
    mate: Mate,
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
    /// The inspector's card shown: `title` large, `line` under it (the game's log has it too).
    fn patrol_card(&mut self, title: String, line: String, tone: u8, secs: f32) {
        log::info!("patrol card: {title}{}{line}", if line.is_empty() { "" } else { " - " });
        self.net.patrol.card = Some(Card { title, line, tone, left: secs, total: secs });
    }

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
        if !spec.partner.is_empty() {
            log::info!("patrol: with {}", spec.partner);
            self.patrol_card(tr("%{name} goes with you").replace("%{name}", &spec.partner), tr("Your partner follows you into the buses and checks tickets too."), 0, 7.0);
        }
        self.net.patrol.spec = Some(spec);
        self.net.patrol.read = true;
    }

    /// K on foot: the ticket of the rider he faces in the bus he is in - one at a time.
    pub(crate) fn patrol_check(&mut self) {
        if !self.net.patrol.on() {
            log::info!("patrol: K, but no ticket-inspection duty (--patrol {:?})", self.args.patrol);
            return;
        }
        if self.net.patrol.over {
            self.patrol_card(tr("Your ticket-inspection duty is over."), String::new(), 0, 4.0);
            return;
        }
        let aboard = self.session.humans.as_ref().and_then(|h| h.avatar_aboard(crate::on_foot::AVATAR_KEY));
        log::info!("patrol: K - the walker aboard {:?}, inside {:?}, seated {:?}", aboard.map(|a| a.0), self.session.on_foot.as_ref().and_then(|f| f.inside.map(|x| x.0)), self.session.on_foot.as_ref().and_then(|f| f.seat.map(|x| x.0)));
        let Some((bus, at)) = aboard else {
            self.patrol_card(tr("Get into a bus first: K by a passenger checks the ticket."), String::new(), 0, 3.5);
            return;
        };
        if self.net.patrol.ride.bus != Some(bus) {
            self.patrol_ride_over();
            self.net.patrol.ride.bus = Some(bus);
        }
        let but: Vec<u32> = self.net.patrol.asked.iter().copied().collect();
        let heading = self.session.on_foot.as_ref().map(|f| f.heading).unwrap_or(0.0);
        let Some(id) = self.session.humans.as_ref().and_then(|h| h.rider_ahead(bus, at, heading, REACH, &but)) else {
            // (14 November 2026: those near him checked already - said so)
            let checked_near = self.session.humans.as_ref().and_then(|h| h.rider_ahead(bus, at, heading, REACH, &[])).is_some();
            let text = if checked_near { tr("Everybody near you is checked: walk on through the bus.") } else { tr("Nobody to check within reach: walk up to a passenger.") };
            self.patrol_card(text, String::new(), 0, 3.0);
            return;
        };
        self.net.patrol.asked.insert(id);
        let client = self.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client);
        if client {
            if let Some(l) = self.net.lan.as_mut() {
                l.command(1, &format!("ticket check {id}"));
            }
            self.net.patrol.pending.push((id, 0.0));
            self.patrol_card(tr("Ticket, please…"), String::new(), 0, WAIT);
        } else {
            let r = self.session.humans.as_mut().map(|h| h.check_ticket(id, AI_SHARE)).unwrap_or(Ticket::Gone);
            self.patrol_result(id, r);
        }
    }

    /// What checking rider `id` found, said and counted.
    fn patrol_result(&mut self, id: u32, r: Ticket) {
        let p = &mut self.net.patrol;
        p.pending.retain(|x| x.0 != id);
        let by_mate = p.mate.ids.remove(&id);
        let mate = p.spec.as_ref().map(|s| s.partner.clone()).unwrap_or_default();
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
        let counts = tr("%{n} checked in this bus, %{c} without a ticket").replace("%{n}", &n).replace("%{c}", &c);
        let (title, line, tone) = match r {
            Ticket::Valid => (tr("Valid ticket"), counts, 1),
            Ticket::Caught => (tr("No ticket!"), format!("{}  ·  {counts}", tr("A penalty fare of %{fine} is written.").replace("%{fine}", &fine)), 2),
            Ticket::Checked => (tr("Checked already."), counts, 0),
            Ticket::Gone => (tr("That passenger is no longer there."), String::new(), 0),
        };
        // (his partner's check: said with his name, only what he found)
        if by_mate {
            if matches!(r, Ticket::Valid | Ticket::Caught) {
                self.patrol_card(format!("{mate}: {title}"), line, tone, 3.5);
            }
            return;
        }
        self.patrol_card(title, line, tone, 4.0);
    }

    /// Out of the bus: what he found in it said, and the next bus starts afresh.
    fn patrol_ride_over(&mut self) {
        let p = &mut self.net.patrol;
        if p.ride.checked > 0 {
            let line = tr("In this bus: %{n} checked, %{c} without a ticket. Today: %{t} checked, %{a} without.").replace("%{n}", &p.ride.checked.to_string()).replace("%{c}", &p.ride.caught.to_string()).replace("%{t}", &p.checked.to_string()).replace("%{a}", &p.caught.to_string());
            self.patrol_card(tr("Out of the bus"), line, 0, 6.0);
        }
        let p = &mut self.net.patrol;
        p.ride = Ride::default();
        p.asked.clear();
        p.pending.clear();
        p.mate.ids.clear();
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
        // (the card's time)
        if let Some(c) = self.net.patrol.card.as_mut() {
            c.left -= dt;
            if c.left <= 0.0 {
                self.net.patrol.card = None;
            }
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
            // (his partner's unanswered: not said)
            let his = lost.iter().filter(|id| !p.mate.ids.remove(id)).count();
            if his > 0 {
                self.patrol_card(tr("No answer about that ticket: try again."), String::new(), 0, 3.0);
            }
        }
        self.patrol_mate(dt);
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

    /// (14 November 2026) His partner this frame: a step along his trail, aboard where he is, and
    /// now and then a rider near him checked.
    fn patrol_mate(&mut self, dt: f32) {
        let Some(spec) = self.net.patrol.spec.clone() else { return };
        if spec.partner.is_empty() {
            return;
        }
        let Some(f) = self.session.on_foot.as_ref() else {
            if let Some(h) = self.session.humans.as_mut() {
                h.avatar_show(MATE_KEY, false);
            }
            return;
        };
        let (me, my_heading, my_z, my_kind) = (match f.inside {
            Some((b, l)) => Spot::Bus(b, l),
            None => Spot::Ground(f.pos),
        }, f.heading, f.pos.z, f.kind);
        let dt64 = dt.max(1e-3) as f64;
        let m = &mut self.net.patrol.mate;
        // (his trail, and the point of it the gap behind him)
        if m.trail.back().is_none_or(|b| b.dist(&me) > 0.15) {
            m.trail.push_back(me);
        }
        let (mut len, mut prev, mut goal) = (0.0, me, None);
        for i in (0..m.trail.len()).rev() {
            let d = prev.dist(&m.trail[i]);
            len += if d.is_finite() { d } else { 0.6 };
            prev = m.trail[i];
            if len >= MATE_GAP {
                goal = Some(i);
                break;
            }
        }
        if let Some(i) = goal {
            m.trail.drain(..i);
        }
        while m.trail.len() > 400 {
            m.trail.pop_front();
        }
        let goal = goal.map(|_| m.trail[0]);
        // (first: just behind him)
        if m.at.is_none() {
            let h = my_heading.to_radians();
            m.at = Some(match me {
                Spot::Ground(p) => Spot::Ground(p - glam::DVec3::new(h.sin(), h.cos(), 0.0) * MATE_GAP),
                s => s,
            });
            m.heading = my_heading;
        }
        let mut at = m.at.unwrap();
        m.speed = 0.0;
        if let Some(g) = goal {
            let d = at.dist(&g);
            if !d.is_finite() || d > 8.0 {
                at = g;
            } else if d > 0.02 {
                let step = (d * 2.5).clamp(0.6, 3.5) * dt64;
                let k = (step / d).min(1.0);
                let (dx, dy) = match (at, g) {
                    (Spot::Ground(a), Spot::Ground(b)) => {
                        at = Spot::Ground(a + (b - a) * k);
                        (b.x - a.x, b.y - a.y)
                    }
                    (Spot::Bus(bus, a), Spot::Bus(_, b)) => {
                        at = Spot::Bus(bus, a + (b - a) * k as f32);
                        ((b.x - a.x) as f64, (b.y - a.y) as f64)
                    }
                    _ => (0.0, 0.0),
                };
                m.speed = step.min(d) / dt64;
                if dx.hypot(dy) > 1e-4 {
                    m.heading = dx.atan2(dy).to_degrees();
                }
            }
        }
        m.at = Some(at);
        // (a rider he checks: faced)
        if let Some((_, left)) = m.face.as_mut() {
            *left -= dt;
        }
        if m.face.is_some_and(|x| x.1 <= 0.0) {
            m.face = None;
        }
        let face = m.face.map(|x| x.0);
        let clothes = spec.clothes.clone();
        let Some(h) = self.session.humans.as_mut() else { return };
        let Some(w) = self.world.as_ref() else { return };
        let m = &mut self.net.patrol.mate;
        if let (Some(id), Spot::Bus(b, l)) = (face, at) {
            if let Some((rb, rl)) = h.rider_at(id).filter(|x| x.0 == b) {
                let d = rl - l;
                if d.truncate().length() > 0.1 {
                    m.heading = (d.x as f64).atan2(d.y as f64).to_degrees();
                }
                let _ = rb;
            }
        }
        // (his figure: one of those that have the uniform, not the walker's own if another has)
        let kind = *m.kind.get_or_insert_with(|| {
            let n = h.types.len().max(1) as u64;
            (1..n)
                .map(|d| ((my_kind + d) % n) as usize)
                .find(|k| !clothes.is_empty() && h.type_wearing(&clothes, Some(*k)) == Some(*k))
                .or_else(|| h.type_wearing(&clothes, None))
                .map(|k| k as u64)
                .unwrap_or(my_kind + 1)
        });
        let cmd = match at {
            Spot::Ground(p) => {
                let r = m.heading.to_radians();
                crate::humans::AvatarCmd { pos: p, heading: m.heading, vel: glam::DVec2::new(r.sin(), r.cos()) * m.speed, lift: 0.0, seat: None, floor: None, aboard: None }
            }
            Spot::Bus(b, l) => {
                let world_heading = h.bus_heading(b, l).unwrap_or(0.0) + m.heading;
                let r = world_heading.to_radians();
                crate::humans::AvatarCmd { pos: glam::DVec3::new(f.pos.x, f.pos.y, my_z), heading: world_heading, vel: glam::DVec2::new(r.sin(), r.cos()) * m.speed, lift: 0.0, seat: None, floor: Some(my_z), aboard: Some((b, l)) }
            }
        };
        h.avatar_in(MATE_KEY, w, cmd, kind, &clothes);
        h.avatar_show(MATE_KEY, true);
        // (a check now and then, aboard the bus he is in)
        m.next -= dt;
        let Spot::Bus(bus, l) = at else {
            m.next = m.next.max(4.0);
            return;
        };
        if !matches!(me, Spot::Bus(b, _) if b == bus) || m.next > 0.0 || self.net.patrol.over {
            return;
        }
        let n = self.net.patrol.checked;
        self.net.patrol.mate.next = MATE_EVERY + (n % 4) as f32;
        let but: Vec<u32> = self.net.patrol.asked.iter().copied().collect();
        let Some(id) = h.nearest_rider(bus, l, MATE_REACH, &but) else { return };
        if self.net.patrol.ride.bus != Some(bus) {
            self.patrol_ride_over();
            self.net.patrol.ride.bus = Some(bus);
        }
        let p = &mut self.net.patrol;
        p.asked.insert(id);
        p.mate.ids.insert(id);
        p.mate.face = Some((id, 3.0));
        log::info!("patrol: {} checks rider {id}", spec.partner);
        let client = self.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client);
        if client {
            if let Some(l) = self.net.lan.as_mut() {
                l.command(1, &format!("ticket check {id}"));
            }
            self.net.patrol.pending.push((id, 0.0));
        } else {
            let r = self.session.humans.as_mut().map(|h| h.check_ticket(id, AI_SHARE)).unwrap_or(Ticket::Gone);
            self.patrol_result(id, r);
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
        assert_eq!(Spec::parse("c17|Linie5|3|840|x|~Inspector_A|Mette Holm").unwrap().partner, "Mette Holm");
        assert_eq!(s.partner, "");
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

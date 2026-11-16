//! (NexiconDK, ticket inspection round V, 12 November 2026) A player as one of his bus company's
//! ticket inspectors in a map's world of Bus Company Online (`--patrol`).
//!
//! His game starts on foot at a stop of the patrol's line (`duty_start::place_on_patrol`), his
//! walker in the company's inspector's uniform - the others draw him in it too (`INFO`'s figure
//! carries the clothes, `omsi_net::figure_clothes`). He walks into a bus (G by its door) and K by
//! a rider checks their ticket: the one he faces, not checked yet (`PeopleSim::rider_ahead`) - one
//! a press. The company's inspectors of the day's patrol go with him (`Mate`, four at most): in the
//! uniform they follow him at a walker's pace in and out of the buses, through the doors, and in a
//! bus each walks the aisle to riders of his own and checks them, counted as his.
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
/// `until` minutes of the day, `fine` as the company writes it, `partner` the company's inspectors
/// who go with him, their names between commas - none: alone).
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

/// The avatar keys of his partners: this and on, one each (the walker's is `on_foot::AVATAR_KEY`,
/// the others' from `on_foot::REMOTE_KEY`, a relief driver's `relief::DRIVER_KEY`).
pub(crate) const MATE_KEY: u32 = 3_000_000;
/// (15 November 2026) The most partners drawn (`inspection_patrol::MOST_TEAM`).
const MOST_MATES: usize = 4;
/// How far behind him the first partner walks (m, along his way), and each next one further.
const MATE_GAP: f64 = 1.4;
const MATE_SPACING: f64 = 0.9;
/// (NexiconDK, 15 November 2026: "følger han instant med mig?") How fast they walk (m/s), the
/// most they hurry when he got ahead, and how far behind him (m, along his way) they are put
/// beside him at once (a long way behind: he took a lift, a train, a bus as a rider).
const WALK: f64 = 1.35;
const HURRY: f64 = 4.5;
const SNAP: f64 = 40.0;
/// Seconds after he got into a bus that one still outside steps aboard (the bus leaves), and that
/// one still aboard after he got out steps off. (NexiconDK, 16 November 2026: "så løber de igennem
/// bussen og falder ud bagved og kort tid efter spawner de i bussen igen?") Outside while he is in,
/// they walk on to the door (not `MATE_GAP` behind him: that left them on the pavement while he
/// stood inside the door); and as soon as the bus has moved off (`MOVED` m from where he got in or
/// out) one outside is aboard and one aboard out - not left on the road it drove through.
const BOARD_WAIT: f32 = 3.0;
const LEAVE_WAIT: f32 = 10.0;
const MOVED: f64 = 3.0;

/// Whether bus `bus` has moved off from where the trail went through its door: its cabin's point
/// `local` (the trail's point inside) now this far from `ground` (its point outside) - `world`
/// the point where the cabin's point is now (None: the bus is gone).
fn moved_off(world: Option<glam::DVec3>, ground: glam::DVec3) -> bool {
    world.is_none_or(|w| (w - ground).truncate().length() > MOVED)
}
/// ("også inde i selve bussen ... så følger han bare med mig fremfor at tjekke andre?") Aboard,
/// each walks the aisle to a rider of his own (m/s): within `MATE_LOOK` m, not one this near the
/// walker (his), nor another's; stands beside them (`BESIDE`) for the ticket `MATE_CHECK` s; gives
/// up one he cannot reach in `MATE_GIVE_UP` s.
const AISLE_WALK: f64 = 1.1;
const MATE_LOOK: f32 = 14.0;
const HIS: f32 = 1.6;
const BESIDE: f32 = 0.9;
const MATE_CHECK: f32 = 3.0;
const MATE_GIVE_UP: f32 = 12.0;
/// (NexiconDK, 16 November 2026: "de stiller sig stadig klods op af mig inde i bussen") Aboard
/// with no one to check, one this near the walker (m) or `APART` near another of them steps aside
/// along the aisle: to where he is further than `AWAY` from the walker, `APART` from the others
/// and `CLEAR` from the riders - the nearest such point of its paths; on for `STEP_FOR` s at most.
const NEAR: f32 = 1.2;
const AWAY: f32 = 1.5;
const APART: f32 = 0.9;
const CLEAR: f32 = 0.5;
const STEP_FOR: f32 = 8.0;

/// Those who go with him (`Spec::partner`: `inspection_patrol::TEAM_SEP` between them): the name,
/// and whether a woman (`/f`) or a man (`/m`) where the launcher said (`inspection_patrol::GENDER_SEP`).
pub(crate) fn team_of(partner: &str) -> Vec<(String, Option<bool>)> {
    partner
        .split(',')
        .map(|n| match n.rsplit_once('/') {
            Some((name, g)) if matches!(g.trim(), "f" | "m") => (name.trim().to_string(), Some(g.trim() == "f")),
            _ => (n.trim().to_string(), None),
        })
        .filter(|(n, _)| !n.is_empty())
        .take(MOST_MATES)
        .collect()
}

/// (NexiconDK, 16 November 2026: "det skulle forestille at være to kvinder men den ene er en
/// mand?") The figure partner `k` is drawn as: of the figures `wearing` the uniform (each: its
/// type, whether a woman's), one of the partner's gender (`female`; not known: any), not the
/// walker's own (`mine`) where another is, the `k`-th of them; none of that gender: any of them.
fn figure_for(wearing: &[(u64, bool)], female: Option<bool>, mine: u64, k: usize) -> Option<u64> {
    let of = |g: Option<bool>| -> Vec<u64> { wearing.iter().filter(|(_, f)| g.is_none_or(|g| *f == g)).map(|x| x.0).collect() };
    let mut list = of(female);
    if list.is_empty() {
        list = of(None);
    }
    if list.len() > 1 {
        list.retain(|t| *t != mine);
    }
    (!list.is_empty()).then(|| list[k % list.len()])
}

/// Where a partner aboard at `at` with no one to check steps aside to (the bus's frame): None
/// where he is will do - the walker (`walker`) and the others (`others`: where they are and are
/// going) not too near (`NEAR`, `APART`) - or no point of the aisle's (`spots`) is free. Free:
/// `AWAY` from the walker, `APART` from the others, `CLEAR` from the riders (`riders`).
fn stand_aside(at: glam::Vec3, walker: Option<glam::Vec3>, others: &[glam::Vec3], riders: &[glam::Vec3], spots: &[glam::Vec3]) -> Option<glam::Vec3> {
    let d = |a: glam::Vec3, b: glam::Vec3| (a - b).truncate().length();
    let crowded = walker.is_some_and(|w| d(at, w) < NEAR) || others.iter().any(|o| d(at, *o) < APART);
    if !crowded {
        return None;
    }
    spots
        .iter()
        .copied()
        .filter(|s| walker.is_none_or(|w| d(*s, w) >= AWAY) && others.iter().all(|o| d(*s, *o) >= APART) && riders.iter().all(|r| d(*s, *r) >= CLEAR))
        .min_by(|a, b| d(*a, at).total_cmp(&d(*b, at)))
}

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

    fn bus(&self) -> Option<BusId> {
        match self {
            Spot::Bus(b, _) => Some(*b),
            Spot::Ground(_) => None,
        }
    }
}

/// The heading (OMSI's degrees) of a step `dx`, `dy`; None for none.
fn heading_of(dx: f64, dy: f64) -> Option<f64> {
    (dx.hypot(dy) > 1e-4).then(|| dx.atan2(dy).to_degrees())
}

/// Where the walker has been, every 15 cm, oldest first; `first` the number of the oldest kept
/// (the partners keep their place in it by number).
#[derive(Default)]
struct Trail {
    pts: std::collections::VecDeque<Spot>,
    first: u64,
}

impl Trail {
    fn end(&self) -> u64 {
        self.first + self.pts.len() as u64
    }

    fn get(&self, i: u64) -> Option<Spot> {
        i.checked_sub(self.first).and_then(|k| self.pts.get(k as usize)).copied()
    }

    /// The way from point `i` to the newest (m); a step through a door counts `0.6`.
    fn len_from(&self, i: u64) -> f64 {
        let k = i.saturating_sub(self.first) as usize;
        self.pts.iter().skip(k).zip(self.pts.iter().skip(k + 1)).map(|(a, b)| a.dist(b)).map(|d| if d.is_finite() { d } else { 0.6 }).sum()
    }

    /// The point `gap` m back from the newest along the way (its number), or the oldest.
    fn back(&self, gap: f64) -> u64 {
        let mut len = 0.0;
        for k in (1..self.pts.len()).rev() {
            let d = self.pts[k].dist(&self.pts[k - 1]);
            len += if d.is_finite() { d } else { 0.6 };
            if len >= gap {
                return self.first + k as u64 - 1;
            }
        }
        self.first
    }

    /// The newest point aboard bus `bus` (its number).
    fn last_in(&self, bus: BusId) -> Option<u64> {
        self.pts.iter().rposition(|p| p.bus() == Some(bus)).map(|k| self.first + k as u64)
    }
}

/// `at` walked `step` m along the trail from point `cursor` on: a step between the ground and a
/// bus is taken at once (through its door). The new place, the next point to walk to, the
/// heading walked (in the world on the ground, in the bus's frame aboard).
fn walk_trail(t: &Trail, mut at: Spot, mut cursor: u64, mut step: f64) -> (Spot, u64, Option<f64>) {
    let mut heading = None;
    while step > 1e-6 {
        let Some(g) = t.get(cursor) else { break };
        let d = at.dist(&g);
        if !d.is_finite() || d <= step {
            if d.is_finite() {
                step -= d;
            }
            heading = heading_between(&at, &g).or(heading);
            at = g;
            cursor += 1;
            continue;
        }
        let k = step / d;
        heading = heading_between(&at, &g).or(heading);
        at = match (at, g) {
            (Spot::Ground(a), Spot::Ground(b)) => Spot::Ground(a + (b - a) * k),
            (Spot::Bus(bus, a), Spot::Bus(_, b)) => Spot::Bus(bus, a + (b - a) * k as f32),
            _ => g,
        };
        step = 0.0;
    }
    (at, cursor, heading)
}

fn heading_between(a: &Spot, b: &Spot) -> Option<f64> {
    match (a, b) {
        (Spot::Ground(a), Spot::Ground(b)) => heading_of(b.x - a.x, b.y - a.y),
        (Spot::Bus(x, a), Spot::Bus(y, b)) if x == y => heading_of((b.x - a.x) as f64, (b.y - a.y) as f64),
        _ => None,
    }
}

/// `at` walked `step` m along `way` (a bus's frame), the points reached taken off it. The new
/// place, and the heading walked.
fn walk_way(mut at: glam::Vec3, way: &mut Vec<glam::Vec3>, mut step: f32) -> (glam::Vec3, Option<f64>) {
    let mut heading = None;
    while step > 1e-6 {
        let Some(&g) = way.first() else { break };
        let d = (g - at).truncate().length();
        heading = heading_of((g.x - at.x) as f64, (g.y - at.y) as f64).or(heading);
        if d <= step {
            step -= d;
            at = g;
            way.remove(0);
        } else {
            at += (g - at) * (step / d);
            step = 0.0;
        }
    }
    (at, heading)
}

/// What a partner does.
#[derive(Debug, Clone, PartialEq)]
enum Doing {
    /// Walks the walker's way behind him.
    Follow,
    /// Aboard with him: looks for a rider of his own.
    Look { wait: f32 },
    /// Walks the aisle to that rider.
    ToRider { id: u32, way: Vec<glam::Vec3>, t: f32 },
    /// Stands by them for the ticket.
    Check { id: u32, left: f32 },
    /// Steps aside along the aisle, off the walker and the others (`stand_aside`).
    Step { way: Vec<glam::Vec3>, t: f32 },
    /// The walker got out: to the door he left by (trail point `exit`), then out after him.
    Leave { way: Vec<glam::Vec3>, exit: u64, t: f32 },
}

/// (14 November 2026; 15 November 2026: each of the team, each his own way) One of the company's
/// inspectors who go with him: walks his way behind him at a walker's pace (hurrying when he got
/// ahead), into the bus he goes into through its door, and aboard checks riders of his own.
struct Mate {
    name: String,
    /// A woman (`team_of`), for the figure drawn.
    female: Option<bool>,
    at: Option<Spot>,
    /// Facing: in the world (on the ground), in the bus's frame (aboard).
    heading: f64,
    speed: f64,
    kind: Option<u64>,
    /// The trail point he walks to next.
    cursor: u64,
    doing: Doing,
    /// Seconds outside while the walker is in a bus (`BOARD_WAIT`).
    board: f32,
}

impl Mate {
    fn new(name: String, female: Option<bool>, cursor: u64) -> Mate {
        Mate { name, female, at: None, heading: 0.0, speed: 0.0, kind: None, cursor, doing: Doing::Follow, board: 0.0 }
    }

    /// The rider he is busy with.
    fn rider(&self) -> Option<u32> {
        match self.doing {
            Doing::ToRider { id, .. } | Doing::Check { id, .. } => Some(id),
            _ => None,
        }
    }

    /// Put `gap` m behind the walker on his trail, following again.
    fn snap(&mut self, t: &Trail, gap: f64) {
        let i = t.back(gap);
        if let Some(p) = t.get(i) {
            self.at = Some(p);
        }
        self.cursor = i + 1;
        self.doing = Doing::Follow;
        self.board = 0.0;
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
    /// What the screen shows him now.
    pub(crate) card: Option<Card>,
    /// His partners (`Spec::partner`), the walker's way they follow, and the riders each asked
    /// about (their ids: the partner's number).
    mates: Vec<Mate>,
    trail: Trail,
    mate_of: HashMap<u32, usize>,
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
        let team = team_of(&spec.partner);
        match team.len() {
            0 => {}
            1 => {
                log::info!("patrol: with {}", team[0].0);
                self.patrol_card(tr("%{name} goes with you").replace("%{name}", &team[0].0), tr("Your partner follows you into the buses and checks tickets too."), 0, 7.0);
            }
            _ => {
                let names = team.iter().map(|t| t.0.as_str()).collect::<Vec<_>>().join(", ");
                log::info!("patrol: with {names}");
                self.patrol_card(tr("%{names} go with you").replace("%{names}", &names), tr("Your partners follow you into the buses, and in a bus each checks passengers of his own."), 0, 8.0);
            }
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
        let by = p.mate_of.remove(&id);
        let by_mate = by.is_some();
        let mate = by.and_then(|k| p.mates.get(k)).map(|m| m.name.clone()).unwrap_or_default();
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
        p.mate_of.clear();
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
            let his = lost.iter().filter(|id| p.mate_of.remove(id).is_none()).count();
            if his > 0 {
                self.patrol_card(tr("No answer about that ticket: try again."), String::new(), 0, 3.0);
            }
        }
        self.patrol_mates(dt);
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

    /// (14 November 2026; 15 November 2026: the whole team, each his own way) His partners this
    /// frame: on the ground a walker's step along his way behind him, into the bus he goes into
    /// through its door; aboard with him each to a rider of his own, checked; out after him.
    fn patrol_mates(&mut self, dt: f32) {
        let Some(spec) = self.net.patrol.spec.clone() else { return };
        let names = team_of(&spec.partner);
        if names.is_empty() {
            return;
        }
        let Some(f) = self.session.on_foot.as_ref() else {
            if let Some(h) = self.session.humans.as_mut() {
                for k in 0..MOST_MATES as u32 {
                    h.avatar_show(MATE_KEY + k, false);
                }
            }
            return;
        };
        let (me, my_heading, my_z, my_kind, my_pos) = (match f.inside {
            Some((b, l)) => Spot::Bus(b, l),
            None => Spot::Ground(f.pos),
        }, f.heading, f.pos.z, f.kind, f.pos);
        let dt64 = dt.max(1e-3) as f64;
        let over = self.net.patrol.over;
        let p = &mut self.net.patrol;
        if p.mates.len() != names.len() {
            let end = p.trail.end();
            p.mates = names.into_iter().map(|(n, f)| Mate::new(n, f, end)).collect();
        }
        if p.trail.pts.back().is_none_or(|b| b.dist(&me) > 0.15) {
            p.trail.pts.push_back(me);
        }
        let Some(h) = self.session.humans.as_ref() else { return };
        let mut checks: Vec<(usize, u32, BusId)> = Vec::new();
        let p = &mut self.net.patrol;
        for k in 0..p.mates.len() {
            let gap = MATE_GAP + MATE_SPACING * k as f64;
            let taken: Vec<u32> = p.mates.iter().enumerate().filter(|(j, _)| *j != k).filter_map(|(_, m)| m.rider()).chain(p.asked.iter().copied()).chain(p.mate_of.keys().copied()).collect();
            // (where the others are aboard, and where they are going)
            let others: Vec<(BusId, glam::Vec3)> = p
                .mates
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != k)
                .filter_map(|(_, o)| match o.at {
                    Some(Spot::Bus(b, l)) => Some((b, l, o)),
                    _ => None,
                })
                .flat_map(|(b, l, o)| {
                    let to = match &o.doing {
                        Doing::ToRider { way, .. } | Doing::Step { way, .. } => way.last().copied(),
                        _ => None,
                    };
                    std::iter::once((b, l)).chain(to.map(|t| (b, t)))
                })
                .collect();
            let m = &mut p.mates[k];
            // (first: behind him, on his way)
            if m.at.is_none() {
                let a = my_heading.to_radians();
                m.at = Some(match me {
                    Spot::Ground(g) => Spot::Ground(g - glam::DVec3::new(a.sin(), a.cos(), 0.0) * gap),
                    s => s,
                });
                m.heading = my_heading;
                m.cursor = p.trail.end();
            }
            let at = m.at.unwrap_or(me);
            m.speed = 0.0;
            // (what he does now: aboard with the walker, his own riders; the walker got out, after him)
            match (at.bus(), me.bus()) {
                (Some(b), Some(w)) if b == w => {
                    if m.doing == Doing::Follow || matches!(m.doing, Doing::Leave { .. }) {
                        m.doing = Doing::Look { wait: 0.4 + 0.3 * k as f32 };
                    }
                }
                (Some(b), _) => {
                    if !matches!(m.doing, Doing::Follow | Doing::Leave { .. }) {
                        match p.trail.last_in(b) {
                            Some(exit) => {
                                let local = match p.trail.get(exit) {
                                    Some(Spot::Bus(_, l)) => l,
                                    _ => glam::Vec3::ZERO,
                                };
                                let Spot::Bus(_, from) = at else { continue };
                                let mut way = h.aisle_way(b, from, local);
                                way.push(local);
                                m.doing = Doing::Leave { way, exit, t: 0.0 };
                            }
                            None => m.snap(&p.trail, gap),
                        }
                    }
                }
                (None, _) => {
                    if m.doing != Doing::Follow {
                        m.doing = Doing::Follow;
                    }
                }
            }
            let at = m.at.unwrap_or(me);
            match &mut m.doing {
                Doing::Follow => {
                    let ahead = p.trail.get(m.cursor).map_or(0.0, |g| {
                        let d = at.dist(&g);
                        if d.is_finite() {
                            d
                        } else {
                            0.6
                        }
                    }) + p.trail.len_from(m.cursor);
                    if ahead - gap > SNAP {
                        m.snap(&p.trail, gap);
                        continue;
                    }
                    // (he got into a bus and it may leave: one still outside walks on to its door, and
                    // steps aboard at once when it moved off, or in `BOARD_WAIT` s)
                    let boarding = me.bus().is_some() && at.bus().is_none();
                    if boarding {
                        m.board += dt;
                        if let Some(i) = (m.cursor..p.trail.end()).find(|i| p.trail.get(*i).is_some_and(|x| x.bus().is_some() && x.bus() == me.bus())) {
                            let moved = match (p.trail.get(i.saturating_sub(1)), p.trail.get(i)) {
                                (Some(Spot::Ground(g)), Some(Spot::Bus(b, l))) => moved_off(h.bus_point_world(b, l), g),
                                _ => false,
                            };
                            if m.board > BOARD_WAIT || moved {
                                m.at = p.trail.get(i);
                                m.cursor = i + 1;
                                m.board = 0.0;
                                continue;
                            }
                        }
                    } else {
                        m.board = 0.0;
                    }
                    let gap = if boarding { 0.0 } else { gap };
                    if ahead > gap {
                        m.speed = ((ahead - gap) * 1.5).clamp(WALK, HURRY);
                        let step = (m.speed * dt64).min(ahead - gap);
                        let (to, cursor, heading) = walk_trail(&p.trail, at, m.cursor, step);
                        m.at = Some(to);
                        m.cursor = cursor;
                        if let Some(hd) = heading {
                            m.heading = hd;
                        }
                    }
                }
                Doing::Look { wait } => {
                    *wait -= dt;
                    let Spot::Bus(b, l) = at else { continue };
                    if *wait > 0.0 {
                        continue;
                    }
                    *wait = 0.6;
                    let walker = match me {
                        Spot::Bus(w, wl) if w == b => Some(wl),
                        _ => None,
                    };
                    let mine = if over { None } else { h.riders_around(b, l, MATE_LOOK, &taken).into_iter().find(|(_, rp)| walker.is_none_or(|wl| (*rp - wl).truncate().length() > HIS)) };
                    if mine.is_none() {
                        // (no one to check: not on the walker, nor on another of them)
                        let near: Vec<glam::Vec3> = others.iter().filter(|o| o.0 == b).map(|o| o.1).collect();
                        let riders: Vec<glam::Vec3> = h.riders_around(b, l, 30.0, &[]).into_iter().map(|r| r.1).collect();
                        if let Some(spot) = stand_aside(l, walker, &near, &riders, &h.aisle_spots(b)) {
                            let mut way = h.aisle_way(b, l, spot);
                            way.push(spot);
                            m.doing = Doing::Step { way, t: 0.0 };
                        }
                    }
                    if let Some((id, rp)) = mine {
                        let mut way = h.aisle_way(b, l, rp);
                        if way.is_empty() {
                            // (no paths known: straight towards them, to stand beside them)
                            let d = (rp - l).truncate();
                            let len = d.length();
                            if len > BESIDE {
                                way.push(l + (rp - l) * ((len - BESIDE * 0.8) / len));
                            }
                        }
                        m.doing = Doing::ToRider { id, way, t: 0.0 };
                    }
                }
                Doing::ToRider { id, way, t } => {
                    let Spot::Bus(b, l) = at else { continue };
                    *t += dt;
                    let id = *id;
                    let rider = h.rider_at(id).filter(|x| x.0 == b).map(|x| x.1);
                    let Some(rp) = rider.filter(|_| !p.asked.contains(&id) && *t <= MATE_GIVE_UP) else {
                        m.doing = Doing::Look { wait: 0.3 };
                        continue;
                    };
                    let (to, heading) = walk_way(l, way, (AISLE_WALK * dt64) as f32);
                    m.speed = if (to - l).length() > 1e-4 { AISLE_WALK } else { 0.0 };
                    m.at = Some(Spot::Bus(b, to));
                    if let Some(hd) = heading {
                        m.heading = hd;
                    }
                    if way.is_empty() || (rp - to).truncate().length() <= BESIDE {
                        if let Some(hd) = heading_of((rp.x - to.x) as f64, (rp.y - to.y) as f64) {
                            m.heading = hd;
                        }
                        if over {
                            m.doing = Doing::Look { wait: 1.0 };
                        } else {
                            checks.push((k, id, b));
                            m.doing = Doing::Check { id, left: MATE_CHECK };
                        }
                    }
                }
                Doing::Check { id, left } => {
                    *left -= dt;
                    if let (Spot::Bus(b, l), Some((_, rp))) = (at, h.rider_at(*id).filter(|x| Some(x.0) == at.bus())) {
                        let _ = b;
                        if let Some(hd) = heading_of((rp.x - l.x) as f64, (rp.y - l.y) as f64) {
                            m.heading = hd;
                        }
                    }
                    if *left <= 0.0 {
                        m.doing = Doing::Look { wait: 0.4 };
                    }
                }
                Doing::Step { way, t } => {
                    *t += dt;
                    let Spot::Bus(b, l) = at else { continue };
                    let (to, heading) = walk_way(l, way, (AISLE_WALK * dt64) as f32);
                    m.speed = if (to - l).length() > 1e-4 { AISLE_WALK } else { 0.0 };
                    m.at = Some(Spot::Bus(b, to));
                    if let Some(hd) = heading {
                        m.heading = hd;
                    }
                    if way.is_empty() || *t > STEP_FOR {
                        m.doing = Doing::Look { wait: 0.6 };
                    }
                }
                Doing::Leave { way, exit, t } => {
                    *t += dt;
                    let exit = *exit;
                    let Spot::Bus(b, l) = at else { continue };
                    let (to, heading) = walk_way(l, way, (WALK * 1.2 * dt64) as f32);
                    m.speed = if (to - l).length() > 1e-4 { WALK * 1.2 } else { 0.0 };
                    m.at = Some(Spot::Bus(b, to));
                    if let Some(hd) = heading {
                        m.heading = hd;
                    }
                    // (at the door he left by - or the bus is going, or gone: out after him)
                    let gone = match (p.trail.get(exit), p.trail.get(exit + 1)) {
                        (Some(Spot::Bus(bb, l)), Some(Spot::Ground(g))) => moved_off(h.bus_point_world(bb, l), g),
                        _ => false,
                    };
                    if way.is_empty() || *t > LEAVE_WAIT || gone {
                        match p.trail.get(exit + 1) {
                            Some(out) => {
                                m.at = Some(out);
                                m.cursor = exit + 2;
                                m.doing = Doing::Follow;
                            }
                            None => m.snap(&p.trail, gap),
                        }
                    }
                }
            }
        }
        // (the trail kept from where the partner furthest behind is)
        let keep = p.mates.iter().map(|m| m.cursor).min().unwrap_or(p.trail.end()).saturating_sub(2);
        while p.trail.first < keep && !p.trail.pts.is_empty() {
            p.trail.pts.pop_front();
            p.trail.first += 1;
        }
        while p.trail.pts.len() > 4000 {
            p.trail.pts.pop_front();
            p.trail.first += 1;
        }
        let first = p.trail.first;
        for (k, m) in p.mates.iter_mut().enumerate() {
            if m.cursor < first {
                m.snap(&p.trail, MATE_GAP + MATE_SPACING * k as f64);
            }
        }
        // (their figures: of those that have the uniform, each another, not the walker's own if
        // another has it)
        let clothes = spec.clothes.clone();
        let Some(h) = self.session.humans.as_mut() else { return };
        let Some(w) = self.world.as_ref() else { return };
        let n = h.types.len().max(1) as u64;
        // (the figures in the uniform, each a woman's or a man's; no uniform: the grown-ups)
        let female = |t: usize| crate::view_sync::people::avatars::is_female(&h.types[t]);
        let mut wearing: Vec<(u64, bool)> = (0..n as usize).filter(|t| !clothes.is_empty() && h.type_wearing(&clothes, Some(*t)) == Some(*t)).map(|t| (t as u64, female(t))).collect();
        if wearing.is_empty() {
            wearing = (0..h.types.len()).filter(|t| h.types[*t].def.grown_up()).map(|t| (t as u64, female(t))).collect();
        }
        let p = &mut self.net.patrol;
        for (k, m) in p.mates.iter_mut().enumerate() {
            let kind = *m.kind.get_or_insert_with(|| figure_for(&wearing, m.female, my_kind % n, k).unwrap_or(my_kind + 1 + k as u64));
            let Some(at) = m.at else { continue };
            let cmd = match at {
                Spot::Ground(g) => {
                    let r = m.heading.to_radians();
                    crate::humans::AvatarCmd { pos: g, heading: m.heading, vel: glam::DVec2::new(r.sin(), r.cos()) * m.speed, lift: 0.0, seat: None, floor: None, aboard: None }
                }
                Spot::Bus(b, l) => {
                    let world_heading = h.bus_heading(b, l).unwrap_or(0.0) + m.heading;
                    let r = world_heading.to_radians();
                    crate::humans::AvatarCmd { pos: glam::DVec3::new(my_pos.x, my_pos.y, my_z), heading: world_heading, vel: glam::DVec2::new(r.sin(), r.cos()) * m.speed, lift: 0.0, seat: None, floor: Some(my_z), aboard: Some((b, l)) }
                }
            };
            h.avatar_in(MATE_KEY + k as u32, w, cmd, kind, &clothes);
            h.avatar_show(MATE_KEY + k as u32, true);
        }
        // (their checks: as his, said with their names)
        for (k, id, bus) in checks {
            if self.net.patrol.ride.bus != Some(bus) {
                self.patrol_ride_over();
                self.net.patrol.ride.bus = Some(bus);
            }
            let p = &mut self.net.patrol;
            if p.asked.contains(&id) {
                continue;
            }
            p.asked.insert(id);
            p.mate_of.insert(id, k);
            log::info!("patrol: {} checks rider {id}", p.mates.get(k).map(|m| m.name.as_str()).unwrap_or(""));
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

    /// (NexiconDK, 15 November 2026) The whole team goes with him, four at most.
    #[test]
    fn the_team_of_the_launcher_is_read() {
        let names = |s: &str| team_of(s).into_iter().map(|t| t.0).collect::<Vec<_>>();
        assert_eq!(names("Mette Holm"), ["Mette Holm"]);
        assert_eq!(names("Mette Holm, Bo Lund,,Eva Kjær ,Ali Nur,Ida Bech"), ["Mette Holm", "Bo Lund", "Eva Kjær", "Ali Nur"]);
        assert!(team_of(" ").is_empty());
        let s = Spec::parse("c17|Linie5|3|840|x|~Inspector_A|Mette Holm,Bo Lund").unwrap();
        assert_eq!(team_of(&s.partner).len(), 2);
        // (16 November 2026: a woman or a man, where the launcher said)
        assert_eq!(team_of("Mette Holm/f,Bo Lund/m,Kim Berg, /f"), [("Mette Holm".to_string(), Some(true)), ("Bo Lund".to_string(), Some(false)), ("Kim Berg".to_string(), None)]);
    }

    /// (NexiconDK, 16 November 2026: "det skulle forestille at være to kvinder men den ene er en
    /// mand?") Each is drawn as a figure of their gender in the uniform, not the walker's own
    /// where another is; none of that gender: one of the other.
    #[test]
    fn a_partner_is_drawn_as_a_woman_or_a_man_by_the_name() {
        // (figures 2 and 5 a woman's, 3 a man's; the walker is 2)
        let w = [(2, true), (3, false), (5, true)];
        assert_eq!(figure_for(&w, Some(true), 2, 0), Some(5));
        assert_eq!(figure_for(&w, Some(true), 2, 1), Some(5));
        assert_eq!(figure_for(&w, Some(false), 2, 0), Some(3));
        assert_eq!((figure_for(&w, Some(true), 3, 0), figure_for(&w, Some(true), 3, 1)), (Some(2), Some(5)));
        assert_eq!(figure_for(&[(3, false)], Some(true), 3, 0), Some(3));
        assert_eq!(figure_for(&w, None, 2, 1), Some(5));
        assert_eq!(figure_for(&[], Some(true), 0, 0), None);
    }

    /// ("så løber de igennem bussen og falder ud bagved") The bus moved off from where the walker got
    /// in or out: they step aboard or out at once.
    #[test]
    fn a_bus_that_moved_off_is_seen() {
        let g = glam::DVec3::new(10.0, 0.0, 0.0);
        assert!(!moved_off(Some(glam::DVec3::new(10.5, 0.4, 1.2)), g));
        assert!(moved_off(Some(glam::DVec3::new(10.0, -3.5, 0.0)), g));
        assert!(moved_off(None, g));
    }

    /// ("de stiller sig stadig klods op af mig inde i bussen") Aboard with no one to check, one on
    /// the walker or on another of them steps aside to the nearest free point of the aisle.
    #[test]
    fn a_partner_aboard_steps_off_the_walker_and_the_others() {
        let v = |y: f32| glam::Vec3::new(0.0, y, 0.0);
        let spots: Vec<glam::Vec3> = (0..10).map(|k| v(k as f32)).collect();
        // on the walker (at 4): to the nearest point 1.5 m off him and off the rider at 6
        assert_eq!(stand_aside(v(4.3), Some(v(4.0)), &[], &[v(6.0)], &spots), Some(v(2.0)));
        // two on one point: the second steps off the first, not onto the walker
        assert_eq!(stand_aside(v(2.0), Some(v(4.0)), &[v(2.0)], &[], &spots), Some(v(1.0)));
        // free where he is: stays
        assert_eq!(stand_aside(v(1.0), Some(v(4.0)), &[v(2.0)], &[], &spots), None);
        // no free point: stays
        assert_eq!(stand_aside(v(4.3), Some(v(4.0)), &[], &[], &[v(4.0), v(5.0)]), None);
    }

    /// ("følger han instant med mig?") A partner walks the walker's way at a walker's pace, and
    /// steps into the bus through its door where the walker did - not there at once.
    #[test]
    fn a_partner_walks_the_way_and_steps_aboard_at_the_door() {
        let g = |x: f64| Spot::Ground(glam::DVec3::new(x, 0.0, 0.0));
        let bus = BusId::Ai(7);
        let mut t = Trail::default();
        for k in 0..=40 {
            t.pts.push_back(g(k as f64 * 0.25));
        }
        t.pts.push_back(Spot::Bus(bus, glam::Vec3::new(0.0, 5.0, 0.0)));
        t.pts.push_back(Spot::Bus(bus, glam::Vec3::new(0.0, 4.0, 0.0)));
        // a second at a walker's pace: 1.35 m along, still outside, heading east
        let (at, cursor, hd) = walk_trail(&t, g(0.0), 1, WALK);
        assert!(matches!(at, Spot::Ground(p) if (p.x - WALK).abs() < 1e-6), "{at:?}");
        assert_eq!(cursor, 6);
        assert!((hd.unwrap() - 90.0).abs() < 1e-6);
        // at the door: aboard at the walker's first point inside, then on along the aisle
        let (at, cursor, _) = walk_trail(&t, g(10.0), 41, 0.5);
        assert!(matches!(at, Spot::Bus(b, l) if b == bus && (l.y - 4.5).abs() < 1e-5), "{at:?}");
        assert_eq!(cursor, 42);
        assert!((t.len_from(0) - 11.6).abs() < 1e-9);
        assert_eq!(t.last_in(bus), Some(42));
        assert_eq!(t.back(1.0), 41);
        // aboard: along the aisle's points
        let mut way = vec![glam::Vec3::new(0.0, 3.0, 0.0), glam::Vec3::new(0.5, 3.0, 0.0)];
        let (at, hd) = walk_way(glam::Vec3::new(0.0, 4.0, 0.0), &mut way, 1.25);
        assert!((at - glam::Vec3::new(0.25, 3.0, 0.0)).length() < 1e-5 && way.len() == 1, "{at:?}");
        assert!((hd.unwrap() - 90.0).abs() < 1e-6);
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

//! (NexiconDK, Bus Company Online: relief at the stop, decided 26 October 2026) A player who takes
//! a tour over from the AI in a LAN world starts on foot at the stop his duty begins at, without a
//! bus (`--relief`; `duty_start::place_on_duty` works out the stop). His game asks the host about
//! his bus every few seconds (`relief want`): the AI drives the tour up to there, and its bus holds
//! at his stop for him (`omsi_sim`'s `ScheduleSim::want_relief`). Walked up to it, his game asks
//! for it (`relief take`): the host takes it off its world and tells where it stood, and his game
//! puts his own bus there (`relief bus`) with the passengers it had; its driver gets up and walks
//! out of the front door, then G at the front door takes the wheel. Not there within
//! `RELIEF_WAIT` of the departure, the AI drives that trip, and he waits for the bus at the start
//! of his next one (`relief kept`).
//!
//! The host answers in `host_command` (a windowed host's `lan_command`, a dedicated server's
//! commands in `offscreen::step`).

use crate::humans::{AvatarCmd, BusId};
use crate::App;
use glam::{DVec2, DVec3};
use omsi_sim::timetable_run::ReliefState;

/// How often the waiting player's game asks the host about his bus (s).
const ASK_EVERY: f32 = 2.0;
/// How often it asks for the bus once near it (s).
const TAKE_EVERY: f32 = 2.5;
/// How near the bus (m, plan view) the walker is when his game asks for it.
const NEAR: f64 = 16.0;
/// The bus's driver's figure (an avatar of the people's animation, as the walker is).
pub(crate) const DRIVER_KEY: u32 = 2_000_000;
/// His pace (m/s).
const DRIVER_WALK: f64 = 1.3;

/// What the waiting player's game is at.
pub(crate) struct Relief {
    /// The trip he takes over: its line and tour, its departure (minutes of the day), the stop
    /// (its index among the trip's stops), its place in his duty, the stop's name.
    pub(crate) line: String,
    pub(crate) tour: String,
    pub(crate) dep: i32,
    pub(crate) stop: usize,
    pub(crate) trip: usize,
    pub(crate) stop_name: String,
    pub(crate) phase: Phase,
    ask_t: f32,
    take_t: f32,
    /// The host's bus standing at his stop for him.
    car: Option<u64>,
    told_here: bool,
    /// Another player drives the bus up to his stop and hands it over (his id).
    from: Option<u32>,
    /// Seconds waited without a word from the world (an older one, or none running a
    /// timetable: his own bus after `NO_ANSWER`), and whether it answered.
    unheard: f32,
    heard: bool,
}

/// How long the waiting player's game waits for the world's first answer (s).
const NO_ANSWER: f32 = 15.0;

/// The player who drives the bus up to another's relief (`relief give`): the trip, where the
/// other waits, who he is, when the host last said so, and when he was last told to get out.
pub(crate) struct Give {
    line: String,
    tour: String,
    dep: i32,
    at: DVec2,
    taker: u32,
    heard: std::time::Instant,
    told_t: f32,
    /// Seconds until it is given on again (the world's answer awaited).
    sent_t: f32,
}

impl Relief {
    /// Waiting for the bus: the trip it runs is not claimed (the AI brings it up to the stop),
    /// his claims begin with the next (`lan_tick`).
    pub(crate) fn waiting(&self) -> Option<usize> {
        matches!(self.phase, Phase::Waiting).then_some(self.trip)
    }

    /// Whether the bus handed over is his already (its passengers stay in it while he walks up
    /// to its door, not left by their driver).
    pub(crate) fn keeps_riders(&self) -> bool {
        !matches!(self.phase, Phase::Waiting)
    }
}

pub(crate) enum Phase {
    /// On foot at the stop, the bus on its way (or there).
    Waiting,
    /// The bus is his: its driver gets out.
    Driver(Driver),
    /// The driver is out: G at the front door takes the wheel.
    Ready,
}

/// The bus's driver getting out: seconds since the bus was handed over, the door opened, his
/// way out (world points: his place, the door, away from the bus), how far along it he is.
pub(crate) struct Driver {
    t: f32,
    kind: u64,
    door: bool,
    path: Vec<DVec3>,
    k: usize,
    pos: DVec3,
    heading: f64,
    /// The bus's floor (his feet on it until he is out of the door).
    floor: f64,
}

/// The texts the game shows (English, the keys of the translations).
const WAIT: &str = "Wait here for your bus: line %{line} at %{time}. Its driver gets out at the stop, then G at the front door takes the wheel.";
const ARRIVED: &str = "Your bus is here: walk up to it.";
const DRIVER_OUT: &str = "The driver gets out: then G at the front door takes the wheel.";
const YOURS: &str = "Your bus: G at the front door takes the wheel.";
const WAIT_DRIVER: &str = "Wait: the driver gets out first.";
const KEPT: &str = "You were not there in time: the AI drives the %{time}. Your bus comes to %{stop} for the %{next}: ride along to there (G at a door of the bus).";
const KEPT_LAST: &str = "You were not there in time: the AI drives the rest of your duty.";
const UNKNOWN: &str = "The world does not know this trip: your bus is put at the stop.";
const NO_BUS: &str = "Your bus could not be put down: %{why}";
const FROM_PLAYER: &str = "Your bus comes with %{name}: they get out at the stop and hand it over to you.";
const GET_OUT: &str = "%{name} waits here to take your bus over: stop, get up (Ctrl+Shift+G) and get out.";
const GIVEN: &str = "Handed over: %{name} drives on.";

fn tr(s: &str) -> String {
    omsi_ui::tr(s).into_owned()
}

fn hhmm_min(m: i32) -> String {
    format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(60))
}

/// A player of the session as the host knows him: his id, claims (`omsi_net::Pose::tour`) and
/// where his walker is (on foot).
pub(crate) struct PeerInfo {
    pub(crate) id: u32,
    pub(crate) tour: String,
    pub(crate) walker: Option<(f64, f64)>,
}

impl PeerInfo {
    /// The players of a LAN session.
    pub(crate) fn of(lan: &omsi_net::LanSession) -> Vec<PeerInfo> {
        lan.peers().map(|p| PeerInfo { id: p.pose.id, tour: p.pose.tour.clone(), walker: p.pose.walker.map(|w| (w.x, w.y)) }).collect()
    }
}

/// Host: a LAN player's game asks about the bus it waits for, or for the bus (`relief want
/// <line>;<tour>;<dep>;<stop>`, `relief take <line>;<tour>;<dep>`), or gives its own bus on to the
/// player waiting for it (`relief gave <line>;<tour>;<dep>;<x>;<y>;<z>;<heading>;<riders>;
/// <number>`) - departures in minutes of the day. The answers to send, to whom (`peers`: the
/// session's players).
pub(crate) fn host_command(from: u32, text: &str, schedule: Option<&mut crate::schedule::Schedule>, traffic: Option<&mut crate::traffic::Traffic>, humans: Option<&crate::humans::Humans>, peers: &[PeerInfo]) -> Vec<(u32, String)> {
    let mut out = Vec::new();
    let Some((verb, args)) = text.strip_prefix("relief ").and_then(|r| r.split_once(' ')) else { return out };
    let f: Vec<&str> = args.split(';').collect();
    let (Some(line), Some(tour), Some(dep)) = (f.first().map(|x| x.trim()), f.get(1).map(|x| x.trim()), f.get(2).and_then(|x| x.trim().parse::<i32>().ok())) else { return out };
    let (Some(s), Some(t)) = (schedule, traffic) else {
        // (a world without a timetable running: his own bus by the stop)
        out.push((from, format!("relief unknown {dep}")));
        return out;
    };
    match verb {
        "want" => {
            let stop: usize = f.get(3).and_then(|x| x.trim().parse().ok()).unwrap_or(0);
            let others: Vec<(u32, Vec<omsi_sim::timetable_run::LanClaim>)> = peers.iter().filter(|p| p.id != from).map(|p| (p.id, omsi_sim::timetable_run::lan_claims(&p.tour))).collect();
            let state = s.sim.want_relief(from, line, tour, dep, stop, &others, &t.sim, t.day_time);
            out.push((
                from,
                match state {
                    ReliefState::Coming => format!("relief coming {dep}"),
                    ReliefState::Here(id) => format!("relief here {dep};{id}"),
                    ReliefState::Kept => format!("relief kept {dep}"),
                    ReliefState::Unknown => format!("relief unknown {dep}"),
                    ReliefState::FromPlayer(a) => format!("relief player {dep};{a}"),
                    ReliefState::Taken => format!("relief taken {dep}"),
                },
            ));
            // (the player who drives the bus up to there: somebody waits for it at his stop - told
            // where he stands)
            if let (ReliefState::FromPlayer(a), Some((wx, wy))) = (state, peers.iter().find(|p| p.id == from).and_then(|p| p.walker)) {
                out.push((a, format!("relief give {line};{tour};{dep};{stop};{wx:.1};{wy:.1};{from}")));
            }
        }
        "take" => match s.sim.take_relief(from, line, tour, dep, &t.sim, t.day_time) {
            Ok(id) => {
                let Some(c) = t.cars.iter().find(|c| c.id == id) else { return out };
                let (p, heading) = (c.vehicle.position, c.vehicle.heading);
                let number = c.vehicle.str_var("number").trim().replace(';', " ");
                let riders = humans.map(|h| h.riding_in(BusId::Ai(id))).unwrap_or(0);
                t.remove_car(id);
                log::info!("LAN: player {from} takes over bus {id} at ({:.1}, {:.1}) with {riders} passenger(s)", p.x, p.y);
                out.push((from, format!("relief bus {dep};{id};{:.3};{:.3};{:.3};{:.2};{riders};{number}", p.x, p.y, p.z, heading)));
            }
            Err(why) => out.push((from, format!("relief no {dep};{why}"))),
        },
        "gave" => match s.sim.gave_relief(from, line, tour, dep, t.day_time) {
            Ok(taker) => {
                // (his bus as he left it, for the player who waits there: from him, not the AI)
                let rest: Vec<&str> = f.iter().skip(3).map(|x| x.trim()).collect();
                log::info!("LAN: player {from} hands his bus on to player {taker}");
                out.push((taker, format!("relief bus {dep};0;{};{from}", rest.join(";"))));
                out.push((from, format!("relief given {dep};{taker}")));
            }
            Err(why) => out.push((from, format!("relief no {dep};{why}"))),
        },
        _ => {}
    }
    out
}

impl App {
    /// `--relief`, the world loaded: on foot at the stop `place_on_duty` found (`at`: x, y, z,
    /// heading), waiting for the bus of the duty's first trip.
    pub(crate) fn relief_begin(&mut self, at: [f64; 4]) {
        let z = self.world.as_ref().and_then(|w| w.walk_height_near(at[0], at[1], at[2])).unwrap_or(at[2]);
        self.start_on_foot(DVec3::new(at[0], at[1], z), at[3]);
        // (no duty after all: his own bus by the stop, not nothing at all)
        let Some(d) = self.session.duty.as_ref() else {
            log::warn!("relief: no duty to wait for: the bus is put at the stop");
            self.relief_own_bus();
            return;
        };
        // (the trips of the duty before it, gone already: the AI's - the world tells his company)
        let skipped: Vec<String> = (0..d.trip_index)
            .filter_map(|k| {
                let trip = d.trips.get(k)?;
                let (l, t) = d.tours.get(k).map(|(l, t, _)| (l.clone(), t.clone())).unwrap_or_else(|| (d.line.clone(), d.tour.clone()));
                Some(format!("relief want {};{};{};0", l.trim(), t.trim(), (trip.departure / 60.0).round() as i32))
            })
            .collect();
        if let Some(l) = self.net.lan.as_mut() {
            for s in &skipped {
                l.command(1, s);
            }
        }
        let k = d.trip_index;
        let Some(trip) = d.trips.get(k) else { return };
        let (line, tour) = match d.tours.get(k) {
            Some((l, t, _)) => (l.clone(), t.clone()),
            None => (d.line.clone(), d.tour.clone()),
        };
        let stop = d.next_stop.min(trip.stops.len().saturating_sub(1));
        let r = Relief {
            line: line.trim().to_string(),
            tour: tour.trim().to_string(),
            dep: (trip.departure / 60.0).round() as i32,
            stop,
            trip: k,
            stop_name: trip.stops.get(stop).map(|s| s.name.trim().to_string()).unwrap_or_default(),
            phase: Phase::Waiting,
            ask_t: 0.0,
            take_t: 0.0,
            car: None,
            told_here: false,
            from: None,
            unheard: 0.0,
            heard: false,
        };
        let when = trip.stops.get(stop).map(|s| (s.dep / 60.0).round() as i32).unwrap_or(r.dep);
        log::info!("relief: on foot at stop {stop} '{}' for line {} tour {} at {}", r.stop_name, r.line, r.tour, hhmm_min(when));
        self.service_msg = Some((tr(WAIT).replace("%{line}", trip.line.trim()).replace("%{time}", &hhmm_min(when)), 12.0));
        self.session.relief = Some(r);
    }

    /// Whether the bus handed over still has its driver in it (G at its wheel waits).
    pub(crate) fn relief_driver_aboard(&self) -> bool {
        matches!(self.session.relief.as_ref().map(|r| &r.phase), Some(Phase::Driver(_)))
    }

    /// Once a frame (LAN client): ask about the bus, ask for it once near it, the driver out.
    pub(crate) fn relief_tick(&mut self, dt: f32) {
        let phase = match self.session.relief.as_ref().map(|r| &r.phase) {
            None => return,
            Some(Phase::Waiting) => 0,
            Some(Phase::Driver(_)) => 1,
            Some(Phase::Ready) => 2,
        };
        match phase {
            0 => {
                let Some(r) = self.session.relief.as_mut() else { return };
                // (the world never answers: his own bus by the stop)
                if !r.heard {
                    r.unheard += dt;
                    if r.unheard > NO_ANSWER {
                        log::warn!("relief: the world does not answer: the bus is put at the stop");
                        self.service_msg = Some((tr(UNKNOWN), 8.0));
                        self.relief_own_bus();
                        return;
                    }
                }
                r.ask_t -= dt;
                r.take_t -= dt;
                let mut say: Vec<String> = Vec::new();
                if r.ask_t <= 0.0 {
                    r.ask_t = ASK_EVERY;
                    say.push(format!("relief want {};{};{};{}", r.line, r.tour, r.dep, r.stop));
                }
                // walked up to the bus standing there: it is his
                if let (Some(car), true) = (r.car, r.take_t <= 0.0) {
                    let bus = self.session.traffic.as_ref().and_then(|t| t.cars.iter().find(|c| c.id == car)).map(|c| c.vehicle.position);
                    let walker = self.session.on_foot.as_ref();
                    let aboard = walker.is_some_and(|f| f.seat.map(|s| s.0) == Some(BusId::Ai(car)) || f.inside.map(|i| i.0) == Some(BusId::Ai(car)));
                    let near = match (bus, walker) {
                        (Some(b), Some(f)) => (b - f.pos).truncate().length() < NEAR,
                        _ => false,
                    };
                    if near || aboard {
                        r.take_t = TAKE_EVERY;
                        say.push(format!("relief take {};{};{}", r.line, r.tour, r.dep));
                    }
                }
                if let Some(l) = self.net.lan.as_mut() {
                    for s in say {
                        l.command(1, &s);
                    }
                }
            }
            1 => self.relief_driver_step(dt),
            _ => {
                // at the wheel: done
                if self.session.on_foot.is_none() && self.player.is_some() {
                    log::info!("relief: at the wheel");
                    self.session.relief = None;
                }
            }
        }
    }

    /// The host's answer about the bus (`host_command`).
    pub(crate) fn relief_answer(&mut self, text: &str) {
        let Some((verb, rest)) = text.split_once(' ') else { return };
        let f: Vec<&str> = rest.split(';').collect();
        // (the player who drives the bus up to another's relief: he gives it on there)
        match verb {
            "give" => {
                let num = |k: usize| f.get(k).and_then(|x| x.trim().parse::<f64>().ok());
                let (Some(dep), Some(x), Some(y), Some(taker)) = (f.get(2).and_then(|x| x.trim().parse::<i32>().ok()), num(4), num(5), f.get(6).and_then(|x| x.trim().parse::<u32>().ok())) else { return };
                let fresh = self.session.relief_give.as_ref().is_none_or(|g| g.dep != dep || g.taker != taker);
                let (told_t, sent_t) = if fresh { (0.0, 0.0) } else { self.session.relief_give.as_ref().map_or((0.0, 0.0), |g| (g.told_t, g.sent_t)) };
                let g = Give { line: f[0].trim().to_string(), tour: f[1].trim().to_string(), dep, at: DVec2::new(x, y), taker, heard: std::time::Instant::now(), told_t, sent_t };
                if fresh {
                    log::info!("relief: player {taker} waits at ({x:.0}, {y:.0}) to take our bus over for line {} tour {} at {}", g.line, g.tour, hhmm_min(dep));
                }
                self.session.relief_give = Some(g);
                return;
            }
            "given" => {
                let taker = f.get(1).and_then(|x| x.trim().parse::<u32>().ok()).unwrap_or(0);
                let name = self.peer_name(taker);
                log::info!("relief: our bus handed on to {name}");
                self.service_msg = Some((tr(GIVEN).replace("%{name}", &name), 8.0));
                self.session.relief_give = None;
                // (it leaves our game: we stay on foot)
                self.relief_drop_bus();
                return;
            }
            _ => {}
        }
        let Some(dep) = f.first().and_then(|x| x.trim().parse::<i32>().ok()) else { return };
        let Some(r) = self.session.relief.as_mut() else { return };
        if dep != r.dep || !matches!(r.phase, Phase::Waiting) {
            return;
        }
        r.heard = true;
        match verb {
            "coming" => r.car = None,
            "here" => {
                r.car = f.get(1).and_then(|x| x.trim().parse::<u64>().ok());
                if r.car.is_some() && !r.told_here {
                    r.told_here = true;
                    self.service_msg = Some((tr(ARRIVED), 6.0));
                }
            }
            "kept" => self.relief_kept(),
            "unknown" => {
                log::warn!("relief: the host does not know line {} tour {} at {}: the bus is put at the stop", r.line, r.tour, hhmm_min(r.dep));
                self.service_msg = Some((tr(UNKNOWN), 8.0));
                self.relief_own_bus();
            }
            "no" => {
                let why = f.get(1).copied().unwrap_or_default().trim().to_string();
                log::info!("relief: not handed over yet: {why}");
            }
            "player" => {
                let from = f.get(1).and_then(|x| x.trim().parse::<u32>().ok());
                if from.is_some() && r.from != from {
                    r.from = from;
                    let name = self.peer_name(from.unwrap_or(0));
                    self.service_msg = Some((tr(FROM_PLAYER).replace("%{name}", &name), 10.0));
                }
            }
            "bus" => {
                let num = |k: usize| f.get(k).and_then(|x| x.trim().parse::<f64>().ok());
                let (Some(car), Some(x), Some(y), Some(z), Some(h)) = (f.get(1).and_then(|x| x.trim().parse::<u64>().ok()), num(2), num(3), num(4), num(5)) else { return };
                let riders = f.get(6).and_then(|x| x.trim().parse::<usize>().ok()).unwrap_or(0);
                let number = f.get(7).map(|x| x.trim().to_string()).filter(|x| !x.is_empty());
                let giver = f.get(8).and_then(|x| x.trim().parse::<u32>().ok());
                self.relief_bus(car, DVec3::new(x, y, z), h, riders, number, giver);
            }
            _ => {}
        }
    }

    /// Not there in time: the AI drives the trip; the next one of his duty is the one to wait
    /// for (its first stop), or none is left.
    fn relief_kept(&mut self) {
        let (Some(r), Some(d)) = (self.session.relief.as_mut(), self.session.duty.as_mut()) else { return };
        let was = r.dep;
        let next = r.trip + 1;
        let Some(trip) = d.trips.get(next) else {
            log::info!("relief: the AI drove the {}, the duty's last trip", hhmm_min(was));
            self.service_msg = Some((tr(KEPT_LAST), 10.0));
            self.session.relief = None;
            return;
        };
        let (line, tour) = match d.tours.get(next) {
            Some((l, t, _)) => (l.clone(), t.clone()),
            None => (d.line.clone(), d.tour.clone()),
        };
        r.line = line.trim().to_string();
        r.tour = tour.trim().to_string();
        r.dep = (trip.departure / 60.0).round() as i32;
        r.stop = 0;
        r.trip = next;
        r.stop_name = trip.stops.first().map(|s| s.name.trim().to_string()).unwrap_or_default();
        r.car = None;
        r.told_here = false;
        r.ask_t = 0.0;
        let msg = tr(KEPT).replace("%{time}", &hhmm_min(was)).replace("%{stop}", &r.stop_name).replace("%{next}", &hhmm_min(r.dep));
        log::info!("relief: the AI drove the {}: waiting for the {} at '{}'", hhmm_min(was), hhmm_min(r.dep), r.stop_name);
        d.start_at(next, 0);
        self.service_msg = Some((msg, 12.0));
    }

    /// The host does not know the trip (a world without it, an older host): his own bus on the
    /// road by the stop, as a start without relief would have put it nearer, and he takes it.
    fn relief_own_bus(&mut self) {
        let Some(f) = self.session.on_foot.as_ref() else { return };
        let at = f.pos;
        let lane = self.session.traffic.as_ref().and_then(|t| {
            t.net
                .lanes
                .iter()
                .filter(|l| l.kind == omsi_sim::traffic::LaneKind::Street)
                .filter_map(|l| l.nearest_point(at).map(|(s, d)| (l, s, d)))
                .filter(|x| x.2 < 30.0)
                .min_by(|a, b| a.2.total_cmp(&b.2))
                .map(|(l, s, _)| l.at(s))
        });
        match lane {
            Some((p, heading)) => self.relief_bus(0, p, heading as f64, 0, None, None),
            None => {
                self.service_msg = Some((tr(NO_BUS).replace("%{why}", "no road near the stop"), 8.0));
                self.session.relief = None;
            }
        }
    }

    /// The host handed its bus `car` over (0: none - his own bus by the stop, or the bus of
    /// player `giver` who got out of it there): his own bus where it stood, with its passengers,
    /// the duty on its trip, its driver getting out (the AI's: `giver` got out himself).
    fn relief_bus(&mut self, car: u64, at: DVec3, heading: f64, riders: usize, number: Option<String>, giver: Option<u32>) {
        let (Some(w), Some(r), Some(scene)) = (self.world.clone(), self.renderer.as_ref(), self.scene.as_mut()) else { return };
        // (the host's bus as it described it to us, when it is not the duty's: a spare bus; the
        // bus of the player who hands it over as his game told us of it)
        let from_player = giver.and_then(|g| self.net.lan.as_ref().and_then(|l| l.peers().find(|p| p.pose.id == g)).map(|p| (p.pose.bus.clone(), p.pose.paint.clone())).filter(|b| !b.0.is_empty()));
        let desc = (car != 0).then(|| self.net.lan_world.car_desc(car as u32)).flatten().or(from_player);
        let own = self.args.bus.clone();
        let (bus, paint) = match desc {
            Some((file, paint)) if own.as_deref().is_none_or(|o| !same_file(o, &file)) => match omsi_lan::mirror::local_file(&self.args.root, &file) {
                Some(p) => (Some(p.to_string_lossy().to_string()), if paint.is_empty() { None } else { Some(paint) }),
                None => (own, self.args.paint.clone()),
            },
            _ => (own, self.args.paint.clone()),
        };
        let one = crate::cli::Args {
            bus,
            paint,
            number: number.or_else(|| self.args.number.clone()),
            spawn: Some(format!("{},{},{},{}", at.x, at.y, heading, at.z)),
            situation: None,
            situation_vars: Vec::new(),
            situation_strvars: Vec::new(),
            situation_odometer_km: None,
            situation_others: Vec::new(),
            situation_next_stop: None,
            line: None,
            tour: None,
            trip: None,
            autostart: false,
            setvar: None,
            setstr: None,
            triggers: None,
            dirt: None,
            riders: 0,
            on_foot: false,
            relief: false,
            relief_at: None,
            ..self.args.clone()
        };
        let mut p = match crate::spawn::spawn_player(&one, &w, r, scene) {
            Ok(Some(p)) => p,
            Ok(None) => return,
            Err(e) => {
                log::warn!("relief: the bus could not be put down: {e:#}");
                self.service_msg = Some((tr(NO_BUS).replace("%{why}", format!("{e:#}").lines().next().unwrap_or_default()), 10.0));
                return;
            }
        };
        // the host's bus goes from our copy of its world
        if car != 0 {
            self.net.lan_world.forget_car(car as u32);
            if let Some(t) = self.session.traffic.as_mut() {
                if t.remove_car(car) {
                    crate::view_sync::apply_traffic(t, &mut self.gfx.sim_view.traffic, &w, r, scene);
                }
            }
        }
        if let Some(a) = self.sound.audio.as_ref() {
            p.load_sounds(a);
        }
        p.vehicle.host.auto_clutch = if self.settings.auto_clutch { 1.0 } else { 0.0 };
        p.ibis_background = true;
        // its passengers in it, the player's bus's (and the walker riding in it, if he is)
        if let Some(f) = self.session.on_foot.as_mut() {
            if car != 0 {
                let was = BusId::Ai(car);
                if let Some(seat) = f.seat.as_mut().filter(|s| s.0 == was) {
                    seat.0 = BusId::Player;
                }
                if let Some(inside) = f.inside.as_mut().filter(|i| i.0 == was) {
                    inside.0 = BusId::Player;
                }
            }
        }
        if let Some(h) = self.session.humans.as_mut() {
            h.player_bus_swapped(0, p.uid, &mut p.vehicle);
            h.ticket_key = crate::situation::ticket_key_name(&self.args.root, &p.bindings);
            h.tickets = p.vehicle.host.tickets.clone();
            // (the map's money: change given and taken, as for a bus put down at the start)
            if h.money.is_none() && !w.global.money_system.trim().is_empty() {
                h.money = Some(crate::money::Money::new(&self.args.root, &w.global.money_system));
            }
            if riders > 0 {
                h.seed_riders(riders, &p.vehicle, &w);
            }
            crate::view_sync::apply_people(h, &mut self.gfx.sim_view.people, &w, r, scene);
        }
        // the duty on the trip he takes over, from his stop, on the IBIS and the timetable paper
        let now = self.clock.time;
        if let (Some(d), Some(rl)) = (self.session.duty.as_mut(), self.session.relief.as_ref()) {
            // (the bus stands at the stop: the duty goes on from there, not from where it finds it)
            d.start_at_here(rl.trip, rl.stop);
            d.update(&mut p.vehicle, now);
            let (trip, stop) = d.trip_for_ibis();
            p.set_duty_destination(trip, stop);
            let mut fonts = w.fonts.lock();
            if let Err(e) = crate::schedule_paper::update_vehicle(&mut p.vehicle, d, &mut fonts) {
                log::warn!("driver timetable paper: {e:#}");
            }
        }
        // running, as the driver left it
        let _ = p.start_up();
        // the driver gets out: his own figure, not the player's
        let kind = crate::driver::driver_type(&w, 1).and_then(|t| self.session.humans.as_mut().map(|h| h.type_index(t) as u64)).unwrap_or(car.wrapping_mul(7) + 11);
        let way = self.session.humans.as_mut().map(|h| {
            let v = &p.vehicle;
            let seat = h.driver_stand(v).and_then(|l| h.vehicle_cabin_world(v, l));
            let door = h.vehicle_driver_door(v);
            (seat, door)
        });
        let v = &p.vehicle;
        let hd = v.heading.to_radians();
        let (fwd, right) = (DVec2::new(hd.sin(), hd.cos()), DVec2::new(hd.cos(), -hd.sin()));
        let half = v.ty.def.bounding_box.map(|b| (b[0] as f64 * 0.5, b[1] as f64 * 0.5 + b[4] as f64)).unwrap_or((1.25, 5.5));
        let (seat, door) = way.unwrap_or((None, None));
        let seat = seat.unwrap_or_else(|| {
            let xy = v.position.truncate() + fwd * (half.1 - 1.5);
            DVec3::new(xy.x, xy.y, v.position.z + 0.9)
        });
        let door = door.unwrap_or_else(|| {
            let xy = v.position.truncate() + fwd * (half.1 - 1.2) + right * (half.0 + 0.5);
            DVec3::new(xy.x, xy.y, v.position.z)
        });
        // (away from the bus on the door's side, and along the kerb a little)
        let side = (door.truncate() - v.position.truncate()).dot(right).signum();
        let away_xy = door.truncate() + right * side * 2.5 + fwd * 4.0;
        let ground = |p: DVec2, z: f64| w.walk_height_near(p.x, p.y, z).unwrap_or(z);
        let door_g = DVec3::new(door.x, door.y, ground(door.truncate(), door.z));
        let away = DVec3::new(away_xy.x, away_xy.y, ground(away_xy, door_g.z));
        let driver = Driver { t: 0.0, kind, door: false, path: vec![seat, door_g, away], k: 1, pos: seat, heading: v.heading, floor: seat.z };
        self.player = Some(p);
        // (a player handed it over: he got out himself - or nobody drove it: his own bus)
        if giver.is_some() || car == 0 {
            log::info!("relief: bus at ({:.1}, {:.1}) handed on by player {giver:?} is ours, {riders} passenger(s)", at.x, at.y);
            if let Some(rl) = self.session.relief.as_mut() {
                rl.phase = Phase::Ready;
            }
            self.service_msg = Some((tr(YOURS), 6.0));
            return;
        }
        log::info!("relief: bus at ({:.1}, {:.1}) is the player's, {riders} passenger(s); its driver gets out", at.x, at.y);
        if let Some(rl) = self.session.relief.as_mut() {
            rl.phase = Phase::Driver(driver);
        }
        self.service_msg = Some((tr(DRIVER_OUT), 6.0));
    }

    /// The driver getting out: up from his place, the front door opened, out of it and away
    /// from the bus, then gone.
    fn relief_driver_step(&mut self, dt: f32) {
        let Some(w) = self.world.clone() else { return };
        let Some(Phase::Driver(d)) = self.session.relief.as_mut().map(|r| &mut r.phase) else { return };
        d.t += dt;
        // the bus put into service (its doors work then), the front door open for him
        let started = self.player.as_ref().is_none_or(|p| p.startup.is_none());
        if !d.door && (started || d.t > 12.0) && d.t > 1.0 {
            d.door = true;
            if let Some(p) = self.player.as_mut() {
                let _ = p.door_key(1);
            }
        }
        let moving = d.door && d.t > 2.5;
        if moving {
            let mut step = DRIVER_WALK * dt as f64;
            while step > 0.0 && d.k < d.path.len() {
                let to = d.path[d.k];
                let gap = to - d.pos;
                let len = gap.truncate().length();
                if len <= step {
                    d.pos = to;
                    d.k += 1;
                    step -= len;
                } else {
                    let dir = gap / len.max(1e-6);
                    d.pos += dir * step;
                    d.heading = dir.x.atan2(dir.y).to_degrees();
                    step = 0.0;
                }
            }
        }
        let done = d.k >= d.path.len();
        let (pos, heading, kind) = (d.pos, d.heading, d.kind);
        // (his feet on the bus's floor until he is out of the door, then on the ground)
        let floor = (d.k <= 1).then_some(d.floor);
        let vel = if moving && !done {
            let h = heading.to_radians();
            DVec2::new(h.sin(), h.cos()) * DRIVER_WALK
        } else {
            DVec2::ZERO
        };
        if let (Some(h), Some(r), Some(scene)) = (self.session.humans.as_mut(), self.renderer.as_ref(), self.scene.as_mut()) {
            if done {
                h.avatar_remove(DRIVER_KEY);
            } else {
                h.avatar(DRIVER_KEY, &w, AvatarCmd { pos, heading, vel, lift: 0.0, seat: None, floor, aboard: None }, kind);
            }
            crate::view_sync::apply_people(h, &mut self.gfx.sim_view.people, &w, r, scene);
        }
        if done {
            log::info!("relief: the driver is out");
            if let Some(r) = self.session.relief.as_mut() {
                r.phase = Phase::Ready;
            }
            self.service_msg = Some((tr(YOURS), 6.0));
        }
    }

    /// A player's name in the session (his id when not known).
    fn peer_name(&self, id: u32) -> String {
        self.net.lan.as_ref().and_then(|l| l.peers().find(|p| p.pose.id == id)).map(|p| p.pose.name.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| format!("#{id}"))
    }

    /// Once a frame (LAN client): another player waits at a stop to take our bus over (`Give`):
    /// told to get out there; out of it, standing at his stop, our bus is given on to him (it
    /// leaves our game: we stay on foot).
    pub(crate) fn relief_give_tick(&mut self, dt: f32) {
        let Some(g) = self.session.relief_give.as_mut() else { return };
        // (the host no longer says so: he took another bus, or left)
        if g.heard.elapsed().as_secs_f32() > 12.0 {
            self.session.relief_give = None;
            return;
        }
        g.told_t -= dt;
        g.sent_t -= dt;
        if g.sent_t > 0.0 {
            return;
        }
        let Some(p) = self.player.as_ref() else { return };
        let v = &p.vehicle;
        let near = (v.position.truncate() - g.at).length() < 60.0;
        let standing = v.physics.velocity_kmh().abs() < 2.0;
        if !near || !standing {
            return;
        }
        let out = self.session.on_foot.as_ref().is_some_and(|f| f.seat.map(|s| s.0) != Some(BusId::Player) && f.inside.map(|i| i.0) != Some(BusId::Player));
        if !out {
            if g.told_t <= 0.0 {
                g.told_t = 10.0;
                let taker = g.taker;
                let name = self.peer_name(taker);
                self.service_msg = Some((tr(GET_OUT).replace("%{name}", &name), 8.0));
            }
            return;
        }
        g.sent_t = 3.0;
        let (line, tour, dep) = (g.line.clone(), g.tour.clone(), g.dep);
        let riders = self.session.humans.as_ref().map(|h| h.riding()).unwrap_or(0);
        let number = v.str_var("number").trim().replace(';', " ");
        let msg = format!("relief gave {line};{tour};{dep};{:.3};{:.3};{:.3};{:.2};{riders};{number}", v.position.x, v.position.y, v.position.z, v.heading);
        if let Some(l) = self.net.lan.as_mut() {
            l.command(1, &msg);
        }
        log::info!("relief: our bus given on to player {} at ({:.1}, {:.1})", g.taker, v.position.x, v.position.y);
    }

    /// Our bus leaves our game (given on to another player): we stay on foot where we are.
    fn relief_drop_bus(&mut self) {
        let Some(mut p) = self.player.take() else { return };
        if let (Some(a), Some(mut ss)) = (self.sound.audio.as_ref(), p.sounds.take()) {
            ss.stop_all(a);
        }
        if let (Some(w), Some(r), Some(scene)) = (self.world.clone(), self.renderer.as_ref(), self.scene.as_mut()) {
            if let Some(mut d) = p.driver.take() {
                d.hide(r, scene);
            }
            if let Some(h) = self.session.humans.as_mut() {
                h.evict(BusId::Player, &w);
                crate::view_sync::apply_people(h, &mut self.gfx.sim_view.people, &w, r, scene);
            }
            w.release_vehicle(r, scene, p.render);
            for t in p.trailer_renders {
                w.release_vehicle(r, scene, t);
            }
        }
    }

    /// G at the wheel of the bus handed over while its driver is still in it: he gets out first.
    pub(crate) fn relief_wait_for_driver(&mut self) -> bool {
        if self.relief_driver_aboard() {
            self.service_msg = Some((tr(WAIT_DRIVER), 3.0));
            return true;
        }
        false
    }
}

/// Whether two vehicle files are the same (relative to the content roots or not, either slash).
fn same_file(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.replace('\\', "/").to_ascii_lowercase();
    let (a, b) = (norm(a), norm(b));
    a == b || a.ends_with(&format!("/{b}")) || b.ends_with(&format!("/{a}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_vehicle_file_however_written() {
        assert!(same_file("Vehicles/MAN_NL_NG/MAN_NL202.bus", "vehicles\\man_nl_ng\\MAN_NL202.bus"));
        assert!(same_file("C:/OMSI 2/Vehicles/MAN_NL_NG/MAN_NL202.bus", "Vehicles/MAN_NL_NG/MAN_NL202.bus"));
        assert!(!same_file("Vehicles/MAN_NL_NG/MAN_NL202.bus", "Vehicles/MAN_NL_NG/MAN_NL222.bus"));
        assert_eq!(hhmm_min(9 * 60 + 5), "09:05");
        assert_eq!(hhmm_min(24 * 60 + 30), "00:30");
    }
}

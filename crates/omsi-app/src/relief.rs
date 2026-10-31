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
//!
//! (27 October 2026) At the end of his duty, where the AI drives the tour on, the AI takes his bus
//! over (`Back`): his game asks the world (`relief end`), which holds the AI's departure after his
//! back; he stops, gets up and gets out, the AI's driver walks up from along the kerb and gets in at
//! the front door, and his game gives the bus back (`relief back`): it leaves his game, the AI's
//! bus of that departure is put out where it stood. A player who gave his bus on to another and
//! whose duty goes on (a trip of another tour) waits for that trip's bus at its first stop, as a
//! relief (`relief_next_trip`).
//!
//! (28 October 2026) On the way through his duty, at a stop of his trip or its terminus, the game
//! menu's "Hand the bus over to the AI here" (`hand_over`) does the same (`relief stop`): the AI
//! drives on with his bus - his trip from that stop, or the tour's next departure -, and his duty
//! ends there: the world tells his company, which books the rest as the AI's (he did not leave it),
//! and he is free for another.

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
    /// (30 October 2026) Where his bus is on its way, as the world last said (`relief coming`):
    /// on the maps (`nav_bus`) - and whether he was told so.
    bus: Option<SeenBus>,
    told_bus: bool,
}

/// The bus a waiting player's game was told of: its place and heading, how late it is (s), and
/// whether it is still on the tour's trip before his.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SeenBus {
    pos: DVec3,
    heading: f64,
    delay: f64,
    before: bool,
}

/// How long the waiting player's game waits for the world's first answer (s), counted while the
/// world is heard from (a world still loading its map says nothing: not counted then).
const NO_ANSWER: f32 = 30.0;
/// The world heard from within this long (s) counts as answering.
const HEARD_LATELY: f32 = 3.0;

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

/// (27 October 2026) The end of his duty where the AI drives the tour on with his bus (`relief end`,
/// `relief back`).
pub(crate) struct Back {
    line: String,
    tour: String,
    /// His last trip's departure (minutes of the day), and the AI's after it (None: not answered
    /// yet; 0: nothing to hand back - the tour ends there, or another drives it on).
    last: i32,
    next: Option<i32>,
    /// Where his bus stood when his duty ended, and how many trips the duty had (driven on: it is
    /// not the end).
    at: DVec2,
    trips: usize,
    ask_t: f32,
    told_t: f32,
    /// Whether the world answered at all, and how often it was asked (a host that does not know
    /// `relief end` says nothing: given up).
    heard: bool,
    asks: u32,
    /// The AI's driver on his way in (and at the wheel, until the world has the bus).
    walker: Option<Driver>,
    /// Given back (`relief back`, sent again until the world answers): seconds since.
    sent: Option<f32>,
    /// (31 October 2026) The world has it (`relief backed`): seconds since - our bus stays in its
    /// place until the AI's that takes it over stands there (`relief_back_swap`).
    swap: Option<f32>,
    /// (28 October 2026) Handed over on the way ("Hand the bus over here", `relief stop`), not at
    /// the end of his duty: his duty ends with it.
    way: Option<OnTheWay>,
}

/// (28 October 2026) A hand-over on the way through his duty: the trip he is on (an index into the
/// duty's trips), and the stop of it he stands at (an index among its stops; None: its terminus -
/// the trip done).
pub(crate) struct OnTheWay {
    trip: usize,
    stop: Option<usize>,
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

/// (28 October 2026) The AI's driver who handed his bus over, out of it: he stands where he got to,
/// turned to the bus, until it has driven off - then he goes once out of sight
/// (`relief_stander_tick`).
pub(crate) struct Stander {
    pos: DVec3,
    /// Seconds he has stood, and until the next look whether he is seen.
    t: f32,
    look_t: f32,
}

/// How far the bus is from him when it has driven off (m), how far off he goes whether seen or not
/// (m), and how long he waits for it at most (s: then he goes once out of sight).
const STANDER_LEFT: f64 = 25.0;
const STANDER_GONE: f64 = 250.0;
const STANDER_MAX: f32 = 1800.0;

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
/// (27 October 2026) The end of his duty, the AI driving the tour on with his bus.
const HAND_BACK: &str = "The AI drives the tour on with your bus at %{time}: stop, get up (Ctrl+Shift+G) and get out - its driver takes it over.";
const DRIVER_IN: &str = "The driver comes to take your bus over.";
const BACK_DONE: &str = "Handed over: the AI drives the tour on.";
const BACK_NO: &str = "The AI could not take your bus over: it stays yours.";
/// (28 October 2026) "Hand the bus over here", the game menu's line, and what it says.
pub(crate) const HAND_OVER: &str = "Hand the bus over to the AI here";
const WAY_ASKED: &str = "The AI is asked to take your bus over here.";
const WAY_NOT_AT_STOP: &str = "Stop at a stop of your trip, or at its terminus, to hand the bus over.";
const WAY_GET_OUT: &str = "The AI takes your bus over here: get up (Ctrl+Shift+G) and get out - its driver comes.";
const WAY_NO: &str = "The AI cannot take your bus over here: try at another stop.";
const WAY_DROVE_OFF: &str = "Hand-over called off: you drove off.";
const WAY_DONE: &str = "Handed over: the AI drives on, and you are free. Leave the server (Esc) to take another trip or duty in the launcher.";
/// His bus given on, his duty going on with another tour.
const NEXT_BUS: &str = "Your duty goes on with line %{line} at %{time} from %{stop}: wait there for its bus.";
/// (30 October 2026) The bus he waits for on the maps.
const BUS_ON_MAP: &str = "Your bus is on its way: follow it on the map (game menu › City map).";
const YOUR_BUS: &str = "Your bus";
const MIN_LATE: &str = "%{n} min late";
const TRIP_BEFORE: &str = "on its trip before";
/// How often his game says that the AI takes his bus over at the end of his duty (s).
const BACK_EVERY: f32 = 3.0;
/// How often it is asked again when the world said no (s): a break too long for the AI's bus to
/// wait at the stop gets shorter.
const BACK_AGAIN: f32 = 30.0;
/// How often the world is asked without any answer before his game gives up (an older host).
const BACK_TRIES: u32 = 10;
/// How long his game waits for the world to take the bus given back (s): kept then.
const BACK_GIVE_UP: f32 = 15.0;
/// (31 October 2026) Given back, our bus stays in its place until a bus of the world's stands
/// within this much of it (m) - the AI's that takes it over -, or this long at most (s).
const BACK_SWAP_NEAR: f64 = 6.0;
const BACK_SWAP_MAX: f32 = 10.0;
/// How far his bus may be from where his duty ended (m): driven off, it is no hand-back.
const BACK_NEAR: f64 = 60.0;
/// (NexiconDK, Bus Company Online: single trips, 27 October 2026) The game menu's line, and what
/// it says.
pub(crate) const DRIVE_ON: &str = "Drive on with the next trip";
const DROVE_ON: &str = "You drive on: the %{time} is yours too.";
const NO_MORE: &str = "The tour has no trip after this one for you, or it has left.";
/// The end of his trip, with trips of the tour he may drive on with.
pub(crate) const END_DRIVE_ON: &str = "End of your trip: the AI drives the tour on from here - or drive its next trip yourself (game menu › Drive on with the next trip).";

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
    /// (31 October 2026) Where his bus stands, and its heading (degrees): one given back to the AI
    /// is taken over just there (`ScheduleSim::back_stood`).
    pub(crate) bus: Option<(DVec3, f64)>,
}

impl PeerInfo {
    /// The players of a LAN session.
    pub(crate) fn of(lan: &omsi_net::LanSession) -> Vec<PeerInfo> {
        lan.peers()
            .map(|p| PeerInfo {
                id: p.pose.id,
                tour: p.pose.tour.clone(),
                walker: p.pose.walker.map(|w| (w.x, w.y)),
                bus: (p.has_pose && p.pose.has_vehicle()).then(|| (DVec3::new(p.pose.x, p.pose.y, p.pose.z), p.pose.heading as f64)),
            })
            .collect()
    }
}

/// Host: a LAN player's game asks about the bus it waits for, or for the bus (`relief want
/// <line>;<tour>;<dep>;<stop>`, `relief take <line>;<tour>;<dep>`), or gives its own bus on to the
/// player waiting for it (`relief gave <line>;<tour>;<dep>;<x>;<y>;<z>;<heading>;<riders>;
/// <number>`); at the end of its duty, whether the AI takes its bus over (`relief end <line>;<tour>;
/// <last dep>`, answered `relief end <last dep>;<the AI's dep, 0: no>`) and its bus given back
/// (`relief back <line>;<tour>;<the AI's dep>`); on the way through its duty, whether the AI takes
/// its bus over at the stop of its trip it stands at (`relief stop <line>;<tour>;<dep>;<stop, -1:
/// the terminus>`, answered as `relief end`) - departures in minutes of the day. The answers to
/// send, to whom (`peers`: the session's players).
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
                    // (30 October 2026: where its bus is now, for his maps)
                    ReliefState::Coming => match s.sim.relief_bus(line, tour, dep, &t.sim, t.day_time) {
                        Some(b) => format!("relief coming {dep};{:.1};{:.1};{:.1};{:.0};{:.0};{}", b.pos.x, b.pos.y, b.pos.z, b.heading, b.delay, u8::from(b.before)),
                        None => format!("relief coming {dep}"),
                    },
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
        // (27 October 2026: the end of his duty - the AI drives the tour on with his bus)
        "end" => match s.sim.want_back(from, line, tour, dep, &t.sim, t.day_time) {
            omsi_sim::timetable_run::BackState::Ai(next, gone) => {
                if let Some(id) = gone {
                    t.remove_car(id);
                }
                out.push((from, format!("relief end {dep};{next}")));
            }
            // (given back already: nothing to hand back - a bus his game has since is his)
            omsi_sim::timetable_run::BackState::Given(_) | omsi_sim::timetable_run::BackState::No => out.push((from, format!("relief end {dep};0"))),
        },
        // (28 October 2026: on the way - "Hand the bus over here")
        "stop" => {
            let stop = f.get(3).and_then(|x| x.trim().parse::<i64>().ok()).filter(|s| *s >= 0).map(|s| s as usize);
            let own = peers.iter().find(|p| p.id == from).map(|p| omsi_sim::timetable_run::lan_claims(&p.tour)).unwrap_or_default();
            let others: Vec<(u32, Vec<omsi_sim::timetable_run::LanClaim>)> = peers.iter().filter(|p| p.id != from).map(|p| (p.id, omsi_sim::timetable_run::lan_claims(&p.tour))).collect();
            match s.sim.want_stop(from, line, tour, dep, stop, &own, &others, &t.sim, t.day_time) {
                omsi_sim::timetable_run::BackState::Ai(next, gone) => {
                    if let Some(id) = gone {
                        t.remove_car(id);
                    }
                    out.push((from, format!("relief end {dep};{next}")));
                }
                omsi_sim::timetable_run::BackState::Given(_) | omsi_sim::timetable_run::BackState::No => out.push((from, format!("relief end {dep};0"))),
            }
        }
        // (31 October 2026: the AI's bus put out just where his stands - `back_stood`)
        "back" => match s.sim.gave_back(from, line, tour, dep, peers.iter().find(|p| p.id == from).and_then(|p| p.bus), &t.sim, t.day_time) {
            Ok(()) => out.push((from, format!("relief backed {dep}"))),
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
    /// (NexiconDK, single trips) The game menu offers "Drive on": the duty is on its last trip, and
    /// the company let him drive on with trips of the tour after it (`--drive-on`).
    pub(crate) fn drive_on_offered(&self) -> bool {
        self.args.drive_on > 0 && self.player.is_some() && self.session.schedule.is_some() && self.session.duty.as_ref().is_some_and(|d| !d.free && d.trip_index + 1 >= d.trips.len())
    }

    /// (NexiconDK, single trips) "Drive on": the tour's next trip is his too - the AI's bus for it
    /// stays off the road (his claims take it in the world), and the company books it as his.
    pub(crate) fn drive_on(&mut self) {
        if !self.drive_on_offered() {
            return;
        }
        let now = self.clock.time;
        let (Some(world), Some(sch), Some(d)) = (self.world.as_ref(), self.session.schedule.as_mut(), self.session.duty.as_mut()) else { return };
        match sch.extend_duty(world, d, now) {
            Ok(dep) => {
                self.args.drive_on -= 1;
                self.service_msg = Some((tr(DROVE_ON).replace("%{time}", &crate::schedule::hhmm(dep)), 5.0));
            }
            Err(e) => {
                log::warn!("drive on: {e}");
                self.args.drive_on = 0;
                self.service_msg = Some((tr(NO_MORE), 4.0));
            }
        }
    }

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
            bus: None,
            told_bus: false,
        };
        let when = trip.stops.get(stop).map(|s| (s.dep / 60.0).round() as i32).unwrap_or(r.dep);
        log::info!("relief: on foot at stop {stop} '{}' for line {} tour {} at {}", r.stop_name, r.line, r.tour, hhmm_min(when));
        self.service_msg = Some((tr(WAIT).replace("%{line}", trip.line.trim()).replace("%{time}", &hhmm_min(when)), 12.0));
        self.session.relief = Some(r);
    }

    /// (30 October 2026) The bus he waits for at his stop as the maps show it, from `at` (where he
    /// stands): "Your bus · 2.4 km · 3 min late" - None while it is not on the road, or here.
    pub(crate) fn relief_nav_bus(&self, at: DVec3) -> Option<crate::navigator::NavBus> {
        let r = self.session.relief.as_ref().filter(|r| matches!(r.phase, Phase::Waiting) && r.car.is_none())?;
        let b = r.bus?;
        let m = (b.pos.truncate() - at.truncate()).length();
        let mut label = format!("{} · {}", tr(YOUR_BUS), if m < 1000.0 { format!("{:.0} m", (m / 10.0).round() * 10.0) } else { format!("{:.1} km", m / 1000.0) });
        let late = (b.delay / 60.0).round() as i64;
        if late >= 1 {
            label = format!("{label} · {}", tr(MIN_LATE).replace("%{n}", &late.to_string()));
        }
        if b.before {
            label = format!("{label} · {}", tr(TRIP_BEFORE));
        }
        Some(crate::navigator::NavBus { position: b.pos, heading: b.heading, label })
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
                // (the world heard from: its silence about the bus counts - not while it says
                // nothing at all, loading its map)
                let live = self.net.lan.as_ref().is_some_and(|l| l.connected && l.host_quiet().as_secs_f32() < HEARD_LATELY);
                let Some(r) = self.session.relief.as_mut() else { return };
                // (the world never answers: his own bus by the stop)
                if !r.heard && live {
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
                let given = self.session.relief_give.take().map(|g| (g.line, g.tour, g.dep));
                // (it leaves our game: we stay on foot - and wait for the bus of our duty's next
                // trip, when it goes on)
                self.relief_drop_bus();
                if let Some(g) = given {
                    self.relief_next_trip(&g);
                }
                return;
            }
            // (27 October 2026: the end of our duty - the AI drives the tour on with our bus)
            "end" | "backed" | "unknown" | "no" if self.relief_back_answer(verb, &f) => return,
            _ => {}
        }
        let Some(dep) = f.first().and_then(|x| x.trim().parse::<i32>().ok()) else { return };
        let Some(r) = self.session.relief.as_mut() else { return };
        if dep != r.dep || !matches!(r.phase, Phase::Waiting) {
            return;
        }
        r.heard = true;
        match verb {
            "coming" => {
                r.car = None;
                // (where it is now: on the maps - said once that it can be followed there)
                let num = |k: usize| f.get(k).and_then(|x| x.trim().parse::<f64>().ok()).filter(|v| v.is_finite());
                r.bus = match (num(1), num(2), num(3), num(4), num(5)) {
                    (Some(x), Some(y), Some(z), Some(heading), Some(delay)) => Some(SeenBus { pos: DVec3::new(x, y, z), heading, delay, before: f.get(6).is_some_and(|b| b.trim() == "1") }),
                    _ => None,
                };
                if r.bus.is_some() && !r.told_bus {
                    r.told_bus = true;
                    self.service_msg = Some((tr(BUS_ON_MAP), 8.0));
                }
            }
            "here" => {
                r.bus = None;
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
        r.bus = None;
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
        let (seat, door_g, away, kind) = driver_way(self.session.humans.as_mut(), &w, &p.vehicle, car);
        let v = &p.vehicle;
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
        // (the figure is this driver's now: one still standing from before goes with it)
        self.session.relief_stander = None;
        self.service_msg = Some((tr(DRIVER_OUT), 6.0));
    }

    /// The driver getting out: up from his place, the front door opened, out of it and a few
    /// steps away from the bus - where he waits, turned to it, until we have driven off
    /// (`relief_stander_tick`).
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
        // (out: he stands there turned to the bus - `Stander`)
        if done {
            let g = d.path[1] - d.pos;
            d.heading = g.x.atan2(g.y).to_degrees();
        }
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
            h.avatar(DRIVER_KEY, &w, AvatarCmd { pos, heading, vel, lift: 0.0, seat: None, floor, aboard: None }, kind);
            crate::view_sync::apply_people(h, &mut self.gfx.sim_view.people, &w, r, scene);
        }
        if done {
            log::info!("relief: the driver is out: he waits beside the bus until we have driven off");
            if let Some(r) = self.session.relief.as_mut() {
                r.phase = Phase::Ready;
            }
            self.session.relief_stander = Some(Stander { pos, t: 0.0, look_t: 0.0 });
            self.service_msg = Some((tr(YOURS), 6.0));
        }
    }

    /// (28 October 2026) Once a frame (LAN client): the driver who handed his bus over stands
    /// beside it until we have driven off (or for half an hour); then he goes once out of sight -
    /// or once the bus is far off.
    pub(crate) fn relief_stander_tick(&mut self, dt: f32) {
        let Some(s) = self.session.relief_stander.as_mut() else { return };
        s.t += dt;
        s.look_t -= dt;
        let pos = s.pos;
        let away = self.player.as_ref().map_or(f64::INFINITY, |p| (p.vehicle.position.truncate() - pos.truncate()).length());
        if away < STANDER_LEFT && s.t < STANDER_MAX {
            return;
        }
        if away < STANDER_GONE {
            if s.look_t > 0.0 {
                return;
            }
            s.look_t = 0.5;
            if !self.session.traffic.as_ref().zip(self.world.as_deref()).is_some_and(|(t, w)| t.hidden(w, pos, 1.0)) {
                return;
            }
        }
        log::info!("relief: the driver who handed the bus over goes");
        self.session.relief_stander = None;
        if let (Some(w), Some(h), Some(r), Some(scene)) = (self.world.clone(), self.session.humans.as_mut(), self.renderer.as_ref(), self.scene.as_mut()) {
            h.avatar_remove(DRIVER_KEY);
            crate::view_sync::apply_people(h, &mut self.gfx.sim_view.people, &w, r, scene);
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

    /// (27 October 2026) Once a frame (LAN client): at the end of our duty, where the AI drives the
    /// tour on, it takes our bus over - asked of the world, and once we are out of it, its driver
    /// walks up and gets in, and the bus is the world's.
    pub(crate) fn relief_back_tick(&mut self, dt: f32) {
        if self.session.relief_back.is_none() {
            self.relief_back_begin();
            return;
        }
        if self.session.relief_back.as_ref().is_some_and(|b| b.swap.is_some()) {
            self.relief_back_swap(dt);
            return;
        }
        if self.session.relief_back.as_ref().is_some_and(|b| b.sent.is_some()) {
            self.relief_back_sent(dt);
            return;
        }
        let way = self.session.relief_back.as_ref().and_then(|b| b.way.as_ref()).map(|w| (w.trip, w.stop));
        // (no longer the end of our duty: driven on, the bus gone or given to a player - or, on the
        // way, no longer on that trip: on to the next at its terminus only)
        let keep = match (self.session.relief_back.as_ref(), self.session.duty.as_ref(), self.player.as_ref()) {
            (Some(b), Some(d), Some(_)) => {
                self.session.relief.is_none()
                    && self.session.relief_give.is_none()
                    && d.trips.len() == b.trips
                    && match way {
                        Some((trip, stop)) => d.trip_index == trip || (stop.is_none() && d.trip_index == trip + 1 && d.next_stop == 0),
                        None => d.trip_index + 1 >= d.trips.len(),
                    }
            }
            _ => false,
        };
        if !keep {
            self.relief_back_cancel();
            return;
        }
        let Some(p) = self.player.as_ref() else { return };
        let out = self.session.on_foot.as_ref().is_some_and(|f| f.seat.map(|s| s.0) != Some(BusId::Player) && f.inside.map(|i| i.0) != Some(BusId::Player));
        let standing = p.vehicle.physics.velocity_kmh().abs() < 2.0;
        let near = self.session.relief_back.as_ref().is_some_and(|b| (p.vehicle.position.truncate() - b.at).length() < BACK_NEAR);
        // (driven off: no hand-back there - nor asked; the world stops holding it - on the way,
        // called off)
        if way.is_some() && !near {
            self.relief_back_cancel();
            self.service_msg = Some((tr(WAY_DROVE_OFF), 5.0));
            return;
        }
        let Some(b) = self.session.relief_back.as_mut() else { return };
        b.ask_t -= dt;
        b.told_t -= dt;
        if near && b.ask_t <= 0.0 {
            if !b.heard && b.asks >= BACK_TRIES {
                log::info!("relief: the host does not answer about handing our bus over: no hand-over");
                if way.is_some() {
                    self.relief_back_cancel();
                    self.service_msg = Some((tr(WAY_NO), 8.0));
                    return;
                }
                b.next = Some(0);
                b.ask_t = f32::INFINITY;
            } else {
                b.asks += 1;
                b.ask_t = if b.next == Some(0) { BACK_AGAIN } else { BACK_EVERY };
                let msg = match way {
                    Some((_, stop)) => format!("relief stop {};{};{};{}", b.line, b.tour, b.last, stop.map_or(-1, |s| s as i64)),
                    None => format!("relief end {};{};{}", b.line, b.tour, b.last),
                };
                if let Some(l) = self.net.lan.as_mut() {
                    l.command(1, &msg);
                }
            }
        }
        let next = b.next.filter(|n| *n > 0);
        let Some(next) = next.filter(|_| near && out && standing) else {
            // (back in the bus before its driver was, or driven off: he waits)
            if b.walker.take().is_some() {
                self.relief_back_walker_gone();
            }
            let Some(b) = self.session.relief_back.as_mut() else { return };
            if let Some(n) = next.filter(|_| near && !out && b.told_t <= 0.0) {
                b.told_t = 15.0;
                let text = if way.is_some() { tr(WAY_GET_OUT) } else { tr(HAND_BACK).replace("%{time}", &hhmm_min(n)) };
                self.service_msg = Some((text, 10.0));
            }
            return;
        };
        if b.walker.is_none() {
            let (Some(w), Some(p)) = (self.world.clone(), self.player.as_ref()) else { return };
            let (seat, door, away, kind) = driver_way(self.session.humans.as_mut(), &w, &p.vehicle, 0);
            log::info!("relief: the AI's driver walks up to take our bus over for the {}", hhmm_min(next));
            let heading = {
                let g = door - away;
                g.x.atan2(g.y).to_degrees()
            };
            if let Some(b) = self.session.relief_back.as_mut() {
                b.walker = Some(Driver { t: 0.0, kind, door: true, path: vec![away, door, seat], k: 1, pos: away, heading, floor: seat.z });
            }
            // (the figure is this driver's now: one still standing from before goes with it)
            self.session.relief_stander = None;
            self.service_msg = Some((tr(DRIVER_IN), 6.0));
        }
        if self.relief_back_walk(dt) {
            self.relief_back_send();
        }
    }

    /// The end of our duty, our bus standing at its terminus, nobody of the session waiting for it:
    /// the AI may take it over (`Back`) - not a duty that ends halfway along a trip, nor a free
    /// drive.
    fn relief_back_begin(&mut self) {
        if self.session.relief.is_some() || self.session.relief_give.is_some() || duty_ends_on_the_way(&self.args) {
            return;
        }
        let (Some(d), Some(p)) = (self.session.duty.as_ref(), self.player.as_ref()) else { return };
        if d.free || !d.trip_done() || d.trip_index + 1 < d.trips.len() || p.vehicle.physics.velocity_kmh().abs() >= 2.0 {
            return;
        }
        let (line, tour) = match d.tours.get(d.trip_index) {
            Some((l, t, _)) => (l.clone(), t.clone()),
            None => (d.line.clone(), d.tour.clone()),
        };
        let last = (d.trip().departure / 60.0).round() as i32;
        self.session.relief_back = Some(Back { line: line.trim().to_string(), tour: tour.trim().to_string(), last, next: None, at: p.vehicle.position.truncate(), trips: d.trips.len(), ask_t: 0.0, told_t: 0.0, heard: false, asks: 0, walker: None, sent: None, swap: None, way: None });
    }

    /// (28 October 2026) The game menu offers "Hand the bus over here": on Bus Company Online's
    /// server, with our bus and a duty that goes on after the trip we are on (its end is the AI's
    /// hand-back by itself), no other hand-over under way.
    pub(crate) fn hand_over_offered(&self) -> bool {
        crate::app_impl::menus::on_server(&self.args)
            && self.args.company.is_some()
            && self.net.lan.as_ref().is_some_and(|l| l.role == omsi_net::Role::Client)
            && self.player.is_some()
            && self.session.relief.is_none()
            && self.session.relief_give.is_none()
            && self.session.relief_back.as_ref().is_none_or(|b| b.way.is_none() && b.next.unwrap_or(0) == 0 && b.walker.is_none())
            && self.session.duty.as_ref().is_some_and(|d| !d.free && !(d.trip_done() && d.trip_index + 1 >= d.trips.len()))
    }

    /// (28 October 2026) "Hand the bus over here": standing at a stop of our trip (or at its
    /// terminus), the world is asked whether the AI takes our bus over there (`relief stop`); once
    /// we are out, its driver walks up and gets in, the bus is the world's - and our duty ends: the
    /// rest is the AI's, and we are free.
    pub(crate) fn hand_over(&mut self) {
        if !self.hand_over_offered() {
            return;
        }
        let (Some(d), Some(p)) = (self.session.duty.as_ref(), self.player.as_ref()) else { return };
        let stop = if d.trip_done() {
            None
        } else if d.at_stop() {
            Some(d.next_stop)
        } else {
            self.service_msg = Some((tr(WAY_NOT_AT_STOP), 5.0));
            return;
        };
        if p.vehicle.physics.velocity_kmh().abs() >= 2.0 {
            self.service_msg = Some((tr(WAY_NOT_AT_STOP), 5.0));
            return;
        }
        let (line, tour) = match d.tours.get(d.trip_index) {
            Some((l, t, _)) => (l.clone(), t.clone()),
            None => (d.line.clone(), d.tour.clone()),
        };
        let dep = (d.trip().departure / 60.0).round() as i32;
        log::info!("relief: handing our bus over to the AI on line {} tour {} at {}, {}", line.trim(), tour.trim(), hhmm_min(dep), stop.map_or("at its terminus".to_string(), |s| format!("at its stop {}", s + 1)));
        let way = Some(OnTheWay { trip: d.trip_index, stop });
        let b = Back { line: line.trim().to_string(), tour: tour.trim().to_string(), last: dep, next: None, at: p.vehicle.position.truncate(), trips: d.trips.len(), ask_t: 0.0, told_t: 0.0, heard: false, asks: 0, walker: None, sent: None, swap: None, way };
        if let Some(old) = self.session.relief_back.replace(b) {
            if old.walker.is_some() {
                self.relief_back_walker_gone();
            }
        }
        self.service_msg = Some((tr(WAY_ASKED), 5.0));
    }

    /// No hand-back after all (the world no longer holds the departure once our game stops asking:
    /// its own bus comes).
    fn relief_back_cancel(&mut self) {
        if let Some(b) = self.session.relief_back.take() {
            if b.walker.is_some() {
                self.relief_back_walker_gone();
            }
        }
    }

    /// Not handed back (the world said no, or nothing): the bus stays ours, the AI's driver gets
    /// off - asked again now and then.
    fn relief_back_decline(&mut self, why: &str) {
        let Some(b) = self.session.relief_back.as_mut() else { return };
        log::info!("relief: our bus not given back for line {} tour {} at {}: {why}", b.line, b.tour, hhmm_min(b.next.unwrap_or(0)));
        // (on the way: called off - asked for again from the menu)
        if b.way.is_some() {
            self.relief_back_cancel();
            self.service_msg = Some((tr(BACK_NO), 8.0));
            return;
        }
        b.sent = None;
        b.next = Some(0);
        b.ask_t = BACK_AGAIN;
        if b.walker.take().is_some() {
            self.relief_back_walker_gone();
        }
        self.service_msg = Some((tr(BACK_NO), 8.0));
    }

    /// The AI's driver's figure gone.
    fn relief_back_walker_gone(&mut self) {
        if let (Some(w), Some(h), Some(r), Some(scene)) = (self.world.clone(), self.session.humans.as_mut(), self.renderer.as_ref(), self.scene.as_mut()) {
            h.avatar_remove(DRIVER_KEY);
            crate::view_sync::apply_people(h, &mut self.gfx.sim_view.people, &w, r, scene);
        }
    }

    /// The AI's driver walks up to the front door and in to his place: whether he is there (his
    /// figure stays there until the world has the bus).
    fn relief_back_walk(&mut self, dt: f32) -> bool {
        let Some(w) = self.world.clone() else { return false };
        let Some(d) = self.session.relief_back.as_mut().and_then(|b| b.walker.as_mut()) else { return false };
        d.t += dt;
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
        let done = d.k >= d.path.len();
        // (on the ground up to the door, then on the bus's floor)
        let floor = (d.k >= 2).then_some(d.floor);
        let (pos, heading, kind) = (d.pos, d.heading, d.kind);
        let vel = if done {
            DVec2::ZERO
        } else {
            let h = heading.to_radians();
            DVec2::new(h.sin(), h.cos()) * DRIVER_WALK
        };
        if let (Some(h), Some(r), Some(scene)) = (self.session.humans.as_mut(), self.renderer.as_ref(), self.scene.as_mut()) {
            h.avatar(DRIVER_KEY, &w, AvatarCmd { pos, heading, vel, lift: 0.0, seat: None, floor, aboard: None }, kind);
            crate::view_sync::apply_people(h, &mut self.gfx.sim_view.people, &w, r, scene);
        }
        done
    }

    /// The AI's driver in his place: the bus given back to the world (`relief back`, sent again
    /// until it answers - `relief_back_sent`).
    fn relief_back_send(&mut self) {
        let Some(b) = self.session.relief_back.as_mut() else { return };
        let Some(next) = b.next.filter(|n| *n > 0) else { return };
        b.sent = Some(0.0);
        b.ask_t = BACK_EVERY;
        let msg = format!("relief back {};{};{next}", b.line, b.tour);
        log::info!("relief: our bus given back to the AI for line {} tour {} at {}", b.line, b.tour, hhmm_min(next));
        if let Some(l) = self.net.lan.as_mut() {
            l.command(1, &msg);
        }
    }

    /// (31 October 2026) The world has our bus (`relief backed`): it stays where it stands, the AI's
    /// driver at the wheel, until the AI's bus that takes it over stands in its place in our world (a
    /// bus of the world's within `BACK_SWAP_NEAR`, the same way round), or for `BACK_SWAP_MAX` at
    /// most - then it goes (`relief_back_done`), and the AI's drives on from there. It went at once
    /// before, and the AI's came seconds later, or at the stop's own place.
    fn relief_back_swap(&mut self, dt: f32) {
        let Some(t) = self.session.relief_back.as_mut().and_then(|b| b.swap.as_mut()) else { return };
        *t += dt;
        let waited = *t;
        let here = self.player.as_ref().map(|p| (p.vehicle.position, p.vehicle.heading));
        let came = here.is_some_and(|(at, heading)| {
            self.session.traffic.as_ref().is_some_and(|tr| {
                tr.cars.iter().any(|c| c.is_bus() && !c.gone && (c.vehicle.position - at).truncate().length() < BACK_SWAP_NEAR && omsi_sim::traffic::wrap_deg((c.vehicle.heading - heading) as f32).abs() < 45.0)
            })
        });
        if came || here.is_none() || waited > BACK_SWAP_MAX {
            if !came {
                log::info!("relief: the AI's bus did not show in our bus's place in {waited:.1} s: ours goes all the same");
            }
            self.relief_back_done();
        }
    }

    /// Our bus is the world's: it goes from our world (the AI's drives on in its place), and its
    /// driver's figure with it (the AI's bus has its own).
    fn relief_back_done(&mut self) {
        let Some(b) = self.session.relief_back.take() else { return };
        log::info!("relief: the AI has our bus");
        let way = b.way.is_some();
        self.relief_back_walker_gone();
        self.relief_drop_bus();
        // (on the way: our duty ends - the AI drives the rest, our company is told by the
        // world; free drive on foot, and free for another duty)
        if way {
            crate::game_lists::end_duty(self);
            self.service_msg = Some((tr(WAY_DONE), 15.0));
        } else {
            self.service_msg = Some((tr(BACK_DONE), 8.0));
        }
    }

    /// Our bus given back, the world's answer awaited: sent again now and then - no answer for
    /// long, the bus stays ours.
    fn relief_back_sent(&mut self, dt: f32) {
        if self.player.is_none() {
            self.relief_back_cancel();
            return;
        }
        let Some(b) = self.session.relief_back.as_mut() else { return };
        let Some(t) = b.sent.as_mut() else { return };
        *t += dt;
        if *t > BACK_GIVE_UP {
            self.relief_back_decline("no answer");
            return;
        }
        b.ask_t -= dt;
        if b.ask_t <= 0.0 {
            b.ask_t = BACK_EVERY;
            let msg = format!("relief back {};{};{}", b.line, b.tour, b.next.unwrap_or(0));
            if let Some(l) = self.net.lan.as_mut() {
                l.command(1, &msg);
            }
        }
    }

    /// The world's answer about the end of our duty (`relief end`, `relief backed`, and `no` or
    /// `unknown` to either): whether it was one.
    fn relief_back_answer(&mut self, verb: &str, f: &[&str]) -> bool {
        let Some(b) = self.session.relief_back.as_mut() else { return false };
        let dep = f.first().and_then(|x| x.trim().parse::<i32>().ok());
        let sent = b.sent.is_some();
        match verb {
            "no" | "unknown" if sent && dep == b.next => {
                let why = f.get(1).map(|x| x.trim().to_string()).unwrap_or_else(|| "unknown".into());
                self.relief_back_decline(&why);
                true
            }
            "end" | "unknown" if dep == Some(b.last) => {
                b.heard = true;
                if sent {
                    // (an answer to an earlier ask, the bus given back meanwhile)
                    return true;
                }
                let next = if verb == "end" { f.get(1).and_then(|x| x.trim().parse::<i32>().ok()).unwrap_or(0) } else { 0 };
                if b.next != Some(next) {
                    let at = if b.way.is_some() { "we hand our bus over on" } else { "our duty ends with" };
                    match next {
                        0 => log::info!("relief: {at} line {} tour {} at {}: nothing to hand back", b.line, b.tour, hhmm_min(b.last)),
                        n => log::info!("relief: {at} line {} tour {} at {}: the AI drives the {} on with our bus", b.line, b.tour, hhmm_min(b.last), hhmm_min(n)),
                    }
                    b.next = Some(next);
                    b.told_t = 0.0;
                    // (on the way: not here - called off)
                    if next == 0 && b.way.is_some() {
                        self.relief_back_cancel();
                        self.service_msg = Some((tr(WAY_NO), 8.0));
                        return true;
                    }
                    if next == 0 && b.walker.take().is_some() {
                        self.relief_back_walker_gone();
                        self.service_msg = Some((tr(BACK_NO), 8.0));
                    }
                }
                true
            }
            "backed" if sent && dep == b.next => {
                // (31 October 2026: ours stays in its place until the AI's stands there -
                // `relief_back_swap`)
                if b.swap.is_none() {
                    log::info!("relief: the AI has our bus: it goes once the AI's stands in its place");
                    b.swap = Some(0.0);
                }
                true
            }
            _ => false,
        }
    }

    /// (27 October 2026) Our bus given on to another player (it was his trip `given`: line, tour,
    /// departure), our duty going on with a trip of another tour: we wait for that trip's bus at
    /// its first stop, as a relief - the AI (or another player) brings it.
    fn relief_next_trip(&mut self, given: &(String, String, i32)) {
        let Some(d) = self.session.duty.as_mut() else { return };
        let of = |d: &crate::schedule::PlayerDuty, k: usize| -> (String, String) {
            match d.tours.get(k) {
                Some((l, t, _)) => (l.trim().to_string(), t.trim().to_string()),
                None => (d.line.trim().to_string(), d.tour.trim().to_string()),
            }
        };
        // (the trip we drove part of is over for us)
        let mut k = if d.trip_done() || d.left_late().is_some() { d.trip_index + 1 } else { d.trip_index };
        // (not the trip given on itself: a relief on the way)
        if d.trips.get(k).is_some_and(|t| {
            let (l, tour) = of(d, k);
            l.eq_ignore_ascii_case(&given.0) && tour.eq_ignore_ascii_case(&given.1) && (t.departure / 60.0).round() as i32 == given.2
        }) {
            k += 1;
        }
        let Some(trip) = d.trips.get(k) else { return };
        let (line, tour) = of(d, k);
        let dep = (trip.departure / 60.0).round() as i32;
        let stop_name = trip.stops.first().map(|s| s.name.trim().to_string()).unwrap_or_default();
        let number = trip.line.trim().to_string();
        d.start_at(k, 0);
        log::info!("relief: our duty goes on with line {line} tour {tour} at {}: waiting for its bus at '{stop_name}'", hhmm_min(dep));
        self.service_msg = Some((tr(NEXT_BUS).replace("%{line}", &number).replace("%{time}", &hhmm_min(dep)).replace("%{stop}", &stop_name), 12.0));
        self.session.relief = Some(Relief { line, tour, dep, stop: 0, trip: k, stop_name, phase: Phase::Waiting, ask_t: 0.0, take_t: 0.0, car: None, told_here: false, from: None, unheard: 0.0, heard: false, bus: None, told_bus: false });
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

/// The way a driver takes between his place in bus `v` and the kerb by its front door: his place,
/// the door on the ground, a place away from the bus on the door's side and along the kerb a
/// little - and his figure (`car`: the AI's bus he drove, for one when the map has no driver).
fn driver_way(humans: Option<&mut crate::humans::Humans>, w: &crate::scene::World, v: &omsi_sim::VehicleInstance, car: u64) -> (DVec3, DVec3, DVec3, u64) {
    let (kind, seat, door) = match humans {
        Some(h) => {
            let kind = crate::driver::driver_type(w, 1).map(|t| h.type_index(t) as u64);
            let seat = h.driver_stand(v).and_then(|l| h.vehicle_cabin_world(v, l));
            (kind, seat, h.vehicle_driver_door(v))
        }
        None => (None, None, None),
    };
    let kind = kind.unwrap_or(car.wrapping_mul(7) + 11);
    let hd = v.heading.to_radians();
    let (fwd, right) = (DVec2::new(hd.sin(), hd.cos()), DVec2::new(hd.cos(), -hd.sin()));
    let half = v.ty.def.bounding_box.map(|b| (b[0] as f64 * 0.5, b[1] as f64 * 0.5 + b[4] as f64)).unwrap_or((1.25, 5.5));
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
    (seat, door_g, away, kind)
}

/// The duty ends halfway along its last trip (a relief on the way: its last `--duty-leg`'s
/// `trips@stop`) - not at a terminus where the AI takes the bus over.
fn duty_ends_on_the_way(args: &crate::cli::Args) -> bool {
    args.duty_legs.last().and_then(|l| omsi_launcher_lib::compose::Block::parse_between(l)).is_some_and(|x| x.2.is_some())
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

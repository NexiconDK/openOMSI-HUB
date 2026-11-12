//! What the launchers of a company session say to each other: a friend's launcher posts a
//! `Request` (JSON) to the boss's (`server`), with the code's secret and the company's tag, its
//! player's id and name, and what it wants; every answer carries the company's day as a driver
//! needs it (`Snapshot`) - its lines and duties with their times and who drives them, the
//! company's date and clock, who is online and what each drives, the notices since the last
//! answer, and the player's own results. Nothing of the money, the staff's wages or the planning's
//! rules goes out.
//!
//! What a player asks (`Request::what`): `hello`, `snapshot`, `take` / `release` a duty,
//! `start` driving it (answered with the duty's start in his own game), `report` its trips
//! (`done` at its end; ended early, the AI drives the rest), `take_back` a duty the AI drives
//! since he left it (from its next trip), `say` a line to everyone, `leave`; with the permission
//! for it, `assign`
//! / `unassign` a duty of a player (`target`), `answer` a request (`yes`), `set_role` of a
//! player (`role`, `perms`), `kick` one (`ban`), and `page`: a page of the company's own,
//! relayed to the boss's launcher (`page`: its command and arguments). A driver answers the
//! control room's messages to the duty his game drives (`msg_reply`: the message `msg`, the
//! answer's key in `text`) and tells control from it (`msg_tell`: the quick button's key); the
//! snapshot carries those messages (`Snapshot::messages`, docs/omsihub/MESSAGES.md).
//!
//! (NexiconDK, Bus Company Online: single trips, 27 October 2026) A player looks for a trip to
//! drive on its own (`trips`, by `filter`: answered with a `TripList`), takes one (`take_trip`:
//! `duty` and `trip`, its place in the duty's trips) or the first free one the filter finds
//! (`next_trip`); he drives it as a duty (`start`), and the AI drives the tour on after it.

use super::super::duty_launch::DutyRef;
use super::book::GuestStats;
use super::roles::{Perm, Role};
use crate::TripRun;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Request {
    /// The company session's version (`code::VERSION`).
    pub v: u8,
    /// The code's secret (hex) and the company's tag.
    pub secret: String,
    pub company: u16,
    /// The player's own id (his launcher keeps it) and his name.
    pub player: String,
    pub name: String,
    #[serde(rename = "do")]
    pub what: String,
    pub duty: Option<DutyRef>,
    pub trips: Vec<TripRun>,
    /// The duty is over (its last trips are in `trips`).
    pub done: bool,
    pub text: String,
    /// What the player does now: the duty his game drives, and whether his game runs.
    pub driving: Option<DutyRef>,
    pub in_game: bool,
    /// The last notice the player has (only newer ones come).
    pub since: u64,
    /// The player an action is about (`assign`, `set_role`, `kick`), and what it sets.
    pub target: u32,
    pub yes: bool,
    pub ban: bool,
    pub role: Role,
    pub perms: Vec<Perm>,
    /// A page of the company relayed (`{ "cmd": …, "args": … }`).
    pub page: Value,
    /// The company's logo wanted with the answer (the player's page does not have it yet).
    pub logo: bool,
    /// The control room's message a `msg_reply` answers.
    pub msg: u64,
    /// (NexiconDK) Bus Company Online: the main server's ticket for this player and company
    /// (`online::ticket`) - a company on the server lets nobody in without it.
    pub ticket: String,
    /// (NexiconDK, single trips) The trip of `duty` a `take_trip` takes (an index into its trips),
    /// and what `trips` and `next_trip` look for.
    pub trip: usize,
    pub filter: super::book::TripFilter,
}

/// The company as the friend's page shows it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Card {
    pub id: String,
    pub name: String,
    pub short: String,
    pub colours: [String; 2],
    pub map: String,
    pub map_name: String,
    /// The boss (his profile's name).
    pub boss: String,
    /// The logo as the page shows it (a `data:` address; only when asked for: `Request::logo`).
    pub logo: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapLine {
    pub line: String,
    pub number: String,
    pub colour: String,
}

/// Who a duty is for, as the friend's page tells it.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DutyState {
    /// Nobody drives it: it may be taken (or asked for).
    #[default]
    Open,
    /// It cannot be taken: begun, no bus, driven already.
    Closed,
    /// Somebody of the company drives it (staff, the agency, the boss).
    Company,
    /// Another player has it.
    Player,
    /// Mine: planned for me, asked for by me, driven by me now, driven.
    Mine,
    Asked,
    Driving,
    Done,
    /// Mine, its day closed before I drove it: still mine to drive (`book::Claim::late`).
    Late,
    /// Mine, and the AI drives it since I left it (`book::AiPart`): taken back from its next trip.
    Ai,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapDuty {
    pub duty: DutyRef,
    /// The number its line shows, the tour as the timetable names it.
    pub number: String,
    pub colour: String,
    pub from: i32,
    pub to: i32,
    /// Where it signs on and off, its trips with passengers.
    pub first: String,
    pub last: String,
    pub trips: usize,
    pub km: f64,
    /// Its bus (fleet number and model; empty: none yet) and the bus's file (to see that it is
    /// installed where it is driven).
    pub bus: String,
    pub file: String,
    pub state: DutyState,
    /// Who drives it (a name; empty: nobody), and the player who has it (0: none).
    pub who: String,
    pub guest: u32,
    /// Why it cannot be taken (`Closed`: English, a key of the translations).
    pub why: String,
    /// The AI drives it now: its player (`who`) left it (Beta 63). From which departure (minutes
    /// of the day), and whether its player may take it back at a trip still to come.
    pub ai: bool,
    pub ai_from: Option<i32>,
    pub back: bool,
    /// Nobody drives what its player left of it: the company drops it (`book::AiPart::dropped`).
    pub dropped: bool,
    /// (NexiconDK, Bus Company Online) The AI drives it since before its player had it: it had
    /// begun (`book::LeftWhy::Begun`) - he takes it over at its next trip, he did not leave it.
    /// (`Company` with `take_over`: it has begun, taken over at its next trip.)
    pub begun: bool,
    /// (NexiconDK, Bus Company Online, 28 October 2026) Its player handed his bus over to the AI on
    /// the way (`book::LeftWhy::HandedOver`): the AI drives the rest, he did not leave it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub handed: bool,
    /// (NexiconDK, Bus Company Online) `Company`: the company's AI driver (`who`) drives it on the
    /// server, and a player may take it over (`book::takes_over`) - from its start, or from its
    /// next trip when it has `begun`.
    pub take_over: bool,
    /// (NexiconDK, single trips) Its player (`guest`) has only these trips of it: the rest is its
    /// driver's (`book::Claim::part`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<SnapPart>,
    /// (NexiconDK, 3 November 2026, K6) Urgent: an admin's doubling or rail replacement - paid
    /// the urgent bonus to a player (`live_more::urgent`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub urgent: bool,
    /// (NexiconDK, 7 November 2026) The trip with passengers it is taken over at - its next on
    /// the company's clock when it is under way, else its first (`book::next_trip`): for the AI's
    /// duties to take over, and the player's own the AI drives until he takes it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next: Option<SnapNext>,
    /// (NexiconDK, 7 November 2026) The name the company's planning shows the duty by: RosterFlow
    /// Pro's own name of it, else its number ("2003"); empty when the company does not plan with
    /// RosterFlow Pro.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// (Round P, 9 November 2026) The player's own duty still to drive: the minutes to its sign-on
    /// on the company's clock (none or less: it has begun). None for every other duty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_on_in: Option<i32>,
    /// (Round R, 9 November 2026) The wishes of the players who said any it does not keep
    /// (`player_wish::misses_of`): for whoever gives it, and the player himself. Not begun only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub misses: Vec<SnapMiss>,
}

/// (Round R) A wish of player `guest` a duty does not keep: `kind` "avoids" (`kinds` the kinds
/// it is that he avoids), "wishes" (`kinds` those he wishes), "day_off" (`day` the weekday, 0
/// Monday, or None: the date) or "hours" (`have` with it, `max` - minutes a week).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapMiss {
    pub guest: u32,
    pub kind: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub day: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub have: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i32>,
}

impl SnapMiss {
    pub fn of(guest: u32, m: &super::super::player_wish::Miss) -> SnapMiss {
        use super::super::player_wish::Miss;
        let mut x = SnapMiss { guest, ..Default::default() };
        match m {
            Miss::Avoids(k) => (x.kind, x.kinds) = ("avoids".into(), vec![k.key().into()]),
            Miss::NotWished(ks) => (x.kind, x.kinds) = ("wishes".into(), ks.iter().map(|k| k.key().to_string()).collect()),
            Miss::DayOff(d) => (x.kind, x.day) = ("day_off".into(), *d),
            Miss::Hours { have, max } => (x.kind, x.have, x.max) = ("hours".into(), Some(*have), Some(*max)),
        }
        x
    }

    /// Back to the wish it says.
    pub fn miss(&self) -> Option<super::super::player_wish::Miss> {
        use super::super::player_wish::{Kind, Miss};
        let ks = || self.kinds.iter().filter_map(|k| Kind::from_key(k)).collect::<Vec<_>>();
        Some(match self.kind.as_str() {
            "avoids" => Miss::Avoids(*ks().first()?),
            "wishes" => Miss::NotWished(ks()),
            "day_off" => Miss::DayOff(self.day),
            "hours" => Miss::Hours { have: self.have?, max: self.max? },
            _ => return None,
        })
    }
}

/// (NexiconDK, 7 November 2026) A trip to take a duty over at: when it leaves (minutes of the
/// day), from where and to where.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapNext {
    pub dep: i32,
    pub from: String,
    pub to: String,
}

/// (NexiconDK, single trips) The trips of a duty a player took on their own: from where and when
/// the first leaves to where and when the last arrives, how many carry passengers.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapPart {
    pub trip: usize,
    pub dep: i32,
    pub arr: i32,
    pub from: String,
    pub to: String,
    pub trips: usize,
}

/// (NexiconDK, single trips) A trip of the company's day free to take on its own
/// (`book::free_trips`): its duty and its place in its trips, its line's number and colour, its
/// times and stops, its bus.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapTrip {
    pub duty: DutyRef,
    pub trip: usize,
    pub number: String,
    pub colour: String,
    pub dep: i32,
    pub arr: i32,
    pub from: String,
    pub to: String,
    pub stops: u32,
    pub km: f64,
    pub bus: String,
    pub file: String,
}

/// (NexiconDK, single trips) The answer to `trips`: whether the company lets its players take
/// single trips, the lines and the stops there are free trips of (for the filter: the stops of the
/// line chosen), the free trips the filter finds (the soonest first, at most `LISTED`), and how many
/// more there are.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct TripList {
    pub open: bool,
    pub lines: Vec<SnapLine>,
    pub stops: Vec<String>,
    pub trips: Vec<SnapTrip>,
    pub more: usize,
}

impl TripList {
    pub const LISTED: usize = 60;
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapPlayer {
    /// His number in the company (0: the boss).
    pub no: u32,
    pub name: String,
    pub me: bool,
    pub boss: bool,
    pub online: bool,
    pub role: Role,
    /// What he drives now (line number, tour, duty; None: nothing).
    pub driving: Option<(String, String, usize)>,
}

/// Something that happened, told to everyone: an English text with its arguments.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Notice {
    pub n: u64,
    /// Unix seconds.
    pub at: u64,
    pub text: String,
    pub args: Vec<(String, String)>,
}

/// A duty a player drove for the company, as its trips came to (`career::evaluate`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapResult {
    pub duty: DutyRef,
    pub number: String,
    pub trips: usize,
    pub passengers: i32,
    pub km: f64,
    /// The trips' average score (0 - 100) and their experience points together.
    pub score: u32,
    pub xp: i64,
    pub on_time: u32,
    pub stops: u32,
    /// What the trips bring in as the day's close books them: the fares and the authority's
    /// payment (cents).
    pub fares: i64,
    pub done: bool,
    /// The trips of the duty the AI drives since he left it (Beta 63).
    pub ai_trips: usize,
}

/// The player himself: his role, what he may do, what he drove for the company.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapYou {
    pub no: u32,
    pub name: String,
    /// (NexiconDK) He founded the company on Bus Company Online's server: its owner, for good.
    pub owner: bool,
    pub role: Role,
    pub perms: Vec<Perm>,
    pub stats: GuestStats,
}

/// A player of the company as one who manages players (or gives duties) sees him.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapMember {
    pub no: u32,
    pub name: String,
    /// (NexiconDK) The company's owner on Bus Company Online's server.
    pub owner: bool,
    pub role: Role,
    pub perms: Vec<Perm>,
    pub online: bool,
    pub banned: bool,
    /// His duties of the day.
    pub duties: usize,
    pub stats: GuestStats,
}

/// A duty a player asked for, as one who may approve sees it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapAsk {
    pub duty: DutyRef,
    pub number: String,
    pub from: i32,
    pub to: i32,
    pub no: u32,
    pub name: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Snapshot {
    pub card: Card,
    pub date: String,
    /// The company's clock: the minute of its day.
    pub minute: i32,
    pub open_claims: bool,
    pub lines: Vec<SnapLine>,
    pub duties: Vec<SnapDuty>,
    pub players: Vec<SnapPlayer>,
    pub notices: Vec<Notice>,
    pub results: Vec<SnapResult>,
    /// The day's timetable is being read (no duties yet).
    pub reading: bool,
    /// The player himself.
    pub you: Option<SnapYou>,
    /// The company's players (to one who gives duties or manages players), the duties asked for
    /// (to one who may approve them).
    pub members: Vec<SnapMember>,
    pub asks: Vec<SnapAsk>,
    /// The control room's messages of the duty the player's game drives, the newest first.
    pub messages: Vec<super::super::messages::Msg>,
    /// (NexiconDK, Bus Company Online) Its players may take single trips (`book::free_trips`).
    pub trips_open: bool,
    /// (NexiconDK, Bus Company Online: free drives) The company's buses as the player may take
    /// one out (none: not on the server, or he may not drive), and the one he has out.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub free_buses: Vec<SnapBus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_drive: Option<SnapBus>,
    /// (NexiconDK, 3 November 2026: `live_more`) What an urgent task pays on top (K6, 0.5: 50 %),
    /// the player's fixed duties (K10), and the dispatcher's duty (K13; None: not on the server).
    #[serde(default)]
    pub urgent_bonus: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fixed: Vec<SnapFixed>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dispatch: Option<SnapDispatch>,
    /// (NexiconDK, 9 November 2026: round Q) The duties of the day without a driver signing on
    /// soon (`pro_watch::unfilled`), for whoever gives or plans the duties.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unfilled: Vec<SnapUnfilled>,
    /// (NexiconDK, ticket inspection round V) The ticket-inspection duties the player may take
    /// now (none: not on the server, or he may not drive), and the one he has today with the
    /// drivers' calls to it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patrols: Vec<super::super::inspection_patrol::Offer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patrol: Option<super::super::inspection_patrol::PlayerPatrol>,
}

/// (NexiconDK, 9 November 2026: round Q) A duty without a driver signing on soon: the name it is
/// shown by, when it signs on (minutes of the day) and in how many minutes.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapUnfilled {
    pub name: String,
    pub start: i32,
    pub within: i32,
}

/// (K10) A fixed duty as the player's page lists it: its id, line number, tour, duty, weekdays
/// (bit 0 Monday), bonus and the player.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapFixed {
    pub id: u32,
    pub number: String,
    pub tour: String,
    pub duty: usize,
    pub days: u8,
    pub bonus: f64,
    pub name: String,
    pub mine: bool,
}

/// (K13) The dispatcher's duty: who is on it (empty: nobody), whether it is the player and
/// whether he may take it, and - for him - the late buses of the day now.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapDispatch {
    pub name: String,
    pub mine: bool,
    pub may: bool,
    /// The late bus (line / tour · bus) and its minutes late; the player's launcher words it.
    pub late: Vec<(String, i32)>,
}

/// (NexiconDK, free drives) A bus of the company as the player's page lists it to take out: its
/// number, model, odometer, tank (0 to 1) and fuel use, and whether it is free (until a minute of
/// the day; None the rest of it) or why not (`free_drive::FreeBus::busy`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct SnapBus {
    pub id: u32,
    pub number: String,
    pub name: String,
    pub file: String,
    pub electric: bool,
    pub km: f64,
    pub fuel: f64,
    pub fuel_use: String,
    pub until: Option<i32>,
    pub busy: Option<String>,
}

/// How a player's game starts on his duty (`duty_launch::Start`, which the boss's launcher works
/// out: the friend's has no company): in his own game, alone, at the duty's day and time.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct DriveStart {
    pub company: String,
    pub map: String,
    pub date: String,
    pub time: i32,
    pub line: String,
    pub tour: String,
    pub first: usize,
    pub legs: Vec<String>,
    pub bus: String,
    pub paint: String,
    pub number: String,
    pub plate: String,
    pub hof: String,
    /// A relief on the way: where the duty takes the bus over, and where it hands it on.
    pub from_stop: Option<usize>,
    pub to_stop: Option<usize>,
    /// (NexiconDK, single trips) How many trips of the tour after his he may drive on with, one
    /// at a time, at the end of his (his game's "Drive on", `--drive-on`): the rest of the duty.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub more: usize,
    /// (NexiconDK, Bus Company Online, 2 November 2026) The fleet's bus (its id), its odometer
    /// (km) and what is in its tank, 0 to 1 (`duty_launch::SheetBus`): his game in the server's
    /// world starts the bus so (`--odometer`, `--fuel-level`). None: a boss's launcher of before.
    #[serde(default, skip_serializing_if = "is_zero_u32")]
    pub vehicle: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub km: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fuel: Option<f64>,
    /// (NexiconDK, Bus Company Online: a free drive with a bus of the company, `free_drive`) No
    /// duty: the bus from the company's garage (where its run out of the depot `garage` begins),
    /// at the company's time; in the game's "Line and tour" only the company's lines (`lines`, the
    /// timetable's names), each tour taken over from the AI or driven behind it as an extra bus.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub free: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub garage: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<String>,
    /// (NexiconDK, faults) The bus's scripts' faults as the company keeps them, and its defects not
    /// repaired yet (`bus_faults::at_start`): set again, and told, at the start.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fault_state: Vec<(String, f32)>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub defects: Vec<String>,
    /// (NexiconDK, ticket inspection, 12 November 2026) The company's ticket inspection on the
    /// duty in his game (`inspection_game::for_duty`): the riders without a ticket, the company's
    /// inspectors boarding some of its trips, whether he may ask control for them. It was the
    /// boss's own launcher's alone: a joined player - every player of Bus Company Online - saw
    /// inspectors only when he asked for them. None: none today, or a boss's launcher of before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inspection: Option<super::super::inspection_game::ForGame>,
    /// (NexiconDK, 12 November 2026) The company's inspectors' clothes, as the server lists its
    /// companies (`server_api::inspectors_json` beside `short`, `name` and `colours`): his
    /// launcher makes them before his game starts. They came only from the list of the world's
    /// companies his launcher had last read, and the inspectors came in their own clothes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uniform: Option<Value>,
    /// (NexiconDK, ticket inspection round V) Not a drive: a ticket-inspection duty - his game
    /// starts on foot at a stop of its line in the company's inspector's uniform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patrol: Option<PatrolStart>,
}

/// (round V) How a player's game starts on a ticket-inspection duty: the line (the timetable's
/// name) and its number, a tour of it whose bus passes a stop soon (where he starts), when the
/// duty ends (minute of the day), the fine (cents) and the uniform's clothes.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct PatrolStart {
    pub line: String,
    pub number: String,
    pub tour: String,
    pub until: i32,
    pub fine: i64,
    pub variant: String,
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

fn is_zero_u32(n: &u32) -> bool {
    *n == 0
}

impl DriveStart {
    /// The duty as the friend's launcher starts the game: `base` (his game options) with the
    /// duty's start, in a game of his own.
    pub fn duty(&self, base: crate::Duty) -> crate::Duty {
        let st = super::super::duty_launch::Start {
            company: self.company.clone(),
            map: self.map.clone(),
            date: self.date.clone(),
            time: self.time,
            line: self.line.clone(),
            tour: self.tour.clone(),
            first: self.first,
            legs: self.legs.clone(),
            from_stop: self.from_stop,
            to_stop: self.to_stop,
            bus: self.bus.clone(),
            paint: self.paint.clone(),
            number: self.number.clone(),
            plate: self.plate.clone(),
            hof: self.hof.clone(),
            vehicle: self.vehicle,
            km: self.km,
            fuel: self.fuel,
        };
        let mut d = st.duty(base, "");
        // (alone, not in a session; the friend has no company of his own to run: his trips go
        // to the boss's by his launcher)
        d.company = None;
        d.drive_on = (self.more > 0).then_some(self.more.min(99) as u32);
        // (a free drive: no line, no tour - from the garage, the company's lines to pick from)
        if self.free {
            d.line = None;
            d.tour = None;
            d.trip = None;
            d.legs = Vec::new();
            d.whole_tour = false;
            d.entry = None;
            d.drive_on = None;
            d.garage_trip = self.garage.clone();
            d.company_lines = self.lines.clone();
        }
        d
    }

    pub fn of(s: &super::super::duty_launch::Start) -> DriveStart {
        DriveStart {
            company: s.company.clone(),
            map: s.map.clone(),
            date: s.date.clone(),
            time: s.time,
            line: s.line.clone(),
            tour: s.tour.clone(),
            first: s.first,
            legs: s.legs.clone(),
            bus: s.bus.clone(),
            paint: s.paint.clone(),
            number: s.number.clone(),
            plate: s.plate.clone(),
            hof: s.hof.clone(),
            from_stop: s.from_stop,
            to_stop: s.to_stop,
            more: 0,
            free: false,
            garage: None,
            lines: Vec::new(),
            fault_state: Vec::new(),
            defects: Vec::new(),
            inspection: None,
            uniform: None,
            patrol: None,
            vehicle: s.vehicle,
            km: s.km.filter(|k| k.is_finite() && *k >= 0.0),
            fuel: s.fuel.filter(|f| f.is_finite()).map(|f| f.clamp(0.0, 1.0)),
        }
    }
}

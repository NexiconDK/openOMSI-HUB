//! (NexiconDK, 30 October 2026) The passengers' signals to the players' buses, between games:
//! a player's bus that left people waiting at their stop (`people::passed_by`) is told to
//! that player's game - ours to our plugins (`passengers_left`, which the trip counts as left
//! behind for the company), another LAN player's to his game, which tells its plugins - and on
//! a LAN host what its waiting people do for a bus (signal to it, glance at it full, wave after
//! it) is told to the clients, which draw the host's people (`PeopleSim::set_mirror_gesture`).
//!
//! The words, from the host to a player: `pax left <count> <stop> <full 0|1>` and
//! `pax gest <person> <hail|full|wave|off> <x> <y>`. A game without them lets them go.
//!
//! (31 October 2026) A client's bus takes the host's waiting people at a stop as it comes
//! (`claim_waiting`): from then on they are the client's, and the host knows nothing of what
//! they do. So the client looks itself at its bus leaving them (`passed_tick`; one time with
//! what the host tells of its own people there, `hold`), and tells the host what its people do
//! for a bus - `pax gest <person> <hail|full|wave|off> <x> <y> <z>`, by its ids on the wire
//! (`lan_world::wire_up`) - which the host draws and tells the other players
//! (`LanWorld::relayed_gestures`).

use glam::{DVec2, DVec3};
use hashbrown::HashMap;
use omsi_sim::people::{remote_bus_player, BusId, Gesture};
use std::sync::atomic::{AtomicBool, Ordering};

/// How often the host looks at what its people do for a bus (s).
const EVERY: f32 = 0.25;

/// (31 October 2026) A player's game is told what the host's people do for a bus this near him
/// (m, from the bus): his game has the people within `lan_world::PERSON_RADIUS` of him, the bus
/// at most 60 m from them. Every gesture went to every player before - most of them for people
/// his game does not have, a flood that could crowd out the ones it has.
const GESTURE_RADIUS: f64 = crate::lan_world::PERSON_RADIUS + 80.0;

/// How often the host logs how many gestures it told (s).
const LOG_EVERY: f32 = 60.0;

/// What the host told the clients its people do (`host_tick`).
#[derive(Default)]
pub(crate) struct PaxSignals {
    acc: f32,
    /// What each player's game was told, by player and person.
    sent: HashMap<u32, HashMap<u32, Gesture>>,
    /// The gestures told since the log said so last (signals, glances at a full bus, waves), and
    /// how long since then.
    told: [u32; 3],
    log_t: f32,
    /// (client) The times our bus left people, held a moment for the host's word of the same
    /// stop (ours and the host's people there are told as one): stop, count, full, how long held.
    held: Vec<(i64, u32, bool, f32)>,
}

/// How long a client holds the times its bus left people for the host's word of the same stop
/// (s): both see the bus 12 m past the stop at much the same time.
const HOLD: f32 = 3.0;

/// The host's id in a LAN session (`LanSession::command`).
const HOST: u32 = 1;

/// The words for each player (`pax gest ...`): of what the people do for a bus now (`now`: by
/// person, the gesture, where that bus is and whose game the person is - not told to that
/// one), what is new to him or over - near him only (`GESTURE_RADIUS`; one whose place is not
/// known, all). `sent`: what each was told, kept.
pub(crate) fn gesture_words(now: &HashMap<u32, (Gesture, DVec3, Option<u32>)>, peers: &[(u32, Option<DVec2>)], sent: &mut HashMap<u32, HashMap<u32, Gesture>>) -> Vec<(u32, String)> {
    sent.retain(|id, _| peers.iter().any(|p| p.0 == *id));
    let mut out = Vec::new();
    for &(peer, at) in peers {
        let near: HashMap<u32, Gesture> = now
            .iter()
            .filter(|(_, (_, bus, whose))| *whose != Some(peer) && at.is_none_or(|a| (bus.truncate() - a).length() < GESTURE_RADIUS))
            .map(|(id, (g, _, _))| (*id, *g))
            .collect();
        let told = sent.entry(peer).or_default();
        for (id, g) in &near {
            if told.get(id) != Some(g) {
                let at = now[id].1;
                out.push((peer, format!("pax gest {id} {} {:.1} {:.1} {:.1}", g.word(), at.x, at.y, at.z)));
            }
        }
        for id in told.keys().filter(|id| !near.contains_key(*id)) {
            out.push((peer, format!("pax gest {id} off 0 0")));
        }
        *told = near;
    }
    out
}

/// One time our bus left people at a stop: how many, the stop (its map object), and whether
/// it was full.
pub(crate) type LeftHere = (u32, i64, bool);

impl PaxSignals {
    /// Once a frame, where the people are simulated (alone, the LAN host, a dedicated
    /// server): the times our bus left people (returned, for the plugins), another LAN
    /// player's told to his game; and on a host, what its waiting people do for a bus, to the
    /// clients.
    /// `relayed`: what the clients' people do for a bus, by their ids in the other games
    /// (`LanWorld::relayed_gestures`).
    pub(crate) fn host_tick(&mut self, dt: f32, lan: Option<&mut omsi_net::LanSession>, h: &mut crate::humans::Humans, relayed: &[(u32, Gesture, DVec3, u32)]) -> Vec<LeftHere> {
        let mut mine = Vec::new();
        let left = h.take_left_behind();
        let host = lan.filter(|l| l.role == omsi_net::Role::Host);
        let Some(l) = host else {
            mine.extend(left.iter().filter(|x| x.bus == BusId::Player).map(|x| (x.count, x.stop, x.full)));
            return mine;
        };
        for x in &left {
            match x.bus {
                BusId::Player => mine.push((x.count, x.stop, x.full)),
                BusId::Ai(k) => {
                    if let Some(player) = remote_bus_player(k) {
                        log::info!("LAN: player {player}'s bus left {} waiting at stop {}{}", x.count, x.stop, if x.full { " (full)" } else { "" });
                        l.command(player, &format!("pax left {} {} {}", x.count, x.stop, x.full as u8));
                    }
                }
            }
        }
        self.acc += dt;
        if self.acc < EVERY {
            return mine;
        }
        self.acc = 0.0;
        let mut now: HashMap<u32, (Gesture, DVec3, Option<u32>)> = h.lan_gestures().into_iter().map(|(id, g, at)| (id, (g, at, None))).collect();
        now.extend(relayed.iter().map(|&(id, g, at, whose)| (id, (g, at, Some(whose)))));
        // (each player's game, where he is: his bus, else his walker)
        let peers: Vec<(u32, Option<DVec2>)> = l
            .peers()
            .filter(|p| p.pose.id != l.my_id)
            .map(|p| {
                let at = if !p.has_pose {
                    None
                } else if p.pose.has_vehicle() {
                    Some(DVec2::new(p.pose.x, p.pose.y))
                } else {
                    p.pose.walker.map(|w| DVec2::new(w.x, w.y))
                };
                (p.pose.id, at)
            })
            .collect();
        let words = gesture_words(&now, &peers, &mut self.sent);
        for (peer, text) in &words {
            // (" wave": after a bus, and to one coming - "waveto")
            for (k, w) in [" hail ", " full ", " wave"].iter().enumerate() {
                if text.contains(w) {
                    self.told[k] += 1;
                }
            }
            l.command(*peer, text);
        }
        // (how many were told, now and then: whether the players' games hear of any)
        self.log_t += EVERY;
        if self.log_t >= LOG_EVERY {
            self.log_t = 0.0;
            if self.told.iter().any(|n| *n > 0) {
                log::info!("LAN: our waiting people's gestures told the players in the last {LOG_EVERY:.0} s: {} signals to a bus, {} glances at a full one, {} waves to or after one", self.told[0], self.told[1], self.told[2]);
            }
            self.told = [0; 3];
        }
        mine
    }

    /// (NexiconDK, 31 October 2026) Once a frame on a LAN client: the times our bus left people
    /// (returned, for the plugins, once held for the host's word - `hold`), and what our own
    /// people do for a bus, to the host.
    pub(crate) fn client_tick(&mut self, dt: f32, lan: &mut omsi_net::LanSession, h: &mut crate::humans::Humans) -> Vec<LeftHere> {
        for x in h.take_left_behind() {
            if x.bus == BusId::Player {
                log::info!("LAN: our bus left {} of ours waiting at stop {}{}", x.count, x.stop, if x.full { " (full)" } else { "" });
                self.hold((x.count, x.stop, x.full));
            }
        }
        self.acc += dt;
        if self.acc >= EVERY {
            self.acc = 0.0;
            let now: HashMap<u32, (Gesture, DVec3, Option<u32>)> = h
                .lan_gestures()
                .into_iter()
                .map(|(id, g, at)| (crate::lan_world::wire_up(id), (g, at, None)))
                .collect();
            for (_, text) in gesture_words(&now, &[(HOST, None)], &mut self.sent) {
                lan.command(HOST, &text);
            }
        }
        self.due(dt)
    }

    /// (client) A time our bus left people - ours, or the host's word of its own: one with
    /// another of the same stop held now.
    pub(crate) fn hold(&mut self, (count, stop, full): LeftHere) {
        match self.held.iter_mut().find(|x| x.0 == stop) {
            Some(x) => {
                x.1 += count;
                x.2 |= full;
            }
            None => self.held.push((stop, count, full, 0.0)),
        }
    }

    /// (client) The times held long enough (`HOLD`), for the plugins.
    fn due(&mut self, dt: f32) -> Vec<LeftHere> {
        let mut out = Vec::new();
        self.held.retain_mut(|x| {
            x.3 += dt;
            if x.3 < HOLD {
                return true;
            }
            out.push((x.1, x.0, x.2));
            false
        });
        out
    }
}

/// `pax gest` (its rest after "gest"): whose person, and what they do for a bus near where
/// (None: no more; z only from a client - the host's words have none).
pub(crate) fn gesture_word<'a>(mut w: impl Iterator<Item = &'a str>) -> Option<(u32, Option<(Gesture, DVec3)>, bool)> {
    let id = w.next()?.parse::<u32>().ok()?;
    let word = w.next()?;
    let mut num = || w.next().and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite());
    let (x, y) = (num().unwrap_or(0.0), num().unwrap_or(0.0));
    let z = num();
    Some((id, Gesture::of_word(word).map(|g| (g, DVec3::new(x, y, z.unwrap_or(0.0)))), z.is_some()))
}

/// What a word of the host (`pax ...`, its rest `rest`) means for a client: a gesture of one
/// of its people put on ours, or our bus having left people (returned).
pub(crate) fn client_word(rest: &str, h: Option<&mut crate::humans::Humans>) -> Option<LeftHere> {
    let mut w = rest.split_whitespace();
    match w.next()? {
        "left" => {
            let count = w.next()?.parse::<u32>().ok()?.min(500);
            let stop = w.next()?.parse::<i64>().ok()?;
            let full = w.next()? == "1";
            (count > 0).then_some((count, stop, full))
        }
        "gest" => {
            let (id, g, _) = gesture_word(w)?;
            // (once a game: that the host tells them - for the log)
            static HEARD: AtomicBool = AtomicBool::new(false);
            if let Some((g, _)) = g {
                if !HEARD.swap(true, Ordering::Relaxed) {
                    log::info!("LAN: the host tells what its waiting people do for a bus (first: person {id}, {})", g.word());
                }
            }
            if let Some(h) = h {
                // (where the bus is, at the person's height: they look at it)
                let z = h.people.iter().find(|p| p.id == id).map_or(0.0, |p| p.position.z);
                h.set_mirror_gesture(id, g.map(|(g, at)| (g, DVec3::new(at.x, at.y, z))));
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (31 October 2026) Each player is told what the host's people do near him only, what is
    /// new to him, and when it is over - also when he goes away from it.
    #[test]
    fn each_player_hears_of_the_gestures_near_him() {
        let mut sent = HashMap::new();
        let mut now: HashMap<u32, (Gesture, DVec3, Option<u32>)> = HashMap::new();
        now.insert(1, (Gesture::Hail, DVec3::new(10.0, 0.0, 0.0), None));
        now.insert(2, (Gesture::Wave, DVec3::new(5000.0, 0.0, 0.0), None));
        let peers = [(7, Some(DVec2::ZERO)), (8, Some(DVec2::new(5000.0, 50.0))), (9, None)];
        let mut w = gesture_words(&now, &peers, &mut sent);
        w.sort();
        assert_eq!(w, vec![(7, "pax gest 1 hail 10.0 0.0 0.0".to_string()), (8, "pax gest 2 wave 5000.0 0.0 0.0".to_string()), (9, "pax gest 1 hail 10.0 0.0 0.0".to_string()), (9, "pax gest 2 wave 5000.0 0.0 0.0".to_string())]);
        assert!(gesture_words(&now, &peers, &mut sent).is_empty(), "told once");
        // the signal over; player 8 gone away from the wave
        now.remove(&1);
        let peers = [(7, Some(DVec2::ZERO)), (8, Some(DVec2::new(0.0, 0.0))), (9, None)];
        let mut w = gesture_words(&now, &peers, &mut sent);
        w.sort();
        assert_eq!(w, vec![(7, "pax gest 1 off 0 0".to_string()), (8, "pax gest 2 off 0 0".to_string()), (9, "pax gest 1 off 0 0".to_string())]);
        // a player gone from the session is forgotten
        assert!(gesture_words(&now, &[(9, None)], &mut sent).is_empty() && !sent.contains_key(&7));
        // (31 October 2026) a client's person, passed on by the host: not told to that client
        let mut sent = HashMap::new();
        let mut now = HashMap::new();
        now.insert(0xC0_0001, (Gesture::Hail, DVec3::new(10.0, 0.0, 2.0), Some(8)));
        let w = gesture_words(&now, &[(7, None), (8, None)], &mut sent);
        assert_eq!(w, vec![(7, "pax gest 12582913 hail 10.0 0.0 2.0".to_string())]);
    }

    /// (31 October 2026) A client tells the host what its people do for a bus with the height
    /// too; the host's words without it are read as before.
    #[test]
    fn a_gesture_word_is_read_with_or_without_its_height() {
        let w = |t: &'static str| gesture_word(t.split_whitespace());
        assert_eq!(w("12 hail 10.5 -3 4.5"), Some((12, Some((Gesture::Hail, DVec3::new(10.5, -3.0, 4.5))), true)));
        assert_eq!(w("12 wave 10.5 -3"), Some((12, Some((Gesture::Wave, DVec3::new(10.5, -3.0, 0.0))), false)));
        assert_eq!(w("12 off 0 0"), Some((12, None, false)));
        assert_eq!(w("x hail 1 2"), None);
        assert_eq!(w("12"), None);
    }

    /// (31 October 2026) A client's own count of a stop its bus left people at and the host's of
    /// its own people there are told the plugins as one, a moment later.
    #[test]
    fn a_client_tells_a_stop_left_once_with_the_hosts_people() {
        let mut s = PaxSignals::default();
        s.hold((2, 7, false));
        assert!(s.due(1.0).is_empty());
        s.hold((1, 7, true));
        s.hold((3, 9, false));
        assert!(s.due(1.0).is_empty());
        assert_eq!(s.due(1.1), vec![(3, 7, true)]);
        assert_eq!(s.due(2.0), vec![(3, 9, false)]);
        assert!(s.due(5.0).is_empty());
    }

    #[test]
    fn the_hosts_words_are_read_and_bad_ones_let_go() {
        assert_eq!(client_word("left 3 1234 0", None), Some((3, 1234, false)));
        assert_eq!(client_word("left 2 -7 1", None), Some((2, -7, true)));
        assert_eq!(client_word("left 0 1 0", None), None);
        assert_eq!(client_word("left x 1 0", None), None);
        assert_eq!(client_word("left 9999 1 0", None), Some((500, 1, false)));
        assert_eq!(client_word("gest 12 hail 10.5 -3", None), None);
        assert_eq!(client_word("what", None), None);
        assert_eq!(client_word("", None), None);
    }
}

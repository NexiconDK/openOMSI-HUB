//! (NexiconDK, 30 October 2026) The passengers' signals to the players' buses, between games:
//! a player's bus that left people waiting at their stop (`people::passed_by`) is told to
//! that player's game - ours to our plugins (`passengers_left`, which the trip counts as left
//! behind for the company), another LAN player's to his game, which tells its plugins - and on
//! a LAN host what its waiting people do for a bus (signal to it, glance at it full, wave after
//! it) is told to the clients, which draw the host's people (`PeopleSim::set_mirror_gesture`).
//!
//! The words, from the host to a player: `pax left <count> <stop> <full 0|1>` and
//! `pax gest <person> <hail|full|wave|off> <x> <y>`. A game without them lets them go.

use hashbrown::HashMap;
use omsi_sim::people::{remote_bus_player, BusId, Gesture};

/// How often the host looks at what its people do for a bus (s).
const EVERY: f32 = 0.25;

/// What the host told the clients its people do (`host_tick`).
#[derive(Default)]
pub(crate) struct PaxSignals {
    acc: f32,
    sent: HashMap<u32, Gesture>,
}

/// One time our bus left people at a stop: how many, the stop (its map object), and whether
/// it was full.
pub(crate) type LeftHere = (u32, i64, bool);

impl PaxSignals {
    /// Once a frame, where the people are simulated (alone, the LAN host, a dedicated
    /// server): the times our bus left people (returned, for the plugins), another LAN
    /// player's told to his game; and on a host, what its waiting people do for a bus, to the
    /// clients.
    pub(crate) fn host_tick(&mut self, dt: f32, lan: Option<&mut omsi_net::LanSession>, h: &mut crate::humans::Humans) -> Vec<LeftHere> {
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
        let now: HashMap<u32, (Gesture, glam::DVec3)> = h.lan_gestures().into_iter().map(|(id, g, at)| (id, (g, at))).collect();
        let peers: Vec<u32> = l.peers().map(|p| p.pose.id).filter(|id| *id != l.my_id).collect();
        let mut say = Vec::new();
        for (id, (g, at)) in &now {
            if self.sent.get(id) != Some(g) {
                say.push(format!("pax gest {id} {} {:.1} {:.1}", g.word(), at.x, at.y));
            }
        }
        for id in self.sent.keys().filter(|id| !now.contains_key(*id)) {
            say.push(format!("pax gest {id} off 0 0"));
        }
        self.sent = now.into_iter().map(|(id, (g, _))| (id, g)).collect();
        if !peers.is_empty() {
            for text in &say {
                for p in &peers {
                    l.command(*p, text);
                }
            }
        }
        mine
    }
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
            let id = w.next()?.parse::<u32>().ok()?;
            let word = w.next()?;
            let x = w.next().and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(0.0);
            let y = w.next().and_then(|v| v.parse::<f64>().ok()).filter(|v| v.is_finite()).unwrap_or(0.0);
            if let Some(h) = h {
                let at = glam::DVec3::new(x, y, h.people.iter().find(|p| p.id == id).map_or(0.0, |p| p.position.z));
                h.set_mirror_gesture(id, Gesture::of_word(word).map(|g| (g, at)));
            }
            None
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

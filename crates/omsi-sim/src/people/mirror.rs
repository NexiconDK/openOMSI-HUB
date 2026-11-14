//! LAN play (see `lan_world`): a host keeps people around every player, tells the clients
//! where they are and hands the waiting ones over to a client's bus; a client draws the
//! host's people instead of its own and simulates only those who board its bus.

use super::*;

/// Where one of the host's people is this frame, as a client draws them.
#[derive(Debug, Clone, Copy)]
pub struct MirrorPose {
    pub pos: DVec3,
    pub heading: f64,
    pub vel: DVec2,
    pub activity: Activity,
    /// Aboard a timetable bus: (its id, the point of its frame, heading in its frame, the
    /// seat or standing place, if known).
    pub aboard: Option<(u64, Vec3, f64, Option<usize>)>,
    /// Waiting at a stop: (the stop object, the waiting place).
    pub waiting: Option<(i64, usize)>,
}

/// The bus id (`BusId::Ai`) another LAN player's bus has among the buses here: far above
/// the traffic's car ids.
pub fn remote_bus_id(player: u32) -> u64 {
    (1 << 40) | player as u64
}

/// The bus id (`BusId::Ai`) of a vehicle the player placed (`Player::uid`).
pub fn placed_bus_id(uid: u64) -> u64 {
    (2 << 40) | uid
}

/// The player whose bus `remote_bus_id` gave this id (None for a traffic bus).
pub fn remote_bus_player(bus: u64) -> Option<u32> {
    (bus >> 40 == 1).then_some((bus & 0xFFFF_FFFF) as u32)
}

/// The waiting place (`LanPerson::waiting`) of somebody who has stepped up to another player's
/// bus rolling in: at the stop, at none of its places (a client puts them nowhere else).
pub const STEPPED_UP: usize = 255;

/// One of the host's people as it tells the clients.
pub struct LanPerson {
    pub id: u32,
    pub ty: Arc<HumanType>,
    pub pos: DVec3,
    pub heading: f64,
    pub speed: f64,
    pub activity: Activity,
    pub aboard: Option<(u64, Vec3, f64, Option<usize>)>,
    pub waiting: Option<(i64, usize)>,
    /// (NexiconDK, ticket inspection) The `.cti` item it wears (empty: its figure's own
    /// clothes), and whether it is one of a bus company's ticket inspectors - the other games
    /// draw it so (`dress_of`).
    pub paint: String,
    pub inspector: bool,
}

impl LanPerson {
    /// The bus company an inspector works for: its tag, as its clothes are named
    /// (`~Inspector_<look>_<company>`); empty for anybody else.
    pub fn company(&self) -> String {
        super::inspect::company_of(&self.paint).unwrap_or_default()
    }
}

/// The clothes person `p` wears (its `.cti` item's name; empty: the figure's own) and whether it
/// is a ticket inspector.
fn clothes_of(p: &Person) -> (String, bool) {
    let paint = p.variant.checked_sub(1).and_then(|v| p.ty.variants.get(v)).map(|v| v.name.trim().to_string()).unwrap_or_default();
    let inspector = super::inspect::is_inspector_variant(&paint) || matches!(&p.state, State::Pax(x) if x.task == Task::Inspecting);
    (paint, inspector)
}

impl PeopleSim {
    /// Is person `id` (one drawn for another game) here?
    pub fn has_mirror(&self, id: u32) -> bool {
        self.people.iter().any(|p| p.id == id && p.remote)
    }

    /// The riders of our own bus, for the other LAN players to see: (id, type, place in the
    /// bus frame, heading there, seat, activity).
    pub fn lan_riders(&self) -> Vec<LanPerson> {
        self.people
            .iter()
            .filter(|p| p.puppet.is_none() && !p.remote)
            .filter_map(|p| match p.place {
                Place::Bus(BusId::Player, l) => Some(LanPerson {
                    paint: clothes_of(p).0,
                    inspector: clothes_of(p).1,
                    id: p.id,
                    ty: p.ty.clone(),
                    pos: p.position,
                    heading: p.heading,
                    speed: 0.0,
                    activity: p.activity,
                    aboard: Some((
                        0,
                        l,
                        p.lheading,
                        match &p.state {
                            State::Pax(x) if x.task == Task::SittingInBus => x.seat,
                            _ => None,
                        },
                    )),
                    waiting: None,
                }),
                _ => None,
            })
            .collect()
    }

    /// The places people are kept around: ours (the player's bus, else the camera) - not on
    /// a dedicated server, where nobody plays there (`players_only`) - and every other LAN
    /// player's.
    pub fn anchors(&self) -> impl Iterator<Item = DVec3> + '_ {
        (!self.players_only)
            .then_some(self.center)
            .into_iter()
            .chain(self.lan_centers.iter().copied())
    }

    /// Is `p` further than `r` from us and from every other LAN player?
    pub fn far_from_players(&self, p: DVec3, r: f64) -> bool {
        self.anchors().all(|c| (p - c).length() > r)
    }

    /// The stops and pavements around the other players of a LAN session (host).
    pub fn populate_lan_centers(
        &mut self,
        world: &dyn World,
        net: &Network,
    ) {
        if self.lan_centers.is_empty() {
            return;
        }
        let mine = self.center;
        for c in self.lan_centers.clone() {
            // (near us they are there already - unless nobody plays here)
            if !self.players_only && (c - mine).length() < 150.0 {
                continue;
            }
            self.populate_with(world, Some(net), c);
            self.populate_on_foot(world, net, 1.0);
        }
        self.center = mine;
    }

    /// Everybody within `radius` of `near` the clients may see (host): on foot, waiting at
    /// a stop, or aboard a timetable bus - not the riders of our own bus, which the others
    /// see from outside only.
    pub fn lan_people(&self, near: DVec3, radius: f64) -> Vec<LanPerson> {
        let r2 = radius * radius;
        self.people
            .iter()
            .filter(|p| p.puppet.is_none() && !p.remote)
            .filter(|p| (p.position - near).length_squared() < r2)
            .filter_map(|p| {
                let aboard = match p.place {
                    Place::Bus(BusId::Player, _) => return None,
                    Place::Bus(BusId::Ai(bus), l) => Some((
                        bus,
                        l,
                        p.lheading,
                        match &p.state {
                            State::Pax(x) if x.task == Task::SittingInBus => x.seat,
                            _ => None,
                        },
                    )),
                    Place::Ground => None,
                };
                let waiting = match &p.state {
                    State::Pax(x) if aboard.is_none() && x.task == Task::WaitingForBus => x.stop.zip(x.spot),
                    // (NexiconDK, 31 October 2026) stepped up to another player's bus rolling in:
                    // at the stop, at no waiting place - his game takes them once it stands
                    State::Pax(x) if aboard.is_none() && x.task == Task::ToBus && x.bus.is_some_and(|b| matches!(b, BusId::Ai(k) if remote_bus_player(k).is_some())) => x.stop.map(|s| (s, STEPPED_UP)),
                    _ => None,
                };
                let (paint, inspector) = clothes_of(p);
                Some(LanPerson {
                    paint,
                    inspector,
                    id: p.id,
                    ty: p.ty.clone(),
                    pos: p.position,
                    heading: p.heading,
                    speed: if aboard.is_some() { 0.0 } else { p.vel.length() },
                    activity: p.activity,
                    aboard,
                    waiting,
                })
            })
            .collect()
    }

    /// A client's bus takes these waiting people (host): those still waiting leave our
    /// world (they are the client's now); returns them. Somebody who has meanwhile walked
    /// up to another bus stays ours.
    ///
    /// (NexiconDK, 31 October 2026) Only those who take that bus - by what it shows, as at
    /// boarding (`takes_waiting`): the client asks for everybody waiting at the stop its bus is
    /// at, and those for another line went with it too. They stayed in the client's game after
    /// its bus had gone (with a destination of its own drawing), drawn for the others only as
    /// long as it was near, while we filled the stop up again: people vanished from the stop
    /// and came twice. A bus we do not know yet (nor what it shows) takes all, as before.
    pub fn hand_over(&mut self, player: u32, ids: &[u32]) -> Vec<u32> {
        let mut out = Vec::new();
        let buses = self.last_buses.clone();
        let bus = buses.iter().find(|b| b.id == BusId::Ai(remote_bus_id(player)));
        let shown = self.remote_termini.get(&player).is_some_and(|t| !t.trim().is_empty());
        for id in ids {
            let Some(i) = self.people.iter().position(|p| p.id == *id) else {
                continue;
            };
            // (waiting, or stepped up to that bus as it rolled in)
            let ours = remote_bus_id(player);
            if !matches!(&self.people[i].state, State::Pax(x) if x.task == Task::WaitingForBus || (x.task == Task::ToBus && x.bus == Some(BusId::Ai(ours)))) || self.people[i].remote {
                continue;
            }
            if let (Some(b), true, Some(x)) = (bus, shown, self.pax(i)) {
                let takes = x.stop.and_then(|stop| self.stops.get(&stop).map(|s| (stop, s))).is_some_and(|(stop, s)| self.takes_waiting(stop, s, b, x.dest.as_deref(), x.line));
                if !takes {
                    continue;
                }
            }
            // (still counted at their stop while that bus stands there, as the people
            // boarding a bus of ours are: see `handed`)
            if let Some(stop) = self.pax(i).and_then(|x| x.stop) {
                self.handed.push((stop, remote_bus_id(player)));
            }
            self.release(i);
            let p = self.people.swap_remove(i);
            self.retire(&p);
            out.push(*id);
        }
        out
    }

    /// Draw the host's people from now on (`on`), or simulate our own again. Everybody who
    /// is not getting on, riding or getting off our bus goes (the host's come instead; the
    /// host's copies cannot walk on by themselves).
    pub fn set_mirror(&mut self, on: bool) {
        if self.mirror == on {
            return;
        }
        self.mirror = on;
        let keep = |p: &Person| !p.remote && p.state.bus() == Some(BusId::Player);
        let mut i = 0;
        while i < self.people.len() {
            if keep(&self.people[i]) || self.people[i].puppet.is_some() {
                i += 1;
                continue;
            }
            self.release(i);
            let p = self.people.swap_remove(i);
            self.retire(&p);
        }
        // our own people from now on are numbered far above the host's (those riding with
        // us already too)
        if on {
            self.next_id = self.next_id.max(1 << 30);
            for p in self.people.iter_mut().filter(|p| p.puppet.is_none()) {
                self.bodies.ops.push(BodyOp::Renamed { from: p.id, to: self.next_id });
                p.id = self.next_id;
                self.next_id += 1;
            }
        }
        for s in self.stops.values_mut() {
            for t in s.taken.iter_mut() {
                *t = false;
            }
        }
        self.claims_out.clear();
        self.claimed.clear();
        self.mirror_wait.clear();
        self.mirror_seat.clear();
        self.mirror_aisle.clear();
    }

    /// The human type of a file relative to a content root (`Humans/…/x.hum`).
    pub fn type_by_file(&self, file: &str) -> Option<usize> {
        let want = file.replace('\\', "/").to_ascii_lowercase();
        self.types.iter().position(|t| {
            t.def
                .path
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase()
                .ends_with(&want)
        })
    }

    /// (NexiconDK, ticket inspection, 11 November 2026) The figure and the clothes to draw a
    /// person another game describes (`file` its figure, `paint` the `.cti` item it wears,
    /// `inspector` one of a bus company's ticket inspectors): that figure in clothes of that name
    /// (1 up; None: as any passenger). An inspector whose figure - or whose clothes on it - this
    /// game has not is drawn in another figure that has those clothes, else the same look of
    /// another company's (`~Inspector_<look>_…`); without any, in its own figure as a passenger.
    /// Nobody else ever wears an inspector's clothes here. None: neither the figure nor an
    /// inspector's to draw it in. Only the clothes this game read with its figures count: nothing
    /// is made or read here (a dedicated server's world draws nothing and makes none).
    pub fn dress_of(&self, file: &str, paint: &str, inspector: bool) -> Option<(usize, Option<usize>)> {
        let wears = |t: &HumanType, name: &str| t.variants.iter().position(|v| v.name.trim().eq_ignore_ascii_case(name.trim())).map(|v| v + 1);
        let own = if file.trim().is_empty() { None } else { self.type_by_file(file) };
        let paint = paint.trim();
        let uniform = inspector || super::inspect::is_inspector_variant(paint);
        if !paint.is_empty() && (uniform || !super::inspect::is_inspector_variant(paint)) {
            if let Some((t, v)) = own.and_then(|t| wears(&self.types[t], paint).map(|v| (t, v))) {
                return Some((t, Some(v)));
            }
        }
        if !uniform {
            return own.map(|t| (t, None));
        }
        // (an inspector: the same uniform on another figure, the same look of another company)
        if !paint.is_empty() {
            if let Some((t, v)) = self.types.iter().enumerate().filter(|(_, t)| t.def.grown_up()).find_map(|(k, t)| wears(t, paint).map(|v| (k, v))) {
                return Some((t, Some(v)));
            }
        }
        if let Some(look) = super::inspect::look_of(paint) {
            let prefix = super::inspect::variant_name(look, "");
            let same_look = |t: &HumanType| t.variants.iter().position(|v| v.name.trim().len() > prefix.len() && v.name.trim()[..prefix.len()].eq_ignore_ascii_case(&prefix)).map(|v| v + 1);
            let mut order: Vec<usize> = own.into_iter().collect();
            order.extend((0..self.types.len()).filter(|k| Some(*k) != own && self.types[*k].def.grown_up()));
            if let Some((t, v)) = order.into_iter().find_map(|k| same_look(&self.types[k]).map(|v| (k, v))) {
                return Some((t, Some(v)));
            }
        }
        own.map(|t| (t, None))
    }

    /// The file of a human type relative to its content root (`Humans/…/x.hum`).
    pub fn type_file(ty: &HumanType) -> String {
        let p = ty.def.path.to_string_lossy().replace('\\', "/");
        match p.to_ascii_lowercase().rfind("/humans/") {
            Some(k) => p[k + 1..].to_string(),
            None => p,
        }
    }

    /// One of the host's people appears here (client).
    pub fn mirror_add(
        &mut self,
        world: &dyn World,
        id: u32,
        ty: usize,
        pose: &MirrorPose,
    ) -> bool {
        self.mirror_add_dressed(world, id, ty, None, pose)
    }

    /// (NexiconDK, ticket inspection) `mirror_add` in clothes `variant` (the figure's `.cti`
    /// item, 1 up: `dress_of`; None: as any passenger, never an inspector's uniform).
    pub fn mirror_add_dressed(
        &mut self,
        world: &dyn World,
        id: u32,
        ty: usize,
        variant: Option<usize>,
        pose: &MirrorPose,
    ) -> bool {
        if self.people.iter().any(|p| p.id == id) {
            return false;
        }
        let state = State::Idle;
        self.force_variant = variant;
        let spawned = self.spawn_as(world, pose.pos, pose.heading, state, Some(ty));
        self.force_variant = None;
        let Some(i) = spawned else {
            return false;
        };
        self.next_id -= 1;
        let p = &mut self.people[i];
        self.bodies.renamed(p.id, id);
        p.id = id;
        p.anim = OmsiAnim::default();
        p.remote = true;
        self.mirror_set(id, pose);
        true
    }

    /// Where one of the host's people is this frame (client).
    pub fn mirror_set(&mut self, id: u32, pose: &MirrorPose) {
        let Some(p) = self.people.iter_mut().find(|p| p.id == id && p.remote) else {
            return;
        };
        p.position = pose.pos;
        p.heading = pose.heading;
        p.vel = pose.vel;
        p.activity = pose.activity;
        match pose.waiting {
            Some(w) => {
                self.mirror_wait.insert(id, w);
            }
            None => {
                self.mirror_wait.remove(&id);
            }
        }
        match pose.aboard {
            Some((bus, local, lheading, seat)) => {
                p.place = Place::Bus(BusId::Ai(bus), local);
                p.lheading = lheading;
                // (their pace up the aisle, from where they were: walking, not sliding)
                let now = self.time;
                let pace = match self.mirror_aisle.get(&id) {
                    Some(&(b, was, since, pace)) if b == BusId::Ai(bus) => {
                        let d = (local - was).truncate().length() as f64;
                        if d > 0.002 {
                            let v = (d / (now - since).max(0.016)).min(3.0);
                            self.mirror_aisle.insert(id, (b, local, now, pace + (v - pace) * 0.5));
                            pace + (v - pace) * 0.5
                        } else if now - since > 0.3 {
                            self.mirror_aisle.insert(id, (b, was, since, 0.0));
                            0.0
                        } else {
                            pace
                        }
                    }
                    _ => {
                        self.mirror_aisle.insert(id, (BusId::Ai(bus), local, now, 0.0));
                        0.0
                    }
                };
                let h = pose.heading.to_radians();
                p.vel = DVec2::new(h.sin(), h.cos()) * if seat.is_some() { 0.0 } else { pace };
                // (NexiconDK, 31 October 2026: in their place - on a seat they sit, as the host
                // has them; they stood beside every seat of a timetable bus, `mirror_facts`)
                match seat {
                    Some(k) => {
                        let seated = self.last_buses.iter().find(|b| b.id == BusId::Ai(bus)).and_then(|b| b.cabin.seats.get(k)).is_some_and(|s| s.seated);
                        if seated {
                            p.activity = Activity::Sit;
                        }
                        self.mirror_seat.insert(id, (BusId::Ai(bus), k));
                    }
                    None => {
                        self.mirror_seat.remove(&id);
                    }
                }
            }
            None => {
                p.place = Place::Ground;
                self.mirror_seat.remove(&id);
                self.mirror_aisle.remove(&id);
            }
        }
    }

    /// (NexiconDK, 31 October 2026) What the pose reads of one of the host's people (client): in
    /// their place in a timetable bus, seated on a seat (its height, as the host's own riders
    /// sit: `Person::pose_facts`), holding on at a standing place - else as they are.
    pub fn mirror_facts(&self, p: &Person) -> PoseFacts {
        let base = p.pose_facts();
        let Some(&(bus, k)) = self.mirror_seat.get(&p.id) else { return base };
        if !matches!(p.place, Place::Bus(b, _) if b == bus) {
            return base;
        }
        let Some(seat) = self.last_buses.iter().find(|b| b.id == bus).and_then(|b| b.cabin.seats.get(k)) else { return base };
        if !seat.seated {
            return PoseFacts { activity: Activity::Stand, hold: super::pose_facts::grip_for_standing_rider(true, true, false, false), ..base };
        }
        let (at, lift) = super::pose_facts::seated_pose(p.ty.def.seat_height, seat.height, p.ty.rig.seat_front(), p.ty.rig.seat_lift);
        PoseFacts { activity: Activity::Sit, seat: Some(at), reach: None, floor_lift: lift, hold: 0.0, seated_level: Some((p.ty.def.seat_height - seat.height) as f64) }
    }

    /// One of the host's people has gone (client).
    pub fn mirror_remove(&mut self, id: u32) {
        if let Some(i) = self.people.iter().position(|p| p.id == id && p.remote) {
            let p = self.people.swap_remove(i);
            self.retire(&p);
        }
        self.claimed.remove(&id);
        self.mirror_wait.remove(&id);
        self.mirror_seat.remove(&id);
        self.mirror_aisle.remove(&id);
        self.mirror_gestures.remove(&id);
    }

    /// A remote person this frame: they stand where the host put them.
    pub fn mirror_want(&mut self, i: usize, _buses: &[BusNow]) -> Want {
        Want::stand(None, self.people[i].activity)
    }

    /// The type of one of our people (host).
    /// (NexiconDK, ticket inspection) Person `id`'s figure, the clothes it wears and whether it
    /// is a ticket inspector (`LanPerson::paint`, `inspector`), for its description.
    pub fn lan_look_by_id(&self, id: u32) -> Option<(Arc<HumanType>, String, bool)> {
        self.people.iter().find(|p| p.id == id && !p.remote).map(|p| {
            let (paint, inspector) = clothes_of(p);
            (p.ty.clone(), paint, inspector)
        })
    }

    pub fn lan_people_by_id(&self, id: u32) -> Option<Arc<HumanType>> {
        self.people
            .iter()
            .find(|p| p.id == id && !p.remote)
            .map(|p| p.ty.clone())
    }

    /// Where the host's people are drawn (client; `OMSI_LAN_TRACE`).
    pub fn mirror_positions(&self) -> Vec<(u32, DVec3)> {
        self.people
            .iter()
            .filter(|p| p.remote && p.place == Place::Ground)
            .map(|p| (p.id, p.position))
            .collect()
    }

    /// Waiting people to ask the host for (client).
    pub fn take_claims(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.claims_out)
    }

    /// The host handed this waiting person over to our bus (client). (The people of a LAN
    /// game stay the host's: nothing is taken over.)
    pub fn grant(&mut self, id: u32) -> bool {
        self.claimed.remove(&id);
        let Some((stop, spot)) = self.mirror_wait.remove(&id) else { return false };
        let Some(i) = self.people.iter().position(|p| p.id == id && p.remote) else { return false };
        if !self.stops.contains_key(&stop) {
            return false;
        }
        // ours from now on: waiting at that place, for the bus that stands there (the first
        // listed, as Omsi.exe takes it without a line record)
        let sp = self.stops[&stop].spots.get(spot).cloned();
        let seatheight = self.people[i].ty.def.seat_height;
        let walk = 1.1 + (self.rand_f() as f32 * 2.0 - 1.0) * 0.2;
        let mut pax = Pax::new(walk, self.rand_f());
        pax.task = Task::WaitingForBus;
        pax.stop = Some(stop);
        // what a waiting person of ours has (sub_626044 and task 6): a destination drawn
        // from the stop (both sides load the same map) and a distance to ride without one;
        // with neither they would get off again at once (#813). (NexiconDK, 31 October 2026:
        // one our bus takes - the host hands over only those who take it, `hand_over`; drawn
        // at random, some of them did not get on, and stayed in our game at the stop)
        let buses = self.last_buses.clone();
        let ours = buses.iter().find(|b| b.id == BusId::Player);
        let mut drawn = self.draw_dest(stop);
        if let Some(b) = ours {
            for _ in 0..16 {
                let s = &self.stops[&stop];
                if self.takes_waiting(stop, s, b, drawn.0.as_deref(), drawn.1) {
                    break;
                }
                drawn = self.draw_dest(stop);
            }
        }
        let (dest, line) = drawn;
        pax.dest = dest;
        pax.line = line;
        pax.ride_km = self.rand_f() as f32 * 19.0 + 1.0;
        pax.pos = self.people[i].position;
        pax.yaw = self.people[i].heading.to_radians();
        if let Some(sp) = sp {
            pax.spot = Some(spot);
            if let Some(t) = self.stops.get_mut(&stop).unwrap().taken.get_mut(spot) {
                *t = true;
            }
            if sp.height != 0.0 {
                pax.seat_h = sp.height;
                pax.pos = sp.pos - DVec3::Z * seatheight as f64;
                pax.pax_state = 2.0;
            }
            pax.yaw = sp.face.to_radians();
        }
        let p = &mut self.people[i];
        p.remote = false;
        p.state = State::Pax(Box::new(pax));
        true
    }

    /// The host's people waiting at the stop our bus stands at (client): ask for them.
    ///
    /// (NexiconDK, 31 October 2026) Once it stands in the stop's box - not as soon as it is
    /// listed there, 60 m before it: a bus driving past took them all with it, and they stayed
    /// in our game at the stop (drawn for the others only while we were near, the host filling
    /// the stop up again). Till then they are the host's, who signal or wave to it as it comes,
    /// and wave after it if it drives on (`passed_by`, `pax left`).
    pub fn claim_waiting(&mut self) {
        if !self.mirror {
            return;
        }
        let now = self.time;
        self.claimed.retain(|_, t| now - *t < 10.0);
        let standing = self.last_buses.iter().any(|b| b.id == BusId::Player && b.speed.abs() < 1.5);
        if !standing {
            return;
        }
        let at: Vec<i64> = self
            .stops
            .iter()
            .filter(|(_, s)| s.buses.iter().any(|b| b.0 == BusId::Player && b.1))
            .map(|(id, _)| *id)
            .collect();
        if at.is_empty() {
            return;
        }
        for (id, (stop, _)) in &self.mirror_wait {
            if at.contains(stop) && !self.claimed.contains_key(id) {
                self.claimed.insert(*id, now);
                self.claims_out.push(*id);
            }
        }
    }
}

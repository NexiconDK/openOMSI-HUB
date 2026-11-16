//! Discord, Steam, the Lua plugins and the personnel file's counts in the window's frame.

use super::*;

impl App {
    /// Discord's status, Steam's callbacks, the plugins' frame (and what they asked the game
    /// to do), `OMSI_WATCH_VARS`, and the people's counts for the personnel file.
    pub(super) fn frame_integrations(&mut self, event_loop: &ActiveEventLoop, dt: f32) {
        // Discord's status: the map, the bus, the line (every few seconds)
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        {
            self.integrations.discord_t -= dt;
            if self.integrations.discord_t <= 0.0 {
                self.integrations.discord_t = 5.0;
                if self.args.server.is_none()
                    && self.integrations.discord.is_none()
                    && self.settings.discord_status
                {
                    self.integrations.discord =
                        crate::discord::Discord::start(&self.settings.discord_app_id);
                }
                // (openOMSI #3798: each part as the settings choose - the map, the bus, the line,
                // the next stop and the delay, the players online and the server's room)
                let show = self.settings.discord_show;
                let max = if show.players && self.integrations.discord.is_some() { self.discord_server_max() } else { 0 };
                if let Some(d) = self.integrations.discord.as_ref() {
                    let bus = self.player.as_ref().map(|p| {
                        let definition = &p.vehicle.ty.def;
                        let short = omsi_launcher_lib::vehicle_type_label(&definition.type_name, &definition.path);
                        let full = omsi_launcher_lib::display_bus_name(&format!("{} {short}", definition.manufacturer));
                        (short, full)
                    });
                    let duty = self.session.duty.as_ref();
                    let next_stop = duty.zip(self.player.as_ref()).and_then(|(d, p)| {
                        let stop = d.trips.get(d.trip_index)?.stops.get(d.next_stop)?;
                        Some((stop.name.as_str(), p.vehicle.host.tt_delay))
                    });
                    let game = crate::discord::Game {
                        map: self.world.as_ref().map(|w| w.global.name.as_str()),
                        bus: bus.as_ref().map(|(short, full)| (short.as_str(), full.as_str())),
                        duty: duty.map(|d| (d.line.as_str(), d.tour.as_str())),
                        next_stop,
                        // (the players: the others the session knows, and this one)
                        players: self.net.lan.as_ref().map(|lan| (lan.session, lan.peer_count() + 1, max)),
                    };
                    d.set(crate::discord::Presence::for_game(&game, show));
                }
            }
        }

        // Steam's callbacks (rich presence)
        #[cfg(steam)]
        if let Some(steam) = self.integrations.steam.as_ref() {
            steam.client.run_callbacks();
        }
        // (NexiconDK, 1 November 2026) the cameras and the speed displays the bus passed: their
        // flash, and the plugins' `speed_camera` this frame
        if !self.paused {
            self.roadside_step();
        }
        // the plugins' frame, with the bus's scripts done
        let mut plugins = self.integrations.plugins.take().unwrap_or_else(crate::plugins::load);
        if !plugins.is_empty() {
            self.plugin_watch(&mut plugins);
        }
        // the mouse on the plugins' script textures since their frame, and which those are now
        plugins.pointer = std::mem::take(&mut self.integrations.plugin_pointer.pending);
        self.integrations.plugin_pointer.pages = plugins.pointer_textures();
        if !plugins.is_empty() && !self.paused {
            let keys = std::mem::take(&mut self.integrations.plugin_keys);
            let events = std::mem::take(&mut self.integrations.plugin_events);
            let events_ex = std::mem::take(&mut self.integrations.plugin_events_ex);
            // (the plugins see the whole game; they are out of it for their frame)
            let mut io = crate::plugins::Io::new(self, dt, keys, events, events_ex);
            plugins.frame(&mut io);
            let commands = std::mem::take(&mut io.commands);
            drop(io);
            crate::plugin_io::world::follow_sounds(self);
            // what the plugins asked the game to do: lines of the game menu
            self.integrations.plugin_command = true;
            for c in commands {
                if let Some(k) = self.game_menu_items().iter().position(|m| m.0 == c) {
                    let was = self.menus.game_menu;
                    self.menus.menu_prev_pause = self.paused;
                    self.menu_choose(event_loop, k);
                    // (an action leaves the menu as it found it)
                    if self.menus.chooser.is_none() && was.is_none() {
                        self.menus.game_menu = None;
                    }
                } else {
                    // (a line of the vehicle or world pages)
                    self.menus.menu_prev_pause = self.paused;
                    self.page_action(&c);
                }
            }
            self.integrations.plugin_command = false;
        } else {
            self.integrations.plugin_keys.clear();
            // (while the game is paused they wait for the next frame)
            if plugins.is_empty() {
                self.integrations.plugin_events.clear();
                self.integrations.plugin_events_ex.clear();
            }
        }
        self.integrations.plugins = Some(plugins);
        // OMSI_WATCH_VARS=a,b: every change of those variables of the player's bus
        if let (Some(p), Some(list)) = (self.player.as_ref(), omsi_cfg::flags::OMSI_WATCH_VARS.var()) {
            thread_local!(static LAST: std::cell::RefCell<std::collections::HashMap<String, f32>> = Default::default());
            LAST.with(|last| {
                let mut last = last.borrow_mut();
                for n in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
                    let v = p.vehicle.var(n).unwrap_or(f32::NAN);
                    if last.get(n).is_none_or(|&o| o.to_bits() != v.to_bits()) {
                        log::info!("watch: {n} = {v} at {:.2} s", self.clock.time);
                        last.insert(n.to_string(), v);
                    }
                }
            });
        }
        // (Omsi-Hub, 16 November 2026) the bus's triggers run from outside: `bus_trigger` - a
        // tablet's buttons, which a plugin answers (Bus Company Online's messages)
        if let Some(p) = self.player.as_mut() {
            for name in p.vehicle.take_pressed() {
                crate::plugins::queue_event(&mut self.integrations.plugin_events, "bus_trigger", vec![omsi_plugin::InfoValue::Text(name)]);
            }
        }
        if let (Some(h), Some(p)) = (self.session.humans.as_mut(), self.player.as_ref()) {
            let hurt = steps::people_in_career(&mut self.session.career, h, p, self.settings.collision_pedestrians);
            for (name, price) in h.take_sales() {
                let args = vec![omsi_plugin::InfoValue::Text(name.trim().to_string()), crate::plugins::num_f32(price)];
                crate::plugins::queue_event(&mut self.integrations.plugin_events, "ticket_sold", args);
            }
            // (NexiconDK, round U: the ticket inspectors got off - what they did)
            for o in h.take_inspections() {
                let num = |x: u32| omsi_plugin::InfoValue::Num(x as f64);
                let args = vec![num(o.checked), num(o.caught), num(o.fled), num(o.stops), omsi_plugin::InfoValue::Text(o.board_stop.clone()), omsi_plugin::InfoValue::Text(o.leave_stop.clone()), num(o.team)];
                crate::plugins::queue_event(&mut self.integrations.plugin_events, "inspection_done", args);
            }
            if hurt > 0 {
                self.service_msg = Some(("Pedestrian knocked down!".into(), 6.0));
                crate::plugins::queue_event(&mut self.integrations.plugin_events, "pedestrian", vec![omsi_plugin::InfoValue::Num(hurt as f64)]);
            }
        }
        // Omsi-Hub: the phone and tablet companion (`companion=1`): sign-on, duty, the bus's
        // screens
        crate::companion::each_frame(self);
    }

    /// (openOMSI #3798) The room of the server played on, for Discord's party (0: not known yet):
    /// the host's own limit, or a client's server asked once (`ServerLimit`).
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    fn discord_server_max(&mut self) -> usize {
        let Some(lan) = self.net.lan.as_ref() else { return 0 };
        if lan.role == omsi_net::Role::Host {
            return 64;
        }
        let Some(target) = self.args.lan_join.clone() else { return 0 };
        self.integrations.discord_max.get(&target, |t| omsi_net::ws::query(t, false).ok().map(|i| i.max_players))
    }
}

//! Bus Company Online (NexiconDK, phase 2 of docs/omsihub/BUS_COMPANY_ONLINE.md): one company's
//! own process on the main server - `omsi-hub-game --company-host <folder> --company <id>`,
//! started and kept running by the main server (`omsi_launcher_lib::company::main_server`).
//!
//! It is the company's boss, without a window: the launcher's session with the company open
//! (`company::Session`, its data folder the server's: `core::set_data_dir`), shared with "Play
//! together" on the company's own port with a code that stays the same across restarts
//! (`online::kept_session`, written to `<id>.code` for the main server). The players join it from
//! "Bus Company Online" in their launchers (phase 2, step 2: the main server's list hands them the
//! code and a ticket of its own for each - `online::ticket`; without it nobody comes in), and, as
//! their roles allow, use the company's pages there - its lines, fleet, staff, planning and
//! duties; the founder is its owner with every permission (`online::seat_owner`).
//!
//! The company's time is run by its host (`online::Host`, on the server's clock: its days, its
//! plan for the map's world, the world's reports), not by the launcher's clock (which waits for a
//! boss to answer): every `STEP` what the pages changed is handed to the host (its plan for the
//! world written again), the host steps, and what it did is handed back to the pages.
//!
//! Its console (the main server writes to it): `quit`, `role <player>; <role>`, `owner <player's
//! id>; <name>` (the main server's player made its owner).

use super::company::{self, Session};
use super::together;
use omsi_launcher_lib as core;
use omsi_launcher_lib::company::online::{self, Host};
use omsi_launcher_lib::company::together::Role;
use omsi_launcher_lib::company::Company;
use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// How often the company's time steps (real time).
pub const STEP: Duration = Duration::from_secs(2);

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// A line for the main server's log of this company.
fn say(id: &str, text: &str) {
    let at = core::local_now().map(|(y, mo, d, h, mi)| format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02} ")).unwrap_or_default();
    println!("{at}company {id}: {text}");
}

/// A role by the console's name for it.
pub fn role_of(s: &str) -> Option<Role> {
    match s.trim().to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
        "driver" | "chauffør" | "chauffor" => Some(Role::Driver),
        "planner" | "planlægger" => Some(Role::Planner),
        "manager" | "leder" | "depotleder" => Some(Role::Manager),
        "co_owner" | "coowner" | "owner" | "ejer" | "medejer" => Some(Role::CoOwner),
        _ => None,
    }
}

/// The console's `role <player>; <role>` on the company: the player of that name (as his launcher
/// says it) gets the role with its permissions. What to say.
pub fn set_role(c: &mut Company, rest: &str) -> String {
    let mut parts = rest.split(';').map(str::trim);
    let (who, role) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    let Some(role) = role_of(role) else { return format!("'{role}' is no role (driver, planner, manager, owner)") };
    let Some(no) = c.together.guests.iter().find(|g| g.name.trim().eq_ignore_ascii_case(who)).map(|g| g.no) else {
        return format!("no player '{who}' has come to the company yet");
    };
    c.together.set_role(no, role, &role.preset());
    format!("{who} is {} now", role.key())
}

/// The console's `owner <player's id>; <name>`: the main server's player the company's owner
/// (every permission once he comes, `online::seat_owner`); the one before keeps his place among
/// the players, as a driver. What to say.
pub fn set_owner(c: &mut Company, rest: &str) -> String {
    let mut parts = rest.split(';').map(str::trim);
    let (pid, name) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if pid.is_empty() || name.is_empty() {
        return "owner <player's id>; <name>".into();
    }
    let before = c.online.owner_id.clone();
    if let Some(no) = c.together.guests.iter().find(|g| !before.is_empty() && g.id == before && g.id != pid).map(|g| g.no) {
        c.together.set_role(no, Role::Driver, &Role::Driver.preset());
    }
    c.online.owner = name.to_string();
    c.online.owner_id = pid.to_string();
    online::seat_owner(c);
    format!("{name} is its owner now")
}

/// What the pages changed handed to the host (its plan for the world written again), the host a
/// step on, and what it did handed back to the pages; the owner seated. Whether the plan for the
/// world was written.
fn step(s: &mut Session, host: &mut Host, data: &Path) -> bool {
    if s.closing {
        return false;
    }
    let Some(c) = s.company.as_mut() else { return false };
    if c.id != host.company.id {
        return false;
    }
    let mut changed = online::seat_owner(c);
    if *c != host.company {
        host.company = c.clone();
        host.replan();
    }
    let mut planned = false;
    match host.tick(unix_now()) {
        Ok(st) => {
            for d in &st.closed {
                say(&host.company.id, &format!("the day {} closed - {} of {} tours run, {} km, a result of {:.2}", d.date, d.covered, d.tours, d.km.round(), d.result as f64 / 100.0));
            }
            planned = st.planned;
        }
        Err(e) => say(&host.company.id, &format!("{e:#}")),
    }
    if s.company.as_ref() != Some(&host.company) {
        s.company = Some(host.company.clone());
        changed = true;
    }
    if changed {
        company::changed(s);
    }
    if planned {
        let _ = online::bump_plans(data);
    }
    planned
}

/// The company `id` of the main server's folder `data`, run until `quit` on the console (or its
/// end). An error: it would not open.
pub fn run(data: &Path, id: &str) -> Result<(), String> {
    core::set_data_dir(data);
    let clock = online::read_server_clock(data).ok_or_else(|| format!("no server's clock in {} - the main server keeps it", data.display()))?;
    let root = std::path::PathBuf::from(core::load_config().root);
    let c = core::company::store::load(data, id).map_err(|e| format!("{e:#}"))?;
    let map_dir = core::company::main_server::map_dir_of(&root, &c.map);
    let (mut host, away) = Host::open(data, id, unix_now(), Some(&map_dir), Some(clock)).map_err(|e| format!("{e:#}"))?;
    if let Some(a) = away {
        say(id, &format!("caught up the time the server was off - {} days closed", a.reports.len()));
    }
    let kept = online::kept_session(data, id, omsi_launcher_lib::company::together::server::DEFAULT_PORT).map_err(|e| format!("{e:#}"))?;
    let key = kept.host_key().ok_or("the company's session key is spoilt")?;
    // (phase 2, step 2: nobody in without the main server's ticket for him)
    let tickets = online::read_server_secret(data).ok_or_else(|| format!("no server's secret in {} - the main server keeps it", data.display()))?;
    let code = company::with(|s| {
        company::open_company(s, host.company.clone());
        together::start_with(s, kept.port, kept.secret_bytes(), key, Some(tickets))
    })?;
    let _ = std::fs::write(online::code_file(data, id), &code);
    say(id, &format!("{} ({}) open on port {}, code {code}", host.company.name, host.company.map, kept.port));
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let _ = std::thread::Builder::new().name("console".into()).spawn(move || {
        let stdin = std::io::stdin();
        let mut line = String::new();
        while stdin.read_line(&mut line).map(|n| n > 0).unwrap_or(false) {
            if tx.send(std::mem::take(&mut line)).is_err() {
                return;
            }
        }
        // (the main server is gone: so is this)
        let _ = tx.send("quit".into());
    });
    let mut next = Instant::now();
    loop {
        let mut quit = false;
        while let Ok(line) = rx.try_recv() {
            let line = line.trim();
            let (cmd, rest) = line.split_once(char::is_whitespace).map(|(a, b)| (a, b.trim())).unwrap_or((line, ""));
            match cmd {
                "quit" | "exit" => quit = true,
                "role" => {
                    let said = company::with(|s| match s.company.as_mut() {
                        Some(c) => {
                            let said = set_role(c, rest);
                            company::changed(s);
                            said
                        }
                        None => "no company open".into(),
                    });
                    say(id, &said);
                }
                "owner" => {
                    let said = company::with(|s| match s.company.as_mut() {
                        Some(c) => {
                            let said = set_owner(c, rest);
                            company::changed(s);
                            said
                        }
                        None => "no company open".into(),
                    });
                    say(id, &said);
                }
                "" => {}
                other => say(id, &format!("'{other}' is no command (quit, role <player>; <role>, owner <id>; <name>)")),
            }
        }
        if quit {
            break;
        }
        if Instant::now() >= next {
            next = Instant::now() + STEP;
            company::with(|s| step(s, &mut host, data));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // (the players told, what their games drive left to the AI; the company saved)
    company::with(|s| {
        together::stop_now(s);
        if let Some(c) = s.company.as_ref().filter(|c| c.id == host.company.id) {
            host.company = c.clone();
        }
    });
    host.save(unix_now()).map_err(|e| format!("{e:#}"))?;
    say(id, "saved, stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_by_their_console_names() {
        assert_eq!(role_of("Owner"), Some(Role::CoOwner));
        assert_eq!(role_of("co-owner"), Some(Role::CoOwner));
        assert_eq!(role_of("planlægger"), Some(Role::Planner));
        assert_eq!(role_of(" driver "), Some(Role::Driver));
        assert_eq!(role_of("boss"), None);
        let mut c = omsi_launcher_lib::company::found(&omsi_launcher_lib::company::Founding { name: "Rolle".into(), date: "2024-03-04".into(), ..Default::default() }, "owner");
        assert!(set_role(&mut c, "Jonas; planner").contains("no player"));
        let no = c.together.guest_of("pc-2", "Jonas").unwrap();
        assert!(set_role(&mut c, "jonas; boss").contains("no role"));
        assert_eq!(set_role(&mut c, "jonas; planner"), "jonas is planner now");
        assert_eq!(c.together.guest(no).unwrap().role, Role::Planner);
        // (the server's player made its owner: seated when he is among its players)
        assert!(set_owner(&mut c, "p1").contains("owner <"));
        assert_eq!(set_owner(&mut c, "pc-2; Jonas"), "Jonas is its owner now");
        assert_eq!((c.online.owner_id.as_str(), c.together.guest(no).unwrap().role), ("pc-2", Role::CoOwner));
        let other = c.together.guest_of("p9", "Eve").unwrap();
        set_owner(&mut c, "p9; Eve");
        assert_eq!((c.together.guest(other).unwrap().role, c.together.guest(no).unwrap().role), (Role::CoOwner, Role::Driver));
    }
}

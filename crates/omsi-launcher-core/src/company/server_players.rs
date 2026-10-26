//! Bus Company Online (NexiconDK, phase 2 step 2 of docs/omsihub/BUS_COMPANY_ONLINE.md): the
//! players the main server knows, in `<folder>/players.json`.
//!
//! A player logs in from "Bus Company Online" in his launcher (`server_api`) with his Omsi-Hub
//! account - the access token of the launcher's session, which the main server has Omsi-Hub
//! online (Supabase) check and does not keep -, with the Steam ID his computer's Steam was last
//! signed in with and the key the server gave that computer the first time, or with both: then
//! the Steam ID is joined to the account's player (once; a computer more of his gets a key of its
//! own when he logs in there with the account). A Steam ID alone is weaker than an account: the
//! first computer to come with it is its owner (it gets the key), and another computer with it
//! is turned away until the account vouches for it.
//!
//! Each player has the server's id (`Player::id`: his id in every company of the server, which
//! knows him by the server's ticket - `online::ticket`), a name unique on the server, and money of
//! his own (the server's `start_money` to begin with; driving pays into it from phase 3), which
//! founds companies (`found_price`).

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// The players' file in the main server's folder.
pub const PLAYERS_FILE: &str = "players.json";
/// At most this many computers keep a key for a player's Steam ID.
pub const KEYS: usize = 5;
/// The longest name on the server.
pub const NAME_MAX: usize = 32;

/// A player of the server.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Player {
    /// The server's id for him (`p` and 16 hex digits): his id in every company of the server.
    pub id: String,
    /// His name on the server (unique, whatever the case).
    pub name: String,
    /// His Omsi-Hub account (its user id), empty: none.
    pub account: String,
    /// His Steam ID (SteamID64), empty: none.
    pub steam: String,
    /// The SHA-256 (hex) of the key each of his computers was given for the Steam ID.
    pub keys: Vec<String>,
    /// His own money (cents).
    pub money: i64,
    /// When he first came, and last logged in (Unix seconds).
    pub since: i64,
    pub seen: i64,
    /// (Phase 6: what founding a company asks, `server_found`) The trips he drove for a company
    /// of the server as its world measured them; the server's day his leadership course ends on
    /// (empty: not booked); his driving tests on the server (the one booked, the classes passed,
    /// the last tests taken).
    pub trips: u32,
    pub course_until: String,
    pub tests: super::career::DriverCareer,
}

/// The ways to log in the server takes (`logins` in company-server.cfg).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Logins {
    pub account: bool,
    pub steam: bool,
}

impl Default for Logins {
    fn default() -> Self {
        Logins { account: true, steam: true }
    }
}

impl Logins {
    /// `account steam`, `account`, `steam` (empty: both).
    pub fn parse(s: &str) -> Logins {
        let words: Vec<String> = s.split([' ', ',', ';']).map(|w| w.trim().to_ascii_lowercase()).filter(|w| !w.is_empty()).collect();
        if words.is_empty() {
            return Logins::default();
        }
        Logins { account: words.iter().any(|w| w == "account" || w == "omsihub" || w == "email"), steam: words.iter().any(|w| w == "steam") }
    }
}

/// What a launcher sends to log in.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Login {
    /// The name he would like on the server (his account's display name, else Steam's).
    pub name: String,
    /// The Omsi-Hub account's access token (checked by the server, not kept).
    pub account: String,
    /// The Steam ID his Steam was last signed in with, and the key the server gave this computer.
    pub steam: String,
    pub key: String,
}

/// An Omsi-Hub account as Omsi-Hub online vouched for it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub user_id: String,
    /// Its display name (empty: none yet).
    pub name: String,
}

/// What a login came to.
#[derive(Clone, Debug)]
pub struct LoggedIn {
    pub player: Player,
    /// He is new on the server.
    pub new: bool,
    /// A new key for this computer's Steam ID (kept by his launcher; the server keeps its hash).
    pub key: Option<String>,
    /// Something to tell him (English: a key of the translations).
    pub note: Option<&'static str>,
}

/// Why a login was turned away, or what it says (English: keys of the translations).
pub const NO_WAY: &str = "Sign in with your Omsi-Hub account in the launcher, or start Steam, to log in to the server.";
pub const STEAM_ELSEWHERE: &str = "This Steam account was used on the server from another computer. Sign in with your Omsi-Hub account here once to add this computer.";
pub const STEAM_TAKEN: &str = "Your Steam account belongs to another player on this server: it was not added to yours.";
pub const STEAM_OTHER: &str = "Your player on this server has another Steam account: this one was not added.";
pub const ACCOUNT_REFUSED: &str = "The server did not accept your Omsi-Hub account: sign in again in the launcher.";
pub const ACCOUNT_OFFLINE: &str = "The server could not reach Omsi-Hub online to check your account: try again in a moment.";
pub const ACCOUNT_OFF: &str = "This server takes no Omsi-Hub accounts: start Steam to log in.";
pub const STEAM_OFF: &str = "This server takes no Steam logins: sign in with your Omsi-Hub account in the launcher.";

/// Every text of the players' (for the translations' test).
pub fn texts() -> Vec<&'static str> {
    vec![NO_WAY, STEAM_ELSEWHERE, STEAM_TAKEN, STEAM_OTHER, ACCOUNT_REFUSED, ACCOUNT_OFFLINE, ACCOUNT_OFF, STEAM_OFF]
}

/// Whether `s` is a Steam ID (SteamID64: 17 digits).
pub fn is_steam_id(s: &str) -> bool {
    s.len() == 17 && s.bytes().all(|b| b.is_ascii_digit()) && s.starts_with("765")
}

fn key_hash(key: &str) -> String {
    omsi_net::hostkey::hex(&omsi_net::http::sha256(key.trim().as_bytes()))
}

/// The players of a main server.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Players {
    pub players: Vec<Player>,
    /// (Phase 3) How far each company's pay file was paid out (bytes of `online::pay_file`).
    pub paid: std::collections::BTreeMap<String, u64>,
}

impl Players {
    /// The players of the folder `data` (none when it has no file yet).
    pub fn load(data: &Path) -> Result<Players> {
        let path = data.join(PLAYERS_FILE);
        match std::fs::read(&path) {
            Ok(b) => serde_json::from_slice(&b).with_context(|| format!("reading {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Players::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// Saved into the folder `data` (written beside, then put in place).
    pub fn save(&self, data: &Path) -> Result<()> {
        let path = data.join(PLAYERS_FILE);
        let tmp = data.join(format!("{PLAYERS_FILE}.tmp"));
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?).with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, &path).with_context(|| format!("cannot write {}", path.display()))
    }

    pub fn get(&self, id: &str) -> Option<&Player> {
        self.players.iter().find(|p| p.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Player> {
        self.players.iter_mut().find(|p| p.id == id)
    }

    /// The player of a name (whatever its case), or of an id.
    pub fn find(&self, who: &str) -> Option<&Player> {
        let who = who.trim();
        self.players.iter().find(|p| p.id == who || p.name.eq_ignore_ascii_case(who))
    }

    /// `name` as a name on the server for the player `me`: trimmed and cut, and with a number
    /// after it when another player has it.
    pub fn free_name(&self, name: &str, me: &str) -> String {
        let base: String = name.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(NAME_MAX).collect();
        let base = if base.is_empty() { "Player".to_string() } else { base };
        let taken = |n: &str| self.players.iter().any(|p| p.id != me && p.name.eq_ignore_ascii_case(n));
        if !taken(&base) {
            return base;
        }
        (2..).map(|k| format!("{base} {k}")).find(|n| !taken(n)).unwrap_or(base)
    }

    fn new_player(&mut self, name: &str, start_money: i64, now: i64) -> usize {
        let b: [u8; 8] = omsi_net::http::random_bytes();
        let id = format!("p{}", omsi_net::hostkey::hex(&b));
        let name = self.free_name(name, &id);
        self.players.push(Player { id, name, money: start_money, since: now, ..Default::default() });
        self.players.len() - 1
    }

    /// A new key for player `k`'s Steam ID on this computer (its hash kept, the oldest gone past
    /// `KEYS`).
    fn new_key(&mut self, k: usize) -> String {
        let b: [u8; 16] = omsi_net::http::random_bytes();
        let key = omsi_net::hostkey::hex(&b);
        let keys = &mut self.players[k].keys;
        keys.push(key_hash(&key));
        if keys.len() > KEYS {
            let extra = keys.len() - KEYS;
            keys.drain(..extra);
        }
        key
    }

    /// A login (`l`; `account`: what Omsi-Hub online said of its token - None: none sent, Err:
    /// the server's word against it), at `now`: the player, made the first time. An error: why
    /// he was turned away.
    pub fn login(&mut self, l: &Login, account: Option<Result<Account, &'static str>>, logins: Logins, start_money: i64, now: i64) -> Result<LoggedIn, &'static str> {
        let mut note = None;
        let account = match account {
            Some(_) if !logins.account => {
                note = Some(ACCOUNT_OFF);
                None
            }
            Some(Ok(a)) if !a.user_id.is_empty() => Some(a),
            Some(Err(e)) => {
                note = Some(e);
                None
            }
            _ => None,
        };
        let steam = l.steam.trim();
        let steam = if is_steam_id(steam) && logins.steam {
            Some(steam)
        } else {
            if is_steam_id(steam) && account.is_none() {
                note = Some(STEAM_OFF);
            }
            None
        };
        let key = l.key.trim();
        let by_steam = steam.and_then(|s| self.players.iter().position(|p| p.steam == s));
        let key_ok = |p: &Player| !key.is_empty() && p.keys.contains(&key_hash(key));
        let mut new = false;
        let mut given = None;
        let k = match (account, steam) {
            (Some(a), _) => {
                let k = match self.players.iter().position(|p| p.account == a.user_id) {
                    Some(k) => k,
                    None => {
                        new = true;
                        let k = self.new_player(if a.name.trim().is_empty() { &l.name } else { &a.name }, start_money, now);
                        self.players[k].account = a.user_id.clone();
                        k
                    }
                };
                if let Some(s) = steam {
                    match by_steam {
                        // (his Steam ID, joined to his account now)
                        None if self.players[k].steam.is_empty() => {
                            self.players[k].steam = s.to_string();
                            given = Some(self.new_key(k));
                        }
                        None => note = Some(STEAM_OTHER),
                        // (a computer of his without the key: the account vouches for it)
                        Some(j) if j == k => {
                            if !key_ok(&self.players[k]) {
                                given = Some(self.new_key(k));
                            }
                        }
                        Some(_) => note = Some(STEAM_TAKEN),
                    }
                }
                // (his account's own name, as he chose it there)
                if !a.name.trim().is_empty() {
                    let name = self.free_name(&a.name, &self.players[k].id);
                    self.players[k].name = name;
                }
                k
            }
            (None, Some(s)) => match by_steam {
                Some(k) if key_ok(&self.players[k]) => k,
                Some(_) => return Err(STEAM_ELSEWHERE),
                None => {
                    new = true;
                    let k = self.new_player(&l.name, start_money, now);
                    self.players[k].steam = s.to_string();
                    given = Some(self.new_key(k));
                    k
                }
            },
            (None, None) => return Err(note.unwrap_or(NO_WAY)),
        };
        self.players[k].seen = now;
        Ok(LoggedIn { player: self.players[k].clone(), new, key: given, note })
    }

    /// `amount` (cents) paid out of player `id`'s money. An error: he has not got it (his money
    /// as it is).
    pub fn pay(&mut self, id: &str, amount: i64) -> Result<(), i64> {
        let Some(p) = self.get_mut(id) else { return Err(0) };
        if p.money < amount {
            return Err(p.money);
        }
        p.money -= amount;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEAM: &str = "76561198000000001";
    const START: i64 = 2_000_000;

    fn acc(id: &str, name: &str) -> Option<Result<Account, &'static str>> {
        Some(Ok(Account { user_id: id.into(), name: name.into() }))
    }

    fn login(name: &str, steam: &str, key: &str) -> Login {
        Login { name: name.into(), steam: steam.into(), key: key.into(), ..Default::default() }
    }

    #[test]
    fn a_steam_id_alone_the_first_computer_keeps_it() {
        let mut ps = Players::default();
        assert_eq!(ps.login(&login("Jonas", "", ""), None, Logins::default(), START, 10).unwrap_err(), NO_WAY);
        assert_eq!(ps.login(&login("Jonas", "1234", ""), None, Logins::default(), START, 10).unwrap_err(), NO_WAY, "no Steam ID");
        let first = ps.login(&login("Jonas", STEAM, ""), None, Logins::default(), START, 10).unwrap();
        assert!(first.new);
        assert_eq!(first.player.money, START);
        assert!(first.player.id.starts_with('p') && first.player.id.len() == 17);
        let key = first.key.clone().expect("a key for this computer");
        assert!(!ps.players[0].keys[0].contains(&key), "only its hash kept");
        // (the same computer, with its key)
        let again = ps.login(&login("Jonas", STEAM, &key), None, Logins::default(), START, 20).unwrap();
        assert_eq!((again.player.id.as_str(), again.new, again.key.is_none()), (first.player.id.as_str(), false, true));
        assert_eq!(ps.players[0].seen, 20);
        // (another computer with the Steam ID, without the key)
        assert_eq!(ps.login(&login("Jonas", STEAM, ""), None, Logins::default(), START, 30).unwrap_err(), STEAM_ELSEWHERE);
        assert_eq!(ps.login(&login("Jonas", STEAM, "beef"), None, Logins::default(), START, 30).unwrap_err(), STEAM_ELSEWHERE);
        assert_eq!(ps.players.len(), 1);
    }

    #[test]
    fn an_account_and_steam_are_one_player() {
        let mut ps = Players::default();
        let a = ps.login(&login("pc name", STEAM, ""), acc("u-1", "Mathias"), Logins::default(), START, 10).unwrap();
        assert_eq!((a.player.name.as_str(), a.player.account.as_str(), a.player.steam.as_str()), ("Mathias", "u-1", STEAM));
        let key = a.key.expect("the Steam ID joined, with a key");
        // (Steam alone on that computer: the same player)
        let b = ps.login(&login("Mathias", STEAM, &key), None, Logins::default(), START, 20).unwrap();
        assert_eq!(b.player.id, a.player.id);
        // (a second computer: the account vouches for it, a key of its own)
        let c = ps.login(&login("Mathias", STEAM, ""), acc("u-1", "Mathias"), Logins::default(), START, 30).unwrap();
        assert_eq!(c.player.id, a.player.id);
        let key2 = c.key.expect("a key for the second computer");
        assert!(ps.login(&login("Mathias", STEAM, &key2), None, Logins::default(), START, 40).is_ok());
        assert!(ps.login(&login("Mathias", STEAM, &key), None, Logins::default(), START, 40).is_ok(), "the first computer's key still good");
        // (the account alone)
        assert_eq!(ps.login(&Login::default(), acc("u-1", ""), Logins::default(), START, 50).unwrap().player.id, a.player.id);
        assert_eq!(ps.players.len(), 1);
        // (another account with his Steam ID: not taken from him)
        let d = ps.login(&login("Eve", STEAM, ""), acc("u-2", "Eve"), Logins::default(), START, 60).unwrap();
        assert_ne!(d.player.id, a.player.id);
        assert_eq!((d.note, d.key.is_none(), d.player.steam.as_str()), (Some(STEAM_TAKEN), true, ""));
        // (an account refused, the Steam key good: in by Steam, told)
        let e = ps.login(&login("Mathias", STEAM, &key), Some(Err(ACCOUNT_REFUSED)), Logins::default(), START, 70).unwrap();
        assert_eq!((e.player.id.as_str(), e.note), (a.player.id.as_str(), Some(ACCOUNT_REFUSED)));
        assert_eq!(ps.login(&Login::default(), Some(Err(ACCOUNT_OFFLINE)), Logins::default(), START, 70).unwrap_err(), ACCOUNT_OFFLINE);
    }

    #[test]
    fn names_are_the_servers_and_the_ways_in_its_choice() {
        let mut ps = Players::default();
        let a = ps.login(&login("  Jonas   B ", STEAM, ""), None, Logins::default(), START, 10).unwrap();
        assert_eq!(a.player.name, "Jonas B");
        let b = ps.login(&Login::default(), acc("u-9", "jonas b"), Logins::default(), START, 10).unwrap();
        assert_eq!(b.player.name, "jonas b 2", "a name is the server's once");
        assert_eq!(ps.free_name("", "x"), "Player");
        assert_eq!(ps.free_name(&"x".repeat(50), "x").len(), NAME_MAX);
        assert_eq!(ps.find("JONAS B").map(|p| p.id.as_str()), Some(a.player.id.as_str()));
        // (only Steam on this server; only accounts on that one)
        let steam_only = Logins::parse("steam");
        assert_eq!(steam_only, Logins { account: false, steam: true });
        assert_eq!(ps.login(&Login::default(), acc("u-9", "x"), steam_only, START, 10).unwrap_err(), ACCOUNT_OFF);
        let account_only = Logins::parse("account");
        assert_eq!(ps.login(&login("x", "76561198000000002", ""), None, account_only, START, 10).unwrap_err(), STEAM_OFF);
        assert_eq!(Logins::parse(""), Logins::default());
        assert_eq!(Logins::parse("account, steam"), Logins::default());
    }

    #[test]
    fn money_is_paid_and_kept() {
        let dir = std::env::temp_dir().join(format!("openomsi-bco-players-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Players::load(&dir).unwrap(), Players::default());
        let mut ps = Players::default();
        let id = ps.login(&login("Jonas", STEAM, ""), None, Logins::default(), START, 10).unwrap().player.id;
        assert_eq!(ps.pay(&id, START + 1), Err(START));
        assert_eq!(ps.pay(&id, 500_000), Ok(()));
        assert_eq!(ps.get(&id).unwrap().money, START - 500_000);
        assert_eq!(ps.pay("nobody", 1), Err(0));
        ps.save(&dir).unwrap();
        assert_eq!(Players::load(&dir).unwrap(), ps);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

# Bus Company Online

Bus Company as it is today, but online. One big server - NexiconDK's own machine, always on -
runs two kinds of program. The **main server** holds every company of every map: their depots,
money, buses, staff, rosters, tenders, and the players; it runs their days on its own clock, 1:1
with the real one, whether anybody is logged in or not, and loads no map. A **world per map** -
the dedicated server as it is, without a window or graphics - runs a map for everybody who drives
there, as a LAN session: the traffic, the passengers and every company's buses in their paint, on
the main server's clock; the main server starts a map's world when the first player wants to
drive there and closes it a while after the last has gone. The plan in Danish, with pictures,
for the team: "Bus Company Multiplayer - plan" (udkast 4, 24 October 2026; udkast 2 had one
company on a dedicated server - a misunderstanding; udkast 3 let each player's computer run its
own world - then the AI buses and the traffic are not the same for everybody).

## Decisions

| Topic | Decision |
|---|---|
| The server | One big server (our own machine), always on: the main server with all the data, and a world per map in use. No other servers in this mode |
| The world | One per map, run by the server (the dedicated server: traffic, passengers, every company's AI buses), started when the first player wants to drive on the map, closed a while after the last has gone; no world: the AI trips worked out at midnight |
| Seeing each other | Everybody on a map is in the same world, as in LAN: the same buses, cars and passengers |
| Time | Real time 1:1, around the clock; the day closes at midnight; the time the server was off is caught up |
| Companies | Founded by players on the server, with enough of their own pay (and courses - which, decided later) |
| Maps | A company is founded on one map and never changes it; each of its depots belongs to its map for good |
| Local depots | A company that does well opens local depots on other maps, the same way it was founded; each has a manager (depot leader), its own cash and books, buses, staff, timetables and rosters |
| The head company | Opens and closes depots, appoints the managers, moves money between depots, approves big purchases (new buses), sets the company's name, colours, logo and share; sees all depots together |
| Between depots | Buses can be moved from depot to depot, staff lent - within one company only |
| Lines | On each map a line belongs to at most one company, won by tender; the map's lines no company has run as the map has them |
| The roster | The easy mode's routine fills it; in Normal and Realistic the planners alone, what they leave open does not run |
| Trips | A map's world reports every trip once, the AI buses' and the players' (measured by the server, not by the player's game); a map without a world: the main server works its AI trips out at midnight, as Bus Company does for the days not played |
| Control room | Per depot: every bus live while the map's world runs (interventions on the AI buses), else where the timetable has them |
| Identity | The player's Omsi-Hub account (email or Google, checked by the server with Omsi-Hub online), the Steam ID read on his computer (`Steam/config/loginusers.vdf`) with a secret key the server gives that computer, or both as one player (decided 24 October 2026: "Begge dele") |
| Pay | A trip counts in the company's books (the depot's) and in the player's own career; the company takes a share of the trip's income it chooses (say 25 %), a change counting from the next day |
| Project | A server program of its own built from Omsi-Hub's code, in modules of its own, behind a switch (off until ready); no new fork, no long-lived branch |

## Phases

1. **The big server**: the main server - every company and depot on every map, on the server's
   clock; days closed at midnight, saved, caught up after a pause; and a world per map - the
   dedicated server with every company's buses in their paint on the server's clock, started and
   closed by the main server, its trips reported to it; no world: the AI trips worked out at
   midnight. Measured: what a world costs in memory and CPU, how long it takes to start.
2. **Log in and the company's pages**: "Bus Company Online" in the launcher, the Omsi-Hub account and/or the Steam ID and key; the
   company's pages as today, with the server's data; found a company on a map from one's own pay;
   roles and permissions; other players hired.
3. **Drive**: Drive joins the duty's map's world on the server with the duty's bus (the world
   started first if it is not running); the AI bus taken off the tour; the trips measured by the
   world (the depot's share, the driver's pay); everybody on the map in one world.
4. **Depots**: a local depot on another map; depot leaders; money and buses moved, staff lent;
   the head office's view and approvals.
5. **Competition and control room**: tenders between the players' companies on each map; a
   control room per depot.
6. **Polish**: courses required to found a company; maps kept running if the machine can carry
   them; translations and guides.

## What is built (phase 1)

**The main server**: `omsi-hub-cli --company-server <folder>` (`company::main_server`), no window,
no map, no graphics. `<folder>/company-server.cfg` (written with the defaults the first time): the
OMSI folder (`root`), the server's clock (`real_time`, or `date` and `time` the first time), the
worlds' game program, ports, how many at once and their idle minutes, and what they run.

- The server's clock (`online::ServerClock`, `server_clock`, `<folder>/clock.json`): set the first
  time, then 1:1 with the real clock, on where it was after a restart (the time it was off
  included); on the real time it takes the machine's date and time at each start, forward only.
- Every company of `<folder>/companies` on every installed map runs on it (`online::Host`, its
  course pinned to the clock; its map's folder from `root`, a mod's copy first): a step every
  `STEP` (2 s), the folder looked through every `SCAN` (30 s) for companies founded since, every
  company saved at `quit`. One main server per folder (`online::lock_folder`, `server.lock`).
- Whenever a plan is written, a company comes on or the clock is set, the worlds are told
  (`online::bump_plans`, `plans.gen`).
- The console: `help`, `status`, `maps` (the installed maps by their folders), `start <map>`, `stop <map>`, `found <name>; <map>; [mode]`
  (`online::found_on_server`: new, on the server's date and minute - nothing of a player's own
  companies comes along), `quit`. Its log: `<folder>/company-server.log`. Until the launcher uses
  the server (phase 2) a new company has no lines, buses or staff.

**A map's world**: `start <map>` writes `<folder>/worlds/<map>/server.cfg` (`main_server::world_cfg`)
and starts `omsi-hub-game --server` with it on the next free ports (UDP `first_port + 2k`, web
`+1`); its output goes to `world.out` beside it. With `companies = 1` the dedicated server is a
world (`omsi-app` `company_server`): it runs on the main server's clock
(`online::read_server_clock`), takes every company with a plan of its map
(`company_live::ServerCompanies`: each line to one company, its tours with its buses in its paint,
its lines' other tours kept off, the map's other lines as ever), takes the plans and the clock in
again when `plans.gen` changes (`ScheduleSim::reload_company`), and reports every company trip
into the folder once (`store::append_live`), which the main server's host tells its company.
`idle_stop` (minutes): the world stops by itself that long after its last player; the main server
lets it go (`reap_worlds`).

Tested: `company::online::tests`, `company::main_server::tests` (companies on two maps, founded from
the console, restarted), `company_live::tests`, `company_server::tests`,
`server::idle_tests`, `server_cfg::tests`. Not run yet on a real server with a real map.

Still to do in phase 1: measured on a real map (a world's memory and CPU, how long it takes to
start); what the launcher's pages do besides (a line's change on its day, the dealer's
deliveries, the control room's messages, the depot's building) moved to the hosts.

## Phase 2, step 1: each company in its own process, its pages from the players' launchers

The players reach a company with "Play together" ("Join a company"), as they reach a friend's
company today: the server is every company's boss.

- **A company's process** (`omsi-hub-game --company-host <folder> --company <id> --root <OMSI>`,
  `omsi_launcher_api::omsihub::company_host`): no window; its data folder is the server's
  (`core::set_data_dir`; the main server writes `<folder>/launcher.json` with the OMSI folder,
  the game and the profile `owner` the server's companies are founded under). It opens the company
  in the launcher's session (`company::open_company`) and shares it on the company's own port
  with a code that stays the same across restarts (`online::kept_session`: `<id>.session` with
  the port, the secret and the session key; `together::start_with`), written to `<id>.code`.
  The players' launchers use the company's pages as their roles allow (`together::relay`).
- **Its time** is run by `online::Host` on the server's clock, not by the launcher's clock (which
  waits for a boss to answer its questions and its day's report): every `STEP` (2 s) what the
  pages changed goes to the host (its plan for the world written again: `Host::replan`), the host
  steps (days, plan, the world's reports, saving) and what it did goes back to the pages.
- **The owner**: `found <name>; <map>; [mode]; [owner]` names him by his launcher's name; the
  first player of that name to come is the owner with every permission (`Role::CoOwner`), from
  then on only he (by the id his launcher keeps: `online::seat_owner`, `Online::owner_id`).
  Other players come as drivers; `role <company>; <player>; <role>` on the main server's console
  gives a role (driver, planner, manager, owner). Step 2 (below) knows every player by the server's
  id for him instead.
- **The main server** supervises: a process for every company whose map is installed, its output
  in `<folder>/hosts/<id>.out`; one that ended is started again after `HOLD` (30 s); a company
  founded is started at once; `quit` tells each to quit (each saves its company) and ends what
  does not within 20 s. A process whose main server is gone (its console closed) quits by itself.
  `status` shows each company's state, port and code. `tunnel = 1` gives the companies (and the
  worlds) a Cloudflare tunnel each, for players across the internet; else their LAN addresses.

## Phase 2, step 2: "Bus Company Online" in the launcher, the players logged in

Nobody needs a company's code or the console any more: the bus company's start has a third tile,
**Bus Company Online** (`omsihub::bco`, `bco.js`; behind the kill switch `bco::KILLED` and
`OMSI_BCO_OFF`, marked "Test").

- **The server's door** (`company::server_api`): plain HTTP on `api_port` (27040), `POST /bco`, and a
  Cloudflare tunnel in front of it with `tunnel = 1`. The players reach it with the **server's
  code** (`HUB-…`, the company codes' format: its secret, the session of its kept key -
  `<folder>/api.session` -, its port and the machine's addresses; the company tag `server_tag()`
  tells it from a company's), which `status` shows. Questions: `info`, `login`, `companies`,
  `join`, `found`, `me`, each with the code's secret; a login lasts a day, longer while used.
- **The players** (`company::server_players`, `<folder>/players.json`): each has the server's id
  (`p…`, his id in every company there), a name unique on the server, and money of his own
  (`start_money` to begin with, in whole euros; driving pays into it from phase 3). He logs in with
  his **Omsi-Hub account** - the launcher hands the server the access token of its session
  (`online::account_for_server`, only with the privacy statement agreed to), which the server has
  Omsi-Hub online check (`omsihub_online::verify_access`: `GET /auth/v1/user` and the profile's
  display name) and does not keep -, with the **Steam ID** of the computer
  (`omsi_cfg::install_search::steam_user`: Steam's `config/loginusers.vdf`, the user `MostRecent`)
  and the **key** the server gave that computer the first time (its SHA-256 kept, up to five
  computers; the launcher keeps the key in `~/.openomsi/bco.json` by the server's session), or
  with **both**: then the Steam ID is joined to the account's player. A Steam ID alone: the first
  computer to come with it keeps it; another one is turned away until the account vouches for it.
  `logins = account steam` says which ways the server takes.
- **Joining**: the server's list shows every company, with the player's role in each; Join asks
  the server for the company's code and its **ticket** for him (`online::ticket`: HMAC-SHA256 of
  the company, his id and name under `<folder>/server.secret`), and his launcher joins as "Play
  together" does (`together::join_server_company`: its menu, its pages) with the server's id and
  name for him. The company's process lets nobody in without a ticket
  (`HostState::tickets`, `host::NO_TICKET`), so a launcher cannot claim another's name or id.
- **Founding**: a name, a map of the server's and a game mode; `found_price` is paid from the
  player's money (`MainServer::found_for`), and he is the owner from the start
  (`Online::owner_id` = his id, set before the company's process starts).
- **The owner is the boss** (25 October 2026, after the first test at home: the founder was
  shown as "Co-owner" and could not change the company's country): the server's process hosts
  the company, but the player who founded it is its boss to everybody - named so on the pages
  (`host::server_owner`, the snapshot's boss and `card.boss`; the process's own profile is not
  shown), shown as **Owner** (`SnapYou::owner`, `SnapMember::owner`), out of every other
  player's reach (`may_manage`: not even a co-owner changes his role or sends him away), and
  doing from afar what the boss alone does elsewhere where it makes sense there
  (`relay::OWNER_ON_SERVER`: the country). The server's clock is nobody's to change: the real
  time switched off is refused with a word why (`relay::SERVER_CLOCK_ACTS`, `Need::Clock`).
- **The console** besides: `players` (each with how he logs in, his money and his companies),
  `money <player>; <amount>` (set, or `+`/`-` to add or take), `owner <company>; <player>` (a
  player of the server a company's owner - one founded from the console without one, say: the
  company's process takes `owner <id>; <name>`, `company_host::set_owner`). A company of the
  server is no longer joined with its code alone ("Join a company"): the ticket is needed.

Tested: `server_players::tests` (a Steam ID alone, an account and Steam as one player, a second
computer, names, the ways in, money), `server_api::tests` (the code's secret and version, a login
refused and taken, founding paid and refused, the list, joining a company still starting; the door
over the loopback), `together::tests::a_server_company_takes_the_servers_ticket_only`,
`verify::tests` (the account check against a stand-in of Supabase),
`install_search::tests::the_steam_user_last_signed_in`, `bco::tests` (every text in six
languages); end to end with the real programs and a stand-in OMSI folder (25 October 2026): an
older `company-server.cfg` given the door's settings, the server's code shown, a false account
refused (Omsi-Hub online not reached from the test machine: said so), two players logged in with
Steam IDs and keys, another computer without the key refused, two companies founded and paid,
the list with the founder's role, join with code and ticket; at the company's process no ticket,
a forged one and a ticket with another id refused, the founder seated as owner with every
permission, another player a driver refused the money page; `money`, `players` and `owner` on
the console, the new owner kept at `quit`. The page itself looked at in a browser (the server's
code; the companies; founding).

Still open for phase 2: other players hired by a company (beyond joining it), roles given from
the company's own pages for the server's players (the pages do it as in "Play together"), a
stable address for the tunnel (a quick tunnel's address changes at every start; the code finds
it through the game's relay).

### Measured on a real map (Ahlheim: 30 lines, 418 tours a day)

`OMSI_COMPANY_MAP_DIR=<map folder> cargo test -p omsi-launcher-core --lib online::tests::on_a_real_map -- --ignored --nocapture`,
a company of 3 lines (74 tours a day), 18 buses, 150 staff:

| Step | Time |
|---|---|
| The map's timetable of a day read | 0.65 s (5.8 s the first time, from a cold disk) |
| Opened (caught up, day entered, planned, saved) | 0.73 s |
| A step a minute | 1.9 ms |
| On through midnight (the next day's timetable read, the day closed, planned, saved) | 1.1 s |
| The company's file | 444 KB |

A midnight costs a company about a second: the companies run on a thread of their own, never on
anything that answers the players.

## Open questions (udkast 4)

The maps to start with; the price of a company and of a depot; whether moving a bus between maps
costs money or time; who pays a lent employee; the limit of a big purchase; the company's share
(a cap, a default); bankruptcy of a depot or the whole company; how often lines go to tender and
how bids are judged; leaving a duty halfway; working for several companies; who may join the
server.

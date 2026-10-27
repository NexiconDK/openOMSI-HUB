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
| Founding (phase 6) | Decided 26 October 2026: besides the price, trips driven for a company on the server (as the world measured them), a leadership course on the server (money and days), and a driving test passed on the server |
| Worlds kept running (phase 6) | Decided 26 October 2026: a world is not closed for want of players while the machine can carry it (memory and CPU under limits of `company-server.cfg`); over them, the one empty the longest goes first |
| Big purchases (phase 6) | Decided 26 October 2026: a depot's purchase over an amount (`big_purchase`) waits for the head office's owner's yes or no |
| A head office in the red (phase 6) | Decided 26 October 2026: the bank's emergency loan first; bankrupt only when that cannot be paid either |
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
   (The tenders brought forward on 25 October 2026, before the depots: "Phase 5 brought forward"
   below.)
6. **Polish**: courses required to found a company; maps kept running if the machine can carry
   them; translations and guides.
7. **The group's head office** (decided 26 October 2026): a company on more than one map is a
   group - a head office with no map above its depots, the first map's included; the group's
   treasury, books and loans; its fleet and staff across the maps; its brand; its roles.

## What is built (phase 1)

**The main server**: `omsi-hub-cli --company-server <folder>` (`company::main_server`), no window,
no map, no graphics. `<folder>/company-server.cfg` (written with the defaults the first time): the
OMSI folder (`root`), the server's clock (`real_time`, or `date` and `time` the first time), the
worlds' game program, ports, how many at once and their idle minutes, and what they run.
`start-company-server.cmd` beside the programs (the release's zip; `scripts/server`) starts it
with a double-click: the first time it asks for the folder and keeps it in
`company-server-folder.txt` beside it, and a folder without a `company-server.cfg` is started only
when the owner says so (an Enter by mistake started a new, empty server).

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
- **The pages read once for everybody** (25 October 2026, the first test at home: the Fleet
  page said "Reading the installed buses…" and the Map "Reading the map…" for good): a player of
  "Play together" has the pages' state of his own swapped in while his command runs
  (`company::Viewer`), and what his page started reading came back into the session's after it,
  never into his. The showroom's catalogue (`dealer::swap_catalogue`) and the map's roads and
  routes (`fleetmap::swap_shared`) stay the session's now, read once for every player. The map's
  roads are read where the company is (`co_more_map` relayed; `fleetmap::roads_for_guest`): the
  company's process on the server reads them itself (it is the game program: `omsi-app` gives it
  `company::set_map_reader`, the launcher service's "map" request), on a thread, kept for every
  player; on a boss's own launcher a player is told it is the boss's (it had read for good).
- **Nobody steps the clock** (25 October 2026: "Simulate until it opens" on a tender moved a
  company four hours ahead of the server, stopped there at a question nobody answers): the
  launcher's simulation never runs on a server company (`company::simulate`; bids and purchases,
  which wait for no step, go through as anywhere - `may_step` had held them too in 0.2.48.779),
  the tenders' step buttons are not offered there, a relayed
  `tender_step` is refused as the real time is (`Need::Clock`), and a question left waiting is
  decided by the dispatcher at the host's next step (`online::Host::tick_in`). A company already
  ahead waits for the server's clock (time never goes back: what happened on the way is booked).
- **The owner deletes his company, and sets its logo** (25 October 2026): `delete_company` from
  the settings, relayed (`relay::OWNER_ON_SERVER`; the page no longer runs `co_delete` on the
  player's own computer for a company of the server - one of his of the same id was deleted
  so), the company's process ending by itself (`company_host`: its session closed, its code and
  session files gone, the world told; the main server says it was deleted, not restarted); a
  logo picture read on the player's computer (`co_logo_read`) and sent as it is (`logo_data`,
  base64, at most 3 MB - the company sessions take 5 MB a message now), kept beside the
  company (`<folder>/logos/<id>-<time>.<ext>`).
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

## Phase 3, step 1: Drive in the map's world

Drive on a duty of a server company joins the company's map's world, the game driving the duty's
bus there - everybody on the map in one world - instead of starting the game alone:

- **The world asked for** (`server_api`'s `world {company}`): of one of the company's players
  (its owner, or a player of its book not sent away; else `NOT_MEMBER`), the main server starts
  the map's world when it does not run (`MainServer::start_world`, the world's LAN status under
  `OMSI_INSTANCE = bco-world-<slot>`: `main_server::world_status`) and says whether it runs yet
  - its map loaded, its timetable and companies taken in: the file it writes in its folder then,
  `main_server::WORLD_RUNNING` with its process id (`company_server::mark_running`), and its web
  gateway answering -, its ports and its tunnel. (27 October 2026: its gateway alone answers at
  once, minutes before the map is loaded - Ahlheim about six -, and a player let in then found a
  world that did not answer him: no bus handed over at his stop, his game's clock set back with
  every reconnection.)
- **The launcher waits for it** (`bco_world`, `together.js`'s `worldOf`: asked every 3 s for up
  to a quarter of an hour, "Starting the world of … (a big map takes a few minutes)" said, then
  how many minutes it has loaded, once a minute), then joins it at the server's own address with the
  world's web port (the main server reached on a network), or at the world's tunnel (reached
  through the main server's).
- **The game joins it** (`join.rs`'s `drive` with `world`): the duty's launch with `lan =
  join:<world>` and the world's timetable (`server_timetable::fetch`, `--content-zip`), under the
  server's name for the player; the world takes the tour's AI bus off while he drives it (the
  LAN's own: the player's `Pose.tour`, `ScheduleSim::set_lan_tours`) and its clock is the
  world's.
- His trips go to the company as before (his game's plugin, his launcher, the company's book):
  the world does not report a player's trips (only its AI buses'), so nothing counts twice.

## Phase 3, step 2: the trips checked by the world, the company's share and the player's pay

Decided 25 October 2026: the game reports, the world checks; the company keeps a share its owner
chooses, the rest is the player's pay.

- **His launcher posts each trip to the world** (`join.rs`'s `post_to_world`: `POST /bco/trip` on
  the world's web gateway, his name on the server and the trip as his game reported it), not to
  the company; the company is told only when the duty is over (`done`). A company on the server
  takes no trips from a launcher any more (`HostState::tickets`: its reports' trips left out).
- **The world checks it** (`omsi-app`'s `company_server`): it keeps where it saw every player's
  bus, every second for four hours (`watch`: `Pose` name, place, tour); a trip is taken when it
  saw his bus on that tour then (ten minutes' slack for his machine's clock) driving at least
  half as far as the trip says (`check`), once (`TAKEN_TRIPS`), and written for the company whose
  line it is on the map (`store::append_world_trip`, `<id>.worldtrips.jsonl`). The gateway takes
  such posts only in a world of Bus Company Online (`ServerInfo::bco_trips_open`, 64 KB a trip,
  500 waiting at most).
- **The company books it** (`company_host` every step: `store::world_trips_from` from
  `Online::world_trips_read`; `together::take_world_trips`): to the duty of his it is a trip of,
  as a report of his launcher's was (`book::report`), and of what it brings the company
  (`book::trip_money`) all but the company's share is his pay - booked as wages
  (`online::pay_of`) and written for the main server (`online::append_pay`,
  `<folder>/pay/<id>.jsonl`).
- **The main server pays it** into his money every few seconds (`MainServer::take_pay`, how far
  kept in `players.json`: nothing twice), and says so in its log.
- **The share**: 25 % when a company is founded (`online::DEFAULT_SHARE`); its owner sets it in
  the company's settings (`dialogs::online_part`, `online_share`, his alone:
  `relay::OWNER_ON_SERVER`), the new one counting from the next day (`Online::share_next`).

Tested: `online::tests::the_worlds_trips_and_the_players_pay`, `a_new_share_counts_from_the_next_day`,
`main_server::tests::the_players_pay_is_paid_into_their_money`,
`company_server::tests::a_trip_is_taken_as_the_world_saw_it`, the gateway's door
(`ws::tests`).

## Phase 3, step 3: a duty that is a part of a tour, and a duty left halfway

Decided 25 October 2026: what a player leaves of his duty the AI drives, or it is dropped - the
company's owner chooses; he is paid for what he drove, and his card counts the duties he left.

- **His game claims his trips, not the tour** (`omsi_sim::timetable_run::LanClaim`): `Pose::tour`
  says, of each tour of his duty in the order he drives them, the departures of his trips still
  to come from the one under way (`linie5/3@570~795;linie7/2@800~860`, minutes of the day; as
  many whole claims as fit in the INFO field's 64 characters, the trip under way first; nothing
  once his last trip is driven: `PlayerDuty::lan_claims`, `lan_claims_text`). An older game's
  `<line>/<tour>` claims the whole tour, as before.
- **The world leaves him those alone** (`ScheduleSim::set_lan_tours`, `is_player_tour`): the AI
  drives the tour up to his first departure - its bus drives off at the terminus where he takes
  the tour over, and one already on his departure goes (`purge_player_tour`): no two buses on one
  tour - and a bus of its own comes for the tour's trips after his last.
- **Left halfway** (his launcher says "leave", his game ended before the duty, or he was not heard
  from: `together::host::left_duty`, as Beta 63 had it): the AI drives the rest from the trip after
  the last he reported (`AiPart`), or - the owner's choice in the company's settings, at once
  (`dialogs::online_part`, `online_left`, his alone: `relay::OWNER_ON_SERVER`;
  `Together::left_dropped`) - nobody does (`AiPart::dropped`): the world keeps those departures
  off the road at once when his claim goes (`TimetableCompany::drops_left`, `LivePlan::drops_left`)
  and from the plan after (`LiveTour::off`, `book::dropped_departures`), and the day's close books
  them as dropped (`DropWhy::Left`: no takings, the authority's penalties). His card counts it
  (`GuestStats::left`, "Duties left"; not when the boss ends the session or sends him away; taken
  off again when his game drove on after all, `book::not_left`).
- **A trip the world checked after his launcher said he left** is his (`book::world_saw`): the
  AI's part begins after it, and goes when it was his last.

Tested: `tours::tests::a_lan_players_duty_takes_only_its_trips_from_the_ai`,
`together::tests::a_duty_its_player_left_is_dropped_when_the_owner_says_so`,
`a_trip_the_world_saw_him_drive_after_he_left_is_his`,
`company_live::tests::on_a_server_the_maps_other_lines_run`,
`company_server::tests::a_trip_is_taken_as_the_world_saw_it`.

Fixed 25 October 2026 (the owner could not start a duty given to "You", nor give a duty under way
to himself):

- **Nobody is "you" of a server company**: its pages offer no "You" among the drivers (the
  roster's, the planning's, a duty's choices); a duty planned for "you" before is open there
  (`book::driver_of`) - the world's AI drives it.
- **A duty under way is a player's from its next trip**: taken or given after it began, the claim
  gets the AI's part until then (`book::joined_part`, `LeftWhy::Begun`: not counted as left); his
  list says "Under way: the AI drives it until you take over", and his Drive takes it over at its
  next trip (`host`'s `start`, as `take_back`). Not when no trip of it is left; elsewhere than on
  the server a duty under way is still not taken.
- **"Drive this duty" on the company's pages, joined from afar**, starts it as his menu's Drive
  does (`kit.js`'s `remoteDrive`), not the boss's duty launcher.

Tested: `together::tests::on_the_server_a_duty_under_way_is_taken_over_at_its_next_trip`.

## Phase 4: depots

Decided 25 October 2026: moving a bus between maps costs time and money; a lent employee is paid
by the depot that borrows him; a depot that goes bankrupt closes - its loss the head office's
when it can pay it, else the head office closes or sells more; what a depot costs is not decided
yet (`depot_price` in `company-server.cfg`, 250 000 until then). In steps: (1) a depot founded on
another map; (2) money moved between the head office and its depots, the head office's view of
them; (3) buses moved between maps, staff lent; (4) a depot's bankruptcy - all four done.

### Step 1: a depot founded on another map

- **A depot is a company of the server on another map** (`server_depots`), tied to its head
  office (`Online::hq`): its own lines, buses, staff, tenders (its map's market) and books, its own
  process; with the head office's name and the map's ("Umove Ahlheim"), its mark, game mode,
  country, currency, owner and share of its players' trips.
- **Its owner founds it** in the head office's settings ("Depots": `dialogs::depots_part`,
  `depot_found`, his alone: `relay::OWNER_ON_SERVER`): a map of the server the company is not on yet
  (`maps_for`), and its starting money; the head office pays `depot_price` (booked as
  construction) and the starting money (capital it puts in) from its own money
  (`server_depots::found`). The main server starts its process at its next look into the folder.
- **Its players** join it from "Bus Company Online", where it is listed under its head office
  ("Depot of Umove": `server_api`'s `companies`, `hq`); its depot leader is a co-owner of the
  depot the owner names on its Multiplayer page. A depot's settings say whose it is; a depot
  founds no depots.
- `depot_price` is added to an older `company-server.cfg` (`DEPOT_CFG`).

Tested: `server_depots::tests::a_head_office_founds_a_depot_on_another_map`.

### Step 2: money between the head office and its depots, its view of them

- **Money is sent** by the owner in the settings (`depot_money`, his alone): from the head office to
  one of its depots, from a depot back to it (`server_depots::send`) - out of the sender's books at
  once (capital it takes out), into the other's at its process's next step (`take_transfers`, from
  `Online::transfers_read` in `<folder>/transfers/<id>.jsonl`; money only from a company of its
  own is taken, nothing twice). Between a head office and its own depots only, at most what the
  sender has.
- **The head office's view** of its depots in its settings: each with its map, its lines, buses
  and staff, its money and this month's result so far, and money to send it.

Tested: the same test, its step 2.

### Step 3: buses moved between maps, staff lent

Decided 25 October 2026 (the owner took the proposed defaults): a move takes `bus_move_days` (2)
and costs `bus_move_price` (5 000) a bus, both in `company-server.cfg` (`MOVE_CFG` added to an
older one).

- **A bus of the company's own** moves to its head office or another depot of the owner's
  (`server_depots::relatives`; `bus_move`, the owner's alone): out of the fleet at once - the
  month's fixed costs up to the day paid as a sale pays them, the move booked as services
  (`move_bus`) - and into the other's fleet on the day it arrives (`take_buses`, from
  `Online::buses_read` in `<folder>/buses/<id>.jsonl`): an id of the other's, its fleet number when
  that is free, from that day its own. Not a leased or rented bus (its contract stays), not to a
  depot with no room, not to a company of another's.
- **An employee is lent** for 1 to 90 days (`staff_lend`, the owner's alone; `lend`): he goes the
  next day (`people_step`, each step of the process), the guarantee of his week paid by the lender
  (`pay::settle`), and works at the other with an id of its own (`Employee::lent`) - paid by it
  while he is there, not to be dismissed by it - and comes back after his last day with his own
  id (`<folder>/people/<id>.jsonl`, `Online::people_read`).
- **The settings show** the buses on the way (either way), who is to be lent, who works at another
  company of the owner's and who is lent to this one (`dialogs::moves_part`).

Tested: `server_depots::tests::a_bus_moves_and_a_driver_is_lent_between_a_head_office_and_its_depot`.

### Step 4: a depot that goes bankrupt

Decided 25 October 2026 (the owner: "a depot that goes bankrupt closes - its loss the head
office's when it can pay it, else the head office closes or sells more"), and 26 October (a
warning a day or two before):

- **In the red at a day's close** (`server_depots::day_closed`, each closed day of the depot's
  process): the depot is warned - "%{n} days to go", then "tomorrow" - and so is its head office
  (`Notice`s through `<folder>/notices/<id>.jsonl`, `take_notices`: into its inbox), `RED_DAYS`
  (2) times; a close out of the red starts the count again (`Online::red_days`).
- **Still in the red the day after** (`bankrupt`): its buses sold (a leased one handed back
  early, a rented one given back), its staff gone (each with his week's guarantee; one lent to it
  back home), its loans paid off, its lines, concessions and contracts given up - out to tender
  again -, and what is left handed to the head office with the notice: its loss covered (booked
  as capital taken out; without the money the head office is in the red, told to sell or close -
  the whole company's bankruptcy is an open question), or what remains. The depot's files go and
  its process ends (`Stepped::bankrupt`, `company_host`).

Tested: `server_depots::tests::a_depot_in_the_red_is_warned_twice_then_goes_bankrupt`.

**And a line won for a later day** (fixed 26 October 2026: the world counted "0 of its lines"
for a company that ran them): it is the company's on the road at once - the map's own buses off
it - unless it was won from another company of the map, which runs it until then
(`Concession::after`): then neither the line nor its tours (`plan::live_plan`). Tested:
`plan::tests::a_line_won_for_later_is_on_the_road_unless_another_runs_it_until_then`.

## Phase 5 brought forward: the map's tenders, shared by its companies

Decided 25 October 2026 (two companies on one map each bid in a market of their own and never saw
each other): every company of a map bids in the same tenders, live against each other and
against the map's operators; a concession ending goes out again open to every company; no line is
bought outright on the server; the operators still bid.

- **The main server keeps a market a map** (`shared_tenders`, `<folder>/market/<map>.json`,
  `MainServer::markets` at every scan): every four weeks of the server's date some of the map's
  lines no company runs go out to tender (the Normal rules' number and one more a company of the
  map), one a day in the office hours; a company's concession ending within `RENEW_BEFORE` days
  goes out again, open to every company - the winner runs the line from the day the old term
  ends (`Shared::from`), the holder until then; a company's application (its Lines page's
  "Apply": `shared_tenders::ask`, `<map>.asks.jsonl`) opens its tender at once, or is refused when
  another company runs the line. Prices, caps and the operators are the market's: priced at the
  costs of a company founded on the map that day (`template`), the operators drawn once for the
  map, bidding by the Normal rules.
- **Every company bids in it**: its process takes the market's open tenders among its own
  (`sync`, each step of `online::Host`; `concessions::Tender::market`), its pages and clock as
  ever - its bids go into the market (`shared_tenders::bid`, `<map>.bids.jsonl`), the others'
  come in at its next step, and the auction is replayed with every company's bids in one order
  for all (`auction::replay_many`, `Who::Other`), so each sees the same auction from its side.
  Its own tenders of before are withdrawn, their fees paid back.
- **The main server decides** each tender a minute after its last minute, every bid in
  (`CLOSE_AFTER`, `decide`): won by a company - its process takes the concession and its contract,
  its line the company's at once (`take_on`), the others lost to it by name - by an operator (the
  map's own buses run the line until it goes out again), or by nobody. A company's clock no longer
  closes such a tender itself.
- **Not on the server**: buying a line outright (`concessions::NOT_ON_SERVER`, its button gone),
  taking a line on directly or a concession renewing by itself (`rules_of`: `direct` off), the
  company's own rounds and renewals.
- **A Casual company** bids as every company does (the owner's wish: everything else of it runs
  by itself): on the server it has the Concessions page (`company::casual_tenders`), its "add a
  line" applies for the line in the market, and the wizard shows no asking price.
- A line whose concession begins on a later day is not in the company's plan for the world yet
  (`plan::live_plan`'s `lines`): the holder's buses run it until then.
- **A tender's tours by the day** (the owner's wish, 25 October 2026): the main server reads the
  week of the map's timetable for the open tenders that have none (`shared_tenders::fill_weeks`,
  at its scan; not again that day when a day could not be read), the companies take it
  (`sync`), and the card says "X tours Mon–Fri · Y Sat · Z Sun" (`concessions::Week::mon_fri`,
  `sat`, `sun`; a company's own page the same from its own reading of the week). A tender's close
  moved in the market's file is the companies' too.

Tested: `auction::tests::two_companies_in_one_tender_see_the_same_auction`,
`shared_tenders::tests::two_companies_bid_against_each_other_in_the_maps_tenders`,
`a_concession_ending_goes_out_to_every_company`, `an_easy_company_applies_and_bids_itself`.

### The server's own contract rules

Decided 25 October 2026 (the owner's rules; on the server alone - a company elsewhere keeps the UK
economy's):

- **A tender runs three days** (`shared_tenders::TENDER_DAYS`); a concession is put out again six
  days before its new term begins and decided three days before it (`RENEW_OPENS`,
  `RENEW_DECIDED`): the winner has three days to make ready.
- **The term is by the buses bid with**: the company chooses diesel or electric buses for its
  bids in the auction (`concessions::set_drive`, `tender_drive`; electric where its level opens
  them), each bid carries them (`MarketBid::drive`) and the winning bid's decide
  (`Decided::drive`): two weeks with diesel buses, a month with electric ones
  (`concessions::server_term`), without extension years; a contract won with electric buses runs
  its tours with electric buses (`bus_contracts::Contract::electric`).
- **The buses are the timetable's**: each tour is run with the bus its timetable asks for (its AI
  group: `network::wants_size`), or a bigger one; the tender keeps its tours by the bus they ask
  for (`concessions::Sizes`).
- **The contract requirements** - a button on a tender's card - show all of it: the tours by the
  day and by the bus, diesel or electric, the three days of bidding, the term's first day, the
  renewal, the buses taken over, the cap and the fines (`concessions::requirements` in the
  launcher).
- **The authority's fines** (`server_fines`, in Danish kroner in `company-server.cfg`, each company
  paying them in its own currency; `bus_contracts::authority_fines` at the day's close, booked as
  penalties): 20.83 kr a minute of a journey not run, 2 500 kr a journey with a smaller bus than
  its tour asks for (the owner gave no amount: proposed), 2 500 kr a journey with a diesel bus on
  a contract won with electric buses (an electric bus on any tour is no fault), and for each
  complaint 2 000 kr (the driving), 1 500 kr (the temperature in the bus), 2 500 kr (the route
  driven, a ticket sold wrong, a red light run). The complaints are drawn by the journey from its
  driver's skills and its bus's condition (`COMPLAINT_*`); a player's red lights are the ones his
  trip ran.
- **Bids by the timetable hour** (the owner's wish the same evening): an ask is a price a timetable
  hour (a tour's first departure to its last arrival); the auction keeps a year's sum - the price
  times the tender's hours a year (`concessions::hours_year`, `per_hour`, `of_hour`) - so the
  70/30 score is as before, the lowest price winning on equal quality. The cap is the most the
  authority pays an hour (`cap_per_hour`, 1 500 kr in `company-server.cfg`); the auction's panel
  shows the cap, the advice, the asks and the bids an hour, and what the ask comes to a week and
  for the contract's days by the buses bid with.
- **Paid by the timetable hours run**: a contract won so (`Contract::per_hour`) pays each day
  the day's timetable hours of the line (`Journeys::timetabled`) less those of the journeys not
  run, at the ask's price of an hour; nothing else is deducted - the fines are the authority's.
- **Departures early and late** (2 500 kr each, `fine_early`, `fine_late`): a departure from a
  stop more than a minute early, or more than 15 minutes late unless the delay was reported to
  the control room - a player's from his trip's stops (`server_fines::departures`) with the
  "Report a delay" of his duty (`report_late`, `Together::late_reports`); a modelled journey's
  as it ran, its delay reported when the dispatcher is on (`clock.dispatcher`).
- **The buses taken over** (`takeover`): a concession won from another company - the loser offers
  the winner the buses of its own it ran the line with (its roster's tours of the line), at their
  book value; the winner takes all of them or none (the Concessions page's "Buses offered",
  `takeover_yes`, `takeover_no`); on the new term's first day they leave the loser's fleet, sold,
  and join the winner's, paid. Through `<folder>/takeover/<id>.offers.jsonl`, `.answers.jsonl`
  and `.buses.jsonl` (`Online::takeover`).

Tested: `shared_tenders::tests` (the renewal's days, the terms by the buses, the cap an hour),
`bus_contracts::tests::the_authoritys_fines_on_a_server_contract`,
`a_server_contract_is_paid_by_the_hour_and_its_departures_fined`,
`server_fines::tests`, `takeover::tests`.

### Danish time, and a player's trip by its tour

Fixed 25 October 2026 (the night summer time ended the server kept its hour ahead; a player's
finished duty said "You left it: the rest of it is dropped"):

- **The server keeps Danish time** on the real time (`online::danish`: UTC and an hour, another in
  the EU's summer time - the owner's choice: Danish time only): behind it, its clock goes on at
  once; ahead of it - summer time over - it goes back at three in its own night, to two, as the
  clocks do (`MainServer::keep_danish_time`, `online::set_server_clock`). The companies' processes
  take the clock in again (`Host::set_server_clock`) and wait the hour out; the worlds take it
  with the plans; a trip of the world's told again in that hour is booked once
  (`day::record_live`).
- **A player's trip is the company's by its tour** (`LivePlan::trip_line`): the world looked for
  the company by the trip's line among the company's lines alone, and a line whose concession
  begins on a later day is not among them - the trip went to no company, and his duty stayed as
  if he had left it. Now the tour of the company's plan it is a trip of decides (its line's
  timetable name or the number the bus showed), and the trip carries the company's name for the
  line from there on.

Tested: `main_server::tests::the_server_keeps_danish_time`,
`plan::trip_line_tests::a_players_trip_is_the_companys_by_its_tour`.

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

## Phase 5: the control room per depot

Decided: "Per depot: every bus live while the map's world runs (interventions on the AI buses),
else where the timetable has them."

### Step 1: the world's buses live

- **The world writes what it sees** (`omsi-app`'s `company_server::publish_live`, from the
  server's step): every couple of seconds (`world_live::EVERY`) the companies' timetable buses on
  its roads (`ScheduleSim::live_buses`: only those of a company's line, `TimetableCompany::owner`)
  and the players' buses on a company's tour (by the tours their games claim, `lan_claims`), whole,
  into `live/<map>.json` of the server's folder (`company::world_live`): the company, line, tour,
  trip and its departure, the place in world metres, heading, speed, delay, passengers, at a stop
  or at the terminus, the next stop, the fleet number; a player's name and his tours.
- **The control room** is the depot's fleet map (and with it the Operations page): on the server
  it reads the file every couple of seconds while it is fresh (`world_live::FRESH`, 15 s), the
  company's own only, and puts each block's bus where the world has it - an AI bus with the
  world's delay, the trip it drives (by its departure), what it does and its next stop, a player's
  bus where his game is; driven on straight from there for up to 3 s, so it glides between the
  world's writes. A bus the world does not have on its roads (the world not running, the bus in
  the depot) stays where the timetable has it. The header says "Live: the world's buses (N on its
  roads)", the card "Live position from the server's world".
- **Fixed**: the world measured a player's bus by x and z (z is up on the map's ground), so a trip
  that ran mostly north-south could be refused ("his bus drove far less than the trip says"); x
  and y now.

Tested: `world_live::tests::a_company_sees_its_own_buses_while_the_world_writes`,
`fleetmap::tests::a_bus_of_the_world_is_driven_on_between_its_writes`.

### Step 2: service control on the world's buses

Decided 26 October 2026: while the world runs, service control's delays and alerts are the
world's; the acts that reach the world's AI bus: out of service to the terminus, a trip
cancelled, a spare bus swapped in at the terminus (not the short-turn). Without the world the
company's own model runs as ever - the timetable with the delays it draws.

- **The world's delays**: the company's process keeps what the world sees of its buses
  (`world_live::WorldSeen` in `Online::world`, read each step; not saved, never a change of the
  company). The day's one delay model (`lateness::model_with`, while the clock looks - not the
  close) takes the trip a world's bus is on (by the trip's name and departure) as the world has
  it: under way it is as late as the world says, its own minutes from there not drawn; waiting at
  its first stop it leaves now at the latest. The trips ahead carry that along the block as ever,
  so the Operations page's late buses, its alerts, the badge and Casual's controller go by the
  world. The other buses run as the model has them.
- **A trip cancelled** (`control::Act::Cancel`, new): nobody runs it, its bus takes the next on
  from there; offered for a late bus ("Cancel the trip") with what it costs - the trip's penalty,
  its passengers' fares, a dropped trip's reputation - but never the controller's suggestion
  (nor Casual's by himself). The model leaves it out; the close books it dropped, "Cancelled by
  service control" (`day::DropWhy::Cancelled`).
- **To the world**: the plan written for the world carries what was done on each tour
  (`LiveTour::off` with the trips cancelled, `express`, `swaps` with the spare bus's file, paint,
  number and plate), written again when the controller acts (a page's act, or Casual's in the
  host's step). The world (`ScheduleSim::company_acts`, each tick): a bus on a trip run out of
  service stops nowhere but its terminus (`BusService::express`); a bus on a trip newly
  cancelled goes back to its depot (it drives off) and the trip leaves the queues; at a
  departure a spare bus is swapped in at, the tour's bus does not take it on - it drives off at
  the terminus - and the departure gets a bus of its own, the spare in its paint
  (`TimetableCompany::bus_at`).
- The world's list of buses leaves out one that drove off (`car.gone`).

Tested: `lateness::tests::the_worlds_bus_is_as_late_as_the_world_says`,
`control::tests::a_trip_cancelled_is_not_run_and_the_bus_takes_the_next_on_time`,
`timetable_run::sim::tours::tests::the_control_rooms_acts_reach_the_buses_on_the_road`.

## Phase 6: polish

### Step 1: what founding a company asks

Decided 26 October 2026: besides the price, trips driven for a company of the server, the
leadership course and a driving test passed on the server (`company::server_found`; each on or
off in `company-server.cfg`: `found_trips = 20`, `found_course = 1` with `course_price = 5000`
and `course_days = 3`, `found_test = 1` - added to an older file the first time).

- **Trips**: every trip of a player's that a company booked from the world's measurement goes to
  the main server as his pay does (`online::Pay`, written now for a trip paid nothing too), and
  the main server counts it once (`server_found::take_trip`, kept in `players.json`: `trips`).
- **The leadership course**: booked from the launcher for `course_price` of his own money; done
  `course_days` days of the server's clock later (`course_until`).
- **A driving test**: booked from the launcher for a class of bus (midi, solo, articulated,
  double-decker); his next trip on the server with a bus of that class is the test, judged by
  his company's process from the trip the world measured as the career's test is
  (`career::judge_exam`: driven to its end, punctual, smooth, safe) and sent with the pay
  (`Pay::class`, `Pay::exam`); passed, the class is his (`tests`), failed, he books it again.
- **The door**: `me` and `companies` answer what founding asks and how far he is (`found`),
  `course` books the course, `test` a test (`class`; none: called off), and `found` refuses with
  what he lacks first ("Drive %{n} more trips…", "Take the leadership course first.", "Your
  leadership course ends on…", "Pass a driving test on the server first.").
- **The launcher**: "Found a company" lists what founding asks, each done or not, with "Book the
  course", the test's class and "Book the test" / "Call it off"; "Found" opens when all is done.
- **The console**: `players` shows each player's trips, course and tests; `grant <player>; trips
  <n> | course | test <class>` gives them (the server's owner, his friends before the rules).

Tested: `server_found::tests::a_player_founds_after_his_trips_the_course_and_a_test`,
`server_api::tests::a_player_logs_in_founds_and_joins`.

### Step 2: the maps' worlds kept running while the machine can

Decided 26 October 2026 (`company::server_machine`; `company-server.cfg`: `keep_worlds = 1`,
`max_memory = 85`, `max_cpu = 90` - added to an older file the first time):

- A world no longer stops by itself for want of players (its `server.cfg`'s `idle_stop = 0`;
  with `keep_worlds = 0` it does after `world_idle` minutes, as before).
- Every half minute the main server looks at its worlds - which have had no players since when
  (each world's LAN status, `world_status`; a world younger than five minutes counts as busy) -
  and at the machine: its memory in use and its processors busy over the last minute
  (`/proc/meminfo` and `/proc/stat` on Linux, `GlobalMemoryStatusEx` and `GetSystemTimes` on
  Windows). Over a limit, the world empty the longest is stopped (one a look); with every world
  busy, it says so once.
- A world asked for while `max_worlds` run already takes the place of the one empty the longest.
- `status` shows the machine's load against its limits and how long each world has been empty.

Tested: `server_machine::tests::the_world_empty_the_longest_goes_when_the_machine_is_full`,
`main_server::tests::a_worlds_settings`.

### Step 3: a depot's big purchases, approved by its head office

Decided 26 October 2026 (`server_depots`, `company-server.cfg`: `big_purchase = 500000`):

- A depot's purchase over `big_purchase` - a bus from the dealer, quick or by contract, new or
  used, paid or on a loan (a lease is no purchase) - is refused until its head office approved
  that much ("A purchase this big needs the head office's approval first…": `check_spend`).
- The depot asks in its settings, under Depots: an amount and what it is for (`approval_ask`, a
  player who may spend its money). The request goes to the head office
  (`approvals/<hq>.asks.jsonl`), into its inbox and its settings' Depots: "Waiting for your
  approval", Approve or Refuse (`approval_answer`, its owner's). The answer goes back
  (`approvals/<depot>.answers.jsonl`): approved, the amount is the depot's to spend on purchases
  over the limit (`Online::approved`, spent by each: `spent`), refused, its inbox says so.

Tested: `server_depots::tests::a_depots_big_purchase_waits_for_its_head_offices_yes`.

### Step 4: a head office in the red

Decided 26 October 2026: the bank's emergency loan first; bankrupt only when that cannot be paid
either (`server_depots::day_closed`):

- A head office that closes its day in the red gets an emergency loan for the deficit and a
  quarter more (whole thousands), over 12 months at its rate and 5 points more
  (`finance::emergency_loan`, "Emergency loan"), and the day is fine; its inbox says so.
- In the red again while that loan runs, it is warned at each day's close - two days to go, then
  one - and the day after, still in the red, it goes bankrupt: everything sold, its staff gone,
  its loans paid off, its lines out to tender again (`wind_up`), and each of its depots told to
  close with it (a notice with `close`: the depot's process winds it up and ends as its own
  bankruptcy does).
- A depot is never lent: it is warned and goes bankrupt as before (phase 4 step 4).

Tested: `server_depots::tests::a_head_office_in_the_red_gets_an_emergency_loan_then_goes_bankrupt`.

### The console's clock (26 October 2026)

`clock` shows the server's clock and the machine's time; `clock now` sets the clock to it at once
(`MainServer::set_machine_time`) - ahead, the companies wait the difference out and the worlds run
it again.

### The machine's own time (26 October 2026)

Decided 26 October 2026 (the server's clock still an hour ahead the day after summer time ended):
a server on the real time keeps its machine's own time - the dedicated server's machine, its zone
and its summer time (`online::machine_time` with `crate::utc_offset`) - no longer a Danish time
of its own, and a minute or more off it goes to it at once, on or back, day or night
(`MainServer::keep_machine_time` at every look into the folder). Tested:
`main_server::tests::the_server_keeps_the_machines_time`.

### Step 5: the players' guide

Decided 26 October 2026: both in the launcher and as a document to share.

- **In the launcher**: "The guide" on Bus Company Online's page (before and after logging in)
  opens the guide in the player's language (`bco_guide`, `bco.rs`'s `guide`): what it is, getting
  in, joining a company, driving and pay, founding your own company (what this server asks, from
  its door's answers; the defaults before it is linked), lines and tenders, the contract's rules,
  the control room, depots, the red, the server's clock.
- **The document**: "Bus Company Online – spillerguide", a Claude doc with a Danish and an English
  tab, for the server's owner to share with his players.

### A duty taken over from the company's AI driver (26 October 2026)

On the server every driver of the company - its own staff, an agency's - is the world's AI under
that driver's name, and the dispatcher gives them most duties of the day. A player may take such a
duty over (`together::book::takes_over`), on the same terms as an open one: at once when the
company lets players take open duties (or he may approve requests), else asked for. Not begun, it
is his from its start; under way, from its next trip - the AI drives it until his Drive takes it
over (`LeftWhy::Begun`, as an open duty taken under way); with no trip of it left, it stays the AI
driver's. The driver it was planned for is free for other work (the plan puts the player first);
whoever may give duties can give it the same way. In the launcher: "Duties" → "Driven by the AI",
each with "Take over this duty" / "Take over from the next trip" (or "Ask to …"). Elsewhere - a
player's own company - the boss's drivers keep theirs. Tested:
`together::tests::on_the_server_a_player_takes_a_duty_over_from_the_companys_ai_driver`.
When none of the AI drivers' duties of the day has a trip left to take over (the evening), the
Duties tab says so (`join.rs`'s `NO_AI_LEFT`) instead of showing nothing.

### A player in the roster of a coming day (26 October 2026)

The roster (RosterFlow's Roster) of a day to come offers every player of the company (not sent
away) beside its drivers - before, only the players online, and only for the company's day. Given
a duty there, the player is in the weekly roster (`Who::Guest`), and the day's plan shows him on it
(`plan::day_plan`). On the company's day only a claim puts a player on a duty: as the day begins
the roster's become his claims, planned as given (`together::book::claims_from_roster` in
`clock::begin_day`) - not when somebody has the duty, it lies over another of his, has no bus of
the company's, or he was sent away. Tested:
`together::tests::a_player_planned_in_the_roster_has_the_duty_on_its_day`.

### The day's report for a player (26 October 2026)

"Show the report" of the last day closed opened nothing for a player of the company - the owner
of a server company too, who is one: the shell a player sees (`relay::shell_for`) hid the day's
reports as the boss's own, though they are the player's own (`company::Viewer`). They are shown
now to a player who may see the money. Tested: `relay::tests::the_shell_shows_a_player_only_his_pages`.

### Relief at the stop (26 October 2026)

Decided 26 October 2026: a duty that begins with its bus in service - taken over from the AI from
its next trip, a duty that begins mid-tour or at a relief on the way, or after another player's -
starts on foot at its stop, and the bus is handed over there; the AI's bus waits five minutes past
its departure, then the AI drives that trip and the relief moves to the next one.

- **The launcher** starts such a duty in the map's world with `--relief` (`Duty::relief`: a
  duty whose first trip is not the tour's first, or that begins at a stop on the way; `join.rs`'s
  `drive`).
- **The game** (`omsi-app`'s `relief`): no bus at the start - the player on foot at the stop
  (`duty_start::relief_stop`, the trip still to be taken over: within `RELIEF_WAIT` of its departure
  there), the duty on its trip and stop. Every two seconds it asks the world about its bus
  (`relief want <line>;<tour>;<dep>;<stop>`, a LAN command); while it waits it does not claim that
  trip (`PlayerDuty::lan_claims_from`: claimed, the world would take the AI's bus off the road),
  only the trips after it. The trips of the duty before it, gone already, it tells the world once
  (they are the AI's). No answer within 30 s while the world is heard from (a world without a
  timetable, an older one - not counted while it says nothing at all, `LanSession::host_quiet`):
  his own bus by the stop. The world sends a player on foot its frames around his walker (`lan_world`: no
  bus of his own).
- **The world** (`omsi_sim`'s `ScheduleSim::want_relief`, `sim/relief.rs`): while he waits the
  departure is the AI's although his claim has it - the tour's bus takes the trip on at the
  terminus rather than driving off, or one is put out for it (no further than his stop when it is
  late: `relief_place_time`) - and the bus serves his stop whatever else it would do and holds
  there with its doors open (`bus_service::ReliefHold`) until `RELIEF_WAIT` past its departure; a
  bus that comes later holds a minute after it came (`RELIEF_ARRIVED_WAIT`), and one that never
  comes leaves the trip to the AI ten minutes after (`RELIEF_NO_BUS`). A relief not asked for again
  in `RELIEF_STALE` is let go. A night trip is the one nearest the clock (tonight's 24:30, not the
  morning's 0:30). A question still on its way when the bus was handed over is answered "taken".
- **Taken over**: walked up to the bus (16 m), his game asks for it (`relief take`); the world
  takes the bus off its roads (`take_relief`) and answers with where it stood, its fleet number and
  its passengers (`relief bus …`). His game puts his own bus there (the duty's bus, or the one the
  world described: a spare bus), with as many passengers, running; its driver (an avatar of the
  people's animation) gets up and walks out of the front door and away; then G at the front door
  takes the wheel (before that: "Wait: the driver gets out first"). The passengers stay in while he
  walks up to the door.
- **Not there in time**: the AI drives that trip (`relief_kept`), his game waits for the bus at the
  start of his next trip ("ride along": G at a door of the AI's bus takes a seat in it), and the
  world writes it for his company (`company_server::relief_kept`, a `WorldTrip` with `kept`): his
  company books the trip as the AI's part of his duty (`book::ai_kept`, `LeftWhy::Begun` - he did
  not leave it).
- **From another player** (`ReliefState::FromPlayer`): when another player's claim has the trip
  before (a relief at the trip's first stop) or the trip itself (a relief on the way), it is his
  bus to hand over. The world
  tells him (`relief give`): at the stop his game says "… waits here to take your bus over: stop,
  get up and get out"; out of the bus, standing within 60 m of the waiting player, his game gives
  it on (`relief gave`, `gave_relief`; given again - an answer lost -, answered the same) and,
  told so (`relief given`), lets it go and stays on foot; the waiting player's game puts the bus
  down where it stood (no driver to get out). A player whose duty goes on after it, on another
  tour, has no bus of his own after it (not handled yet). The giver not there within five minutes of the
  departure, or gone from the session: the AI brings a bus to the stop.
- **A world without the trip** (`relief unknown`): his own bus on the road by the stop.

Tested: `omsi_sim`'s `ai_traffic::service_tests::a_bus_holds_at_the_stop_its_relief_waits_at`,
`timetable_run::sim::relief::tests::a_lan_player_takes_the_ais_bus_over_at_his_stop`,
`together::tests::a_trip_the_ai_drove_while_he_was_not_at_his_stop_is_the_ais`,
`relief::tests::the_same_vehicle_file_however_written`.

### Single trips (27 October 2026)

Asked by NexiconDK on 27 October 2026: a player may choose a trip to drive rather than a whole
duty - by its line or the stop it leaves from, as the HUB picks a start, by a bus on the fleet map,
or simply the next free one -, takes the bus over at its stop, and the AI drives the tour on after
it; at its end he may drive the next trip too. Decided: all three ways to find one; at the
terminus a choice, hand the bus back or drive on; the company's owner switches it on and off (on by
default), and duties planned for players come first.

- **The claim** (`together::book::Claim::part`): a claim of the duty with only some of its trips -
  from one up to another (indexes into its trips). The duty stays its driver's - the company's AI
  driver in the world -: the plan does not put the player on it (`guest_on`), the driver's pay and
  hours do not change, and nobody takes the whole duty while a trip of it is a player's. Other
  players may take its other trips, a claim each (`Together::trip_taken`, `claim_for`: a player's
  own claim of a duty; `trip_claim`: the claim a reported trip is of). Only trips with passengers
  that are whole (not cut at a relief point on the way), of the block's own timetable tour
  (`book::single`), of a duty a driver of the company's has (`takes_over`), leaving at least
  `TRIP_LEAD` (4) minutes from now - the next free one, his game started at once, `NEXT_LEAD` (8).
  It lies over none of his other duties or trips (`overlapping` by the minutes it takes,
  `span_of`). Taken at once, or asked for as a duty is (`open_claims`).
- **Finding one** (`book::free_trips`, `TripFilter`): by line (name or number), stop, or - from the
  fleet map - tour and departure; the soonest first. The session's requests: `trips` (answered
  with a `wire::TripList`: the lines and stops there are free trips of, at most 60 trips and how
  many more), `take_trip` (`duty`, `trip`) and `next_trip` (the first that fits his day,
  `book::take_next_trip`). In the launcher: the joined company's "Single trips" tab - a line and a
  stop to look by, "Take", "Take and drive", and "Drive the next free trip" (takes it and starts
  the game); on the fleet map, a bus's card offers "Drive its HH:MM trip" (its block's next free
  trip, `fleetmap::next_single`) to a player of the company joined from afar. His own trip shows
  under "My work" with Drive and Give back; another player's as "… drives the HH:MM trip".
- **Driving it** (`host::start_part`): his game starts on those trips alone (`--duty-leg` of the
  trip; from the first he has not driven, `book::part_from` - not one that left before he started
  it), with relief at the stop when it is not the tour's first, and with `--drive-on <n>`: how
  many trips of the duty after his he may drive on with (`DriveStart::more`, from the timetable, up
  to a trip another player took, not a trip handed on halfway).
- **At the terminus** (`omsi-app`'s `relief::drive_on`): on his last trip the game menu offers
  "Drive on with the next trip", and the trip's end says so. Chosen, his duty gets the tour's next
  trip (`ScheduleSim::extend_duty`; not one that left a minute or more ago): his claims in the
  world take it from the AI (its bus waiting at the stop, if it was already put out, goes), and the
  company books it as his - his part grows when the world
  reports it (`book::report`: the next trip with passengers of the duty after his). Not chosen, his
  duty ends there and the world gives the tour back to the AI, which drives it on with a bus of
  its own - as at the end of any duty.
- **The books**: his trips are booked as measured with the players' (`GuestTrip::part`); the day's
  close runs the rest of the duty with its driver and leaves those trips out
  (`book::part_drove`). A single trip counts as trips in his figures, not as a duty. Not at the
  stop in time, the AI driver drives it and the trip is the company's again (`book::ai_kept`).
  Given back or left before it left: the company's again, said in the news ("… gave the HH:MM trip
  … back: the AI drives it"); taken and never started, the same six minutes after it left
  (`book::stale_trips` in `HostState::watch`: the AI's bus waits five for him). Left after it began:
  counted as left (`Claim::left`) and kept, so that the world's check of it can still come - when
  all its trips come after all (his launcher's "done" or silence came first), he did not leave it.
  Not kept past its day (`after_close`), unless his game drives it as the day closes: then it is
  kept as a duty of a closed day is.
- **The owner's choice**: Settings → "Single trips" ("Players may take them" / "Whole duties
  only", `online_trips`, `Together::trips_open`; a company saved before has it on).

Tested: `together::tests::on_the_server_a_player_takes_a_single_trip_and_the_ai_drives_the_tour_on`,
`the_next_free_trip_is_taken_and_one_given_back_before_it_leaves_is_free_again`,
`a_single_trip_the_ai_drove_or_he_left_under_way`,
`a_single_trip_never_started_goes_back_after_it_left`, and `omsi_sim`'s
`timetable_run::sim::tours::tests::a_duty_driven_on_gets_the_tours_next_trip`.

## Phase 7: the group's head office (koncern-HQ)

Decided 26 October 2026: a company that runs on more than one map is a **group**. Its head office
has no map, no lines, no buses and no process of its own; every department of it is a depot -
the first map's too ("Umove Ahlheim" beside "Umove Grundorf") - with its own money, lines, buses,
staff and books. The head office keeps the group's treasury and its loans; the depots keep their
own money, with money moved by hand and by each depot's rule (its surplus over an amount sent
home, topped up below an amount). Its owner runs it from his launcher; the depots are joined as
companies are.

### Step 1: the head office, and the head offices of before split

- **The head office** (`company::server_hq`) is a company's file all the same:
  `<companies>/<slug>.hq.json`, its id `<slug>.hq` - with a dot, which no look into the companies
  takes for a company (`online::company_ids`, `store::list`): no process, no tenders, no world.
  The depots' files that name their head office (`server_depots`' transfers, notices, approvals)
  reach it by its id as they reached a head office; on its own file `Online::group` holds the
  group's part (`server_hq::Group`: the depots' rules, later its brand and roles).
- **Made with the first depot**: a company on its own that founds a depot (its settings' Depots)
  becomes the group's first depot - its head office made with the company's name, mark, game
  mode, owner and no money (`server_hq::new_group`, kept once the depot is founded:
  `join_group`), the company named by its map - and the new depot is tied to the head office too.
  The company pays for it as before. From then on the head office founds the depots, from its
  treasury (its page's Found a depot: `server_depots::found` with the head office).
- **The head offices of before** - a company of the server with depots - are split when the main
  server opens its folder, before any company's process runs (`server_hq::split_head_offices`):
  the head office made, the company its first depot named by its map, its depots tied to the new
  head office, the requests for approval waiting for it (taken, or still in its file) the head
  office's. The company keeps its money and loans - only what it is in the red and the bank's
  emergency loan are the treasury's; the treasury starts with nothing else. Kept in that order
  (the head office, the depots, the company last), a split cut short is done again at the next
  start with the same head office (`<its id>.hq`).
- **The main server keeps it** (`MainServer::groups`, every look into the folder:
  `server_hq::step`): the money, the news and the requests the depots sent; its days closed on
  the server's clock - in the red the bank's emergency loan, in the red again while it runs warned
  twice and then bankrupt (its depots told to close, its file removed) -, its loans' rates at a
  new month; the depots below their rule topped up. `status` lists the groups.
- **The red is the whole group's** (`server_hq::day_closed`): the treasury short at its day's
  close while the whole group is not in the red, what it lacks is called in from the depots with
  money - the richest first, each keeping what its rule tops it up to - by an order each
  (`send_home`, done at the depot's next step; what is on its way home counted); only the whole
  group in the red brings the bank's emergency loan, the warnings and the end.
- **What a company removed leaves**: the files that name it as their reader (transfers, buses,
  people, notices, approvals, the head office's orders: `server_depots::files_of`) go with it
  (`store::delete`), and a new head office takes no id they are left under - a company founded
  later under the same id starts afresh.
- **The server page**: a group is the overview of its depots (their lines, buses and depots
  summed, its line plates); Join asks which depot; its owner has a Head office button beside it.

### Step 2: the overview, the books and the treasury

The head office's page (`bco.js`'s `HqPage`; `bco_hq.rs`; the door's `hq` and `hq_do`:
`server_hq::view`, `server_hq::act`), its owner's (`server_hq::may_run`):

- **Overview**: the treasury, the whole group's money, this month's result (last month's under
  it) and the group's debt; every depot with its money, its month, its lines, buses and staff,
  whether it runs, its days in the red, the money on its way to it and its rule; the depots'
  requests for big purchases to approve or refuse; the head office's news (a depot in the red,
  bankrupt, an emergency loan, a request).
- **Money and books**: the group's books of this month and the last - the depots' months summed
  with the head office's (`server_hq::consolidated`; what moved between them is capital, no part
  of a result) - by kind; money sent to a depot; each depot's rule: what it keeps at most (the
  rest sent home at its day's close by its own process: `server_hq::sweep`) and below what the
  head office tops it up from the treasury (`server_hq::top_ups`: what its file says it has and
  what is on its way to it counted, never more than the treasury has; the top-up never over what
  it keeps); the head office's loans - the bank lends on the whole group (`server_hq::credit`:
  its buses, what it earns, less what all of it owes) - borrowed and paid back early; a depot
  founded from the treasury; the treasury's last bookings.
- Money is typed in the group's currency (`bco_hq.rs`: `MONEY`) and goes to the server in cents.

### Step 3: the fleet and the staff across the depots

The page's Fleet and staff: every depot's buses (owned, leased, in the workshop) and staff (free,
lent, to be lent); a bus of a depot's own moved to another depot, an employee lent to one. The
head office cannot touch a depot's file - its process keeps it - so it gives the depot an
**order** (`server_hq::order`, `<folder>/hq-orders/<depot>.jsonl`: `move_bus {bus, to}`, `lend
{employee, to, days}`, checked first: the depots its group's, not the same, the bus owned, the
employee not borrowed). The depot's process does it at its next step (`server_hq::take_orders`:
`server_depots::move_bus` at the server's price and days, `server_depots::lend`), told in its
inbox; what it can no longer do it tells the head office (`ORDER_FAILED`). The page shows the
buses on their way between the depots and the orders not taken yet.

### Step 4: the brand

The page's Brand: the group's name, short name and colours; a depot's mark - its logo picture,
chosen in its settings, its short name and colours - made the group's (`server_hq::mark_of_depot`);
a livery a depot made in the livery studio shared with the whole group
(`server_hq::share_livery`, its design not paid again), or no longer handed on. Each change is a
new version of the brand (`Group::brand`); every depot's process looks at its head office every
`BRAND_EVERY` seconds (`server_hq::take_group`) and takes a newer one (`take_brand`): named "<group>
<map>", the group's short name, colours and logo, the group's liveries beside its own - its buses
painted in them in its workshop. The page says which depots have not taken it yet.

### Step 5: the roles

The page's Roles, its owner's to give (`server_hq::set_member`, `Group::members`): the group's
**management** runs the head office with him (`server_hq::may_run`: the page, the server page's
Head office button) and is a co-owner of every depot; a **depot manager** is a co-owner of his
depot (one a depot). Each depot takes them at its look (`server_hq::take_roles`): the players
co-owners of it (`together::Together::guest_of`, as a player who joined), and who the group made
one and no longer does (`Online::group_roles`) a driver again. The requests for big purchases go
to the head office: its owner or its management answers them.

Tested: `server_hq::tests::the_first_depot_makes_a_group_with_its_head_office`,
`a_head_office_of_before_is_split_into_its_group`, `the_treasury_and_the_depots_rules`,
`the_groups_books_its_loans_and_its_days`, `the_owners_acts`,
`the_head_office_moves_buses_and_lends_staff_between_its_depots`,
`the_groups_brand_reaches_every_depot`, `the_groups_roles_reach_its_depots`; `server_depots`'
tests with the group's head office; the launcher's page: `bco_hq::tests::the_head_offices_page_from_the_servers_view`.

## Open questions (udkast 4)

The maps to start with; the price of a company and of a depot (`depot_price` until then); the
company's share (a cap); working for several companies; who may join the server. (The limit of a
big purchase and the bankruptcy of the whole company: decided 26 October 2026, phase 6.)

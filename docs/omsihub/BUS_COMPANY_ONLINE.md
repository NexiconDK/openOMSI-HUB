# Bus Company Online

Bus companies that live on a dedicated server: one shared world, many drivers, as in a LAN
session - everybody sees everybody and every bus. All the companies of the server live there
(never on a player's computer); the server runs around the clock on its own clock; the
companies' days, plans and books go on whether anybody is logged in or not, and their own buses
drive their timetables in the server's world. Players log in with their Steam ID, get a role
from the owner and drive their duties in the same world. The plan in Danish, with pictures, for the team: "Bus Company Multiplayer -
plan" (udkast 2, 24 October 2026; written for one company on a server, before all of them were).

## Decisions

| Topic | Decision |
|---|---|
| Where the companies live | All of them on the server; the game is the shared world as in LAN (decided 24 October: "you must STILL see each other and everything, as in LAN") |
| New companies | Always founded on the server, never brought along from a player's computer |
| The server's clock | The owner's choice in `server.cfg`: the real date and time (`real_time = 1`), or a date and time of his own the first time; then 1:1 with the real clock, going on where it was after a restart; every company runs on it |
| Lines | Each line belongs to at most one company on the server; the map's lines no company has run as the map has them |
| The world | Persistent; the day closes at midnight; saved as it runs, goes on after a restart, catches up what it missed |
| Tours nobody drives | The company's own buses (its liveries, numbers, plates) drive them as AI in the world |
| The roster | The easy mode's routine fills it; in Normal and Realistic the planners alone, what they leave open does not run; the map's lines no company has run as the map has them |
| Control room | Live, from the first playable version on |
| Identity | The Steam ID read on the player's computer (`Steam/config/loginusers.vdf`) and a secret key the server gives that computer; the owner approves new players and resets a key |
| Host | The server program on a PC or VPS, and a button in the launcher |
| Pay | A trip counts in the company's books and in the player's own career |
| The company's share | A percentage of the trip's income the company chooses (say 25 %); a change counts from the next day, the duty keeps the share it was taken at |
| Own company | 100 000 of one's own pay founds a company on the same server; lines go by tender (concessions); a later phase - the server holds several companies from the start |
| Project | A server program of its own built from Omsi-Hub's code, in modules of its own, behind a switch (off until ready); no new fork, no long-lived branch |

## Phases

1. **The companies move onto the server**: all of them in its folder, on its clock; days planned
   and closed; saved and resumed; each company's buses drive its lines' tours, the plans taken in
   as they change.
2. **Players come in**: "Bus Company Online" in the launcher (server list, code), Steam ID and
   key, roles and permissions (from "Play together"), the company's day: take, give, ask for and
   approve duties.
3. **Drive the duty in the shared world**: Drive starts the game joined to the server with the
   duty's bus and time; the AI bus taken off the tour; one player a tour; the duty strip and
   messages from the server; trips booked (the company's share, the driver's pay).
4. **Live control room**: every bus with its actual position and delay, messages to the drivers,
   interventions on the AI buses.
5. **Polish**: taking over a tour mid-route, trips measured by the server, the launcher's button,
   translations.
6. **Players found companies**: 100 000, several companies, lines by tender.

## Phase 1: what exists now

`omsi_launcher_lib::company::online::Host` (crates/omsi-launcher-core/src/company/online.rs) is
the company as the server keeps it:

- `Host::open(data, id, real, map_dir, clock)` loads the company from the server's folder, pins
  its course to the server's clock (`ServerClock`), catches up the time it was away (`clock::catch_up`: days closed, the
  dispatcher deciding), enters its day (`clock::enter_day`: the roster filled or repaired),
  writes the day's plan for the world (`plan::save_live_plan`, `plan_gen`) and saves it.
- `Host::tick(real)` tells what the world reported of the company's buses (`store::peek_live`,
  `clock::tell_live`), moves its time on to the real clock's (`clock::advance`, quick), closes
  its days at midnight, writes the new day's plan and saves every `SAVE_EVERY` seconds and after
  each close.
- `Host::now()` is the company's date and minute (the server's, once caught up).
- The map's timetables are read from the server's map folder (`store::Disk::on_map`,
  `lines_in_map_dir`), not through a launcher's OMSI folder, and kept between steps
  (`Disk::with_read` / `into_parts`).

Tested (`company::online::tests`): the real clock, days closed planned and saved, a restart
going on where it was without closing a day twice, the world's reports told once.

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

### Findings

- **The host runs on a thread of its own.** A midnight costs about a second (six from a cold
  disk): on the world's frame loop that stalls everybody's world. The world and the host meet
  through the plan file (`plan_gen`) and the live file, as the game and the launcher do today.
- **The roster is the planners'.** Decided (24 October): in the easy mode the company's own
  routine fills the roster on the server as on a player's computer (`autopilot::morning`); in
  Normal and Realistic it is the planners' alone - not even a driver ill or a bus broken is
  covered for them (`plan::repair_day` is not run: `Online::on_server`) - and what nobody
  filled does not run. The map's lines no company has run as the map has them.
- **Tours far from the players.** The timetable runs a tour wherever its lanes are loaded; a
  server loads the whole map by default (`radius = 0`), so every tour of the company drives in
  the world and reports its trips (measured). With a smaller radius the tours out of reach wait,
  and the day's close models them as it does for a company today. Both work.
- **What the launcher's pages do besides** (a line's change on its day, the dealer's deliveries,
  the control room's messages, the depot's building) still runs only there: to be moved to the
  host in this phase.

### In the server

`server.cfg` (the keys are not in its defaults yet - the feature is off until it is ready):

```
companies = 1                    # the bus companies live on this server
company_dir = company            # their folder, beside server.cfg unless absolute
```

A company is founded on the server (phase 2: from the launcher):

```
omsi-hub-game --server server.cfg --found-company "Ahlheim Bus" --company-mode normal
```

- **The server's clock** (`online::ServerClock`, `server_clock`, kept in `company_dir/clock.json`):
  set the first time from `date` and `time` (empty date: today), then 1:1 with the real clock and
  on where it was after a restart, the time the server was off included. With `real_time = 1` it
  takes the real date and time each time the server starts - forward only: when the clocks go
  back an hour in the autumn the server keeps its course (the companies cannot go back). The
  world runs on it (`company_server::clock`), the speed 1 and the administration's clock locked
  as with `real_time = 1`.
- **Every company on it**: `company_server::start` opens every company of the folder that is on
  the server's map (one of another map is left there), its course pinned to the server's clock
  (`Host::open`'s `clock`: one behind catches up, one ahead waits); the hosts run on a thread of
  their own (`company hosts`), a step every 2 s, the folder looked through every 30 s for
  companies founded since; each is saved when the server stops (`company_server::stop`).
- **Founded on the server** (`online::found_on_server`): its first day the server's date, its
  minute the server's, an id not taken yet.
- **The timetable** (`company_live::ServerCompanies`): every company with a plan of the map; each
  line to the company that has it (`LivePlan::lines`; two claiming one: the first by id, logged),
  its tours with its buses in its paint, its lines' other tours kept in their depots, the trips
  told to it; the map's other lines as ever. Whenever a host writes a plan, or a company comes on,
  the plans are taken in again for the same day (`ScheduleSim::reload_company`), the trips told
  already staying told.
- Midnight: the world's day rolls on its own; until a company's host has written the new day's
  plan (a second or a few), its lines stay in their depots.

Tested: `company::online::tests` (the server's clock on where it was and never back on the real
time; companies behind it caught up, ahead of it waiting; founded on it), `company_live::tests::
every_company_of_the_server_runs_its_own_lines`, `company_server::tests`, `server_cfg::tests::
the_companies_and_their_folder`.

### Still to do in phase 1

- What the launcher's pages do besides (a line's change on its day, the dealer's deliveries,
  the control room's messages, the depot's building) moved to the hosts.
- A line refused to a company when another of the server has it (today the world's guard only).
- A run on a real server with the whole map (Ahlheim), measured - with several companies.

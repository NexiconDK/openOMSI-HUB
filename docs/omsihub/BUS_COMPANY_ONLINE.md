# Bus Company Online

A bus company that lives on a dedicated server: one shared world, many drivers. The server runs
around the clock in real time; the company's days, plan and books go on whether anybody is
logged in or not, and the company's own buses drive its timetable in the server's world. Players
log in with their Steam ID, get a role from the owner and drive their duties in the same world,
as in a LAN session. The plan in Danish, with pictures, for the team: "Bus Company Multiplayer -
plan" (udkast 2, 24 October 2026).

## Decisions

| Topic | Decision |
|---|---|
| Where the company lives | On the server (the owner creates it there; it runs while he is away) |
| The world | Persistent, real time 1:1; the day closes at midnight; saved as it runs, goes on after a restart, catches up what it missed |
| Tours nobody drives | The company's own buses (its liveries, numbers, plates) drive them as AI in the world |
| Control room | Live, from the first playable version on |
| Identity | The Steam ID read on the player's computer (`Steam/config/loginusers.vdf`) and a secret key the server gives that computer; the owner approves new players and resets a key |
| Host | The server program on a PC or VPS, and a button in the launcher |
| Pay | A trip counts in the company's books and in the player's own career |
| The company's share | A percentage of the trip's income the company chooses (say 25 %); a change counts from the next day, the duty keeps the share it was taken at |
| Own company | 100 000 of one's own pay founds a company on the same server; lines go by tender (concessions); later phase, prepared from the start (several companies in the server's folder, a line to one company, the AI plan per company, pay to the player) |
| Project | A server program of its own built from Omsi-Hub's code, in modules of its own, behind a switch (off until ready); no new fork, no long-lived branch |

## Phases

1. **The company moves onto the server**: `server.cfg` names it; the server's clock is the
   company's; days planned and closed; saved and resumed; the company's buses drive its tours,
   their plan taken in as it changes.
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

- `Host::open(data, id, real, map_dir)` loads the company from the server's folder, turns its
  real-time running on, catches up the time it was away (`clock::catch_up`: days closed, the
  dispatcher deciding), enters its day (`clock::enter_day`: the roster filled or repaired),
  writes the day's plan for the world (`plan::save_live_plan`, `plan_gen`) and saves it.
- `Host::tick(real)` tells what the world reported of the company's buses (`store::peek_live`,
  `clock::tell_live`), moves its time on to the real clock's (`clock::advance`, quick), closes
  its days at midnight, writes the new day's plan and saves every `SAVE_EVERY` seconds and after
  each close.
- `Host::now()` is the date and minute the server's world runs on.
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
- **The roster must be filled.** A Normal or Realistic company whose roster nobody planned
  drops its tours: on the server, either the dispatcher fills what is left open each morning
  (as the easy mode's routine does, `autopilot::morning`), or planners do it. To decide.
- **Tours far from the players.** The timetable runs a tour wherever its lanes are loaded; a
  server loads the whole map by default (`radius = 0`), so every tour of the company drives in
  the world and reports its trips (measured). With a smaller radius the tours out of reach wait,
  and the day's close models them as it does for a company today. Both work.
- **What the launcher's pages do besides** (a line's change on its day, the dealer's deliveries,
  the control room's messages, the depot's building) still runs only there: to be moved to the
  host in this phase.

### Still to do in phase 1

- `server.cfg`: `company = <id>` and `company_dir` (default: the server's `company` folder).
- The host on its own thread in the server; its `now()` sets the world's date and clock; a new
  `plan_gen` makes the timetable take the plan in (`CompanyLive::set_day`), not only at midnight.
- `Schedule::set_company` with the server's company folder (today the launcher's data folder).
- Saved when the server stops.
- Creating the company on the server (phase 1: copied from a launcher; phase 2: from the
  launcher with the admin code).

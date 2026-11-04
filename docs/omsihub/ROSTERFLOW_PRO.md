# RosterFlow Pro

RosterFlow Pro is the third planning tool of a bus company, next to the standard planning and
RosterFlow. The approved plan ("Rosteren – plan") gives it five tabs: rosters, a calendar of
what every driver does, duties, asking drivers, and frames. It is built in steps. This page
describes step 1, "Ansættelse og rammer" (employment and frames), and step 2, the tabs Vognløb
and Vagter, both done on 4 November 2026 (NexiconDK).

## The planning tool

A company chooses its planning tool in the company's settings, or in the segmented choice at
the left of the planning page's toolbar:

| Choice | What it is |
|---|---|
| Standard | The day's roster, filled by tapping or dragging. |
| RosterFlow | The scheduling studio, timeplan and roster. |
| RosterFlow Pro | For now the same as RosterFlow. Its own tabs come in later steps. |

The choice is kept as the add-on switches (`Addons::rosterflow`, `Addons::pro`), so an older
file plans as it did. Casual has the standard planning only. RosterFlow's own pages have no
copy of the choice yet: to go back to Standard, use the company's settings.

Code: `model::Planner`, `autopilot::planner` / `set_planner`, the act `planner`
(`dialogs::planner_json`).

## How a driver is employed

Every driver has one of four kinds of employment (`model::Employment`). An older file's
drivers are full time. The hiring dialog asks for the kind when a driver is hired.

| Kind | Guarantee | A day is paid | Overtime above |
|---|---|---|---|
| Full time | The country's full week: 37 hours at the Danish price level, else 39. A seventh of it for each day the driver is there. | The duties, from signing on to signing off, at least the duty floor (4 hours). | The full week |
| Part time (e.g. 25 h) | Its own week, pro rata. | As full time. | Its own week |
| Reserve with a frame (e.g. 05:00–13:00, Monday to Friday) | None beyond the frame. | On a frame day: the whole frame, whether used or not, plus work outside it. On other days: the duties. | The frame's week |
| Hourly paid | None. | Only the hours worked, at least 3 hours on a day called in (no duty floor), at 10 % more an hour. | The full week |

The 37-hour week applies when the company plays at the Danish price level: a server's
`price_level = dk`, or a company in Denmark (`econ_uk::contract_week_hours`). A wage stays the
reference week's, so an hour costs the same and the week guarantees fewer hours.

**The bus company's week begins on Sunday.** The contract week (the guarantee, the overtime
and "hours this week") runs Sunday to Saturday for every company, and the guarantee is settled
on Saturday night (`pay::week_ends`). The drivers' five working days (`staff::WEEK_DAYS`) still
count from Monday. In the first week after updating, a save's pay week runs Monday to Saturday
once.

**Changing a driver's kind takes a month's notice** (30 days, `staff::change_employment`). The
old kind holds until the new one applies as the company's day begins
(`staff::employment_due`). On the day a driver was hired, a change applies at once. Choosing
the kind the driver already has takes back a change given notice.

## The order an open duty is filled in

The dispatcher fills a duty without a driver in this order (`plan::fill_tier`):

1. A reserve whose frame holds the duty. He is not asked: the frame is the company's.
2. A free colleague, the standby drivers first (a reserve outside his frame counts as a
   colleague).
3. An hourly-paid driver.
4. An agency driver booked for the day.
5. An agency driver sent on the day itself.

This order holds for the dispatcher's own filling, the holiday cover, the central's cover of a
duty that fell out today, the repair of the roster, and the lists of who could take a duty
(`candidates`, `covers_of_duty`, `late_covers`). The working-time rules (rest, overlap, night's
rest, the longest day) still hold for each. For a driver who comes late, a free reserve within
his frame drives the first trips before a spare line. A company without reserves uses its spare
lines as before.

## A reserve is told of a duty given late

A reserve who is given a duty from 12:00 the day before is sent a message to his driver's
screen: the duty, its day and his sign-on time (`plan::tell_reserves`, the template
`messages::RESERVE_DUTY`). A duty he already had before noon the day before is not told, and
each duty is told once. A duty taken from him and given again is told again. The control
room's message link checks today's and tomorrow's plans every two seconds.

## On the pages

- **Staff page:** each driver's card shows the kind, the contract hours or the frame, and the
  hours this week. When a company has reserves or hourly-paid drivers, the list is grouped:
  "Permanent drivers" (full and part time), "Reserves (frame)", "Hourly paid", then the other
  staff, each with its count. A driver's page shows the employment, a change waiting for its
  day, and "Change the employment".
- **Hiring:** "Hire" on a driver opens the choice of kind, with the hours of a part-time week
  or the frame's times and weekdays.
- **Planning, the day panel:** "Frames this day" lists the reserves whose frame holds that day,
  with what of it is used, and "Free all day" lists who has no duty.

## Not in step 1

Players of "Play together" as reserves, the drivers' answers ("too early", "too late"), sign-on
times and driver numbers in the calendar, and the cost of overtime when asking a full-time
driver come in later steps.

# Step 2: the tabs Vognløb and Vagter (4 November 2026)

With RosterFlow Pro chosen, RosterFlow's title bar has the Pro tabs after Routes and Timeplan:
"1 Vognløb" (Blocks) and "2 Vagter" (Duties), and "3 Turnus", "4 Daglig drift" and
"5 Kalender" greyed out until their steps come. Vehicles, Drivers and Roster stay. RosterFlow
opens on "1 Vognløb" when Pro is chosen. Underneath it is RosterFlow's own day: the studio's
blocks and pieces, joined duties for duties of several pieces, the draft, Undo, Redo, Save and
Discard. Code: `co::pro_duties` (the core), `omsihub/rosterflow/pro.rs` (what the page gets and
the acts `pro_*`), `launcher/ui/js/omsihub/rosterflow/pro.js` (the page).

## 1 Vognløb (Blocks)

A read-only view of the day's buses, one row per block: its line, its name, when it pulls out
and pulls in, and its trips on a time axis in the line's colour (an arrow for the direction; a
run without passengers hatched). A red mark is a relief point where the driver may be changed
(`studio::can_cut`: the depot, the termini and the relief points of Preferences › Relief Points),
a red line is a change already made. On the left: the lines with their number of blocks, and the
relief points with the lines that stop there. A block clicked says its times, trips, kilometres,
pieces and how often and where a relief is possible. "Cut into duties" opens Vagter, "Move
trips" RosterFlow's Vehicles tab.

## 2 Vagter (Duties)

Top: every block cut into its pieces (A, B, C ...), each with its duty's number or dashed as
"no duty". A red mark clicked cuts the block there (`pro_duties::cut_to_pool`): the piece after
the cut goes to the pool. A red line clicked joins the two pieces again (`studio::uncut`).

Left: **the pool**, the pieces without a duty. A piece is dragged onto a duty, or chosen and then
"Put in duty" (`add_to_duty`): next to a piece of the duty on the same bus the cut goes, else the
duty gets a piece of another bus - often another line (a line change). Only pieces at the same
time are refused; everything else the rules say is marked on the duty, not refused. "New duty"
makes the piece a duty of its own (`new_duty`). A piece in the pool still runs: the day's roster
has it as a short duty of its own, numbered from 1501 (2501, 3501), so no trip is dropped
because a duty is not built yet.

Bottom: **the duties**, one row each: number, type, sign-on, sign-off, work and paid time, the
pieces on the time axis with the breaks (P) and walks (→) between them, and a mark: ✓ all rules
kept, a yellow ! a warning, a red ! a rule broken (the reasons on hover and on the card of the
duty chosen). The card says the duty in words: "Signs on 05:30 · line 7 vehicle 7/02 piece A to
Hospital 08:55 · walks 10 min to Station · off duty 7:05 · line 5 vehicle 5/01 piece C to Bytorvet
16:50 · signs off 16:55 at Bytorvet". "Take piece out" puts the piece chosen back in the pool
(`to_pool`), "Take duty apart" every piece of the duty (`dissolve`). "Make duties automatically"
is RosterFlow's Optimise of the duties alone, the buses' blocks kept (`auto`): every piece in a
duty, the pool emptied.

| Type | When |
|---|---|
| Hel (whole) | One stretch, or pieces with breaks under 1 h 30 between |
| Delt (split) | A gap of 1 h 30 or more between two pieces (unpaid) |
| Kort (short) | Under 4 hours of work; paid 4 hours (`staff::MIN_PAID`) |

The marks are RosterFlow's issues of the duty (the working-time rules: driving without a break,
the meal break, the spread, rest, the duty types) and Pro's own (`pro_duties::marks`): a split
duty spread over 12 h 30, and too little time to change from one piece to the next - 5 minutes
at the same stop, 10 minutes to walk to another stop (a relief point's own travel time when it
is longer). The four times on the left - signing on (10 min), signing off (5 min), changing at
the same stop and walking to another - are the company's preferences (`Prefs::change`, the act
`pro_rules`).

## The duty numbers

A duty keeps its number while the day is edited (`Sheet::numbers`, `pro_duties::renumber` after
every edit of RosterFlow's with Pro chosen): the duty with the same first piece keeps it, else
the duty its first piece was in, else one it shares a piece with; a new duty takes the lowest
number free. Monday to Friday count from 1001, Saturday from 2001, Sunday from 3001 (a plan of
single dates by its first date's weekday). A plan copied to another day is numbered anew there.
The roster, the duty card and the PDF show the same numbers.

## The fall-back

A day nothing was built on with Pro runs RosterFlow's own duties (`studio::duties`: every piece
a duty, the joined ones together), numbered in their rows from the day type's first number -
the same duties as before Pro. An older save loads as it was: the pool, the numbers and the
change times are new fields with defaults, left out of the file when empty. Without RosterFlow
Pro nothing of this applies: the numbers are RosterFlow's 1001 on.

## Not in step 2

The rota (Turnus), the daily operations, the calendar and asking drivers for a duty come in the
next steps.

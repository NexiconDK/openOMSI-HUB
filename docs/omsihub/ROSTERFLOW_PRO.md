# RosterFlow Pro

RosterFlow Pro is the third planning tool of a bus company, next to the standard planning and
RosterFlow. The approved plan ("Rosteren – plan") gives it five tabs as HASTUS has them: the
blocks (Vognløb), the duties (Vagter), the rota (Turnus), the daily operations (Daglig drift) and
a calendar of what every driver does (Kalender). It was built in four steps, all done on 4
November 2026 (NexiconDK): step 1 "Ansættelse og rammer" (employment and frames), step 2 the tabs
Vognløb and Vagter, step 3 the tab Turnus, step 4 the tabs Daglig drift and Kalender.

## The planning tool

A company chooses its planning tool in the company's settings, or in the segmented choice at
the left of the planning page's toolbar:

| Choice | What it is |
|---|---|
| Standard | The day's roster, filled by tapping or dragging. |
| RosterFlow | The scheduling studio, timeplan and roster. |
| RosterFlow Pro | RosterFlow with the five Pro tabs: Vognløb, Vagter, Turnus, Daglig drift, Kalender. |

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


# Step 3: the tab Turnus (4 November 2026)

With RosterFlow Pro chosen, "3 Turnus" (Rota) is the third Pro tab of RosterFlow's title bar.
A rota is who drives the duties of the Vagter tab week after week, as a Danish bus company hangs
it up: rows of a week each, a driver on every row, every driver a row down each week. Code:
`co::pro_turnus` (the core), `omsihub/rosterflow/turnus.rs` (the commands `rf_turnus` and
`rf_turnus_act`, and the print), `launcher/ui/js/omsihub/rosterflow/turnus.js` (the page).

## A rota

A company has as many rotas as it likes ("Nord fuldtid", "Nord deltid", "Reserve"), kept in its
planning (`Planning::rotas`) and saved at once - not in RosterFlow's draft, and so without its
Undo: a rota is not a day's blocks but the drivers' weeks.

- **A row is a week, Sunday to Saturday** (the bus company's week begins on Sunday, as the pay's
  week of step 1). Each of its days is a duty of the Vagter tab by its number (1001 on a weekday,
  2001 on a Saturday, 3001 on a Sunday), a day off ("Off"), or a frame ("Reserve 05:00-13:00": at
  the company's disposal within it, `pro_turnus::Cell`).
- **A driver is put on a row**, and every week he moves a row down - from the last row to the
  first again. The cycle is as many weeks as the rota has rows. The rota begins on a Sunday (the
  company's week when it is made; "Begin it in the week shown" moves it): the driver on row 1 is on
  row 1 that week (`row_on`, `driver_on`). Before it begins it decides nothing.
- **Full- and part-time drivers** go in a rota, **reserves** (`Employment::Reserve`) in a reserve
  rota, **the hourly paid** in none - they are called in. A driver is on one row of one rota.
- A duty is on one row a day among all the rotas: a day of a row takes only a duty that runs that
  weekday (`set_cell`).

## Made by hand or automatically

A day clicked shows that weekday's duties - those no rota has first, those in another row greyed
with their place on hover -, a day off and the reserve frames. A driver's place clicked shows the
drivers who can go on the row. "New row" and "Delete row" add and remove rows.

"Make the rota automatically" (`pro_turnus::auto`) spreads the duties of the week no other rota
has over as many rows as they need: the cycle's average near the hours aimed at, at most 5 days of
work a row, and as many rows as the busiest day has duties. Each row has two days off together,
begun at most a day after the row before's ended - so that there are never more than 6 days of
work in a row, round the cycle -, where the week has the most rows to spare (the weekend, mostly),
found by a search (`search`; a row at a time where it finds none); when no such days off leave
every day enough rows at work, it takes a row more. A day with more rows at work than duties gives
the rows with the most hours so far a day off more. Monday to Saturday each day's duties go to the
rows by their times - the row that finished early the day before takes an early one -, Sunday last
between the row before's Saturday and the row's own Monday; then duties of a day, or of two or
three days running, are swapped between two rows where that keeps the rest and evens the hours. A reserve rota gets a row for each reserve driver, five days a week in his own frame (early
05:00-13:00 and late 13:00-21:00 else). The drivers stay on their rows.

## The rules over the whole cycle

`pro_turnus::check` checks the rota round its whole cycle - a row's Saturday is followed by the
next row's Sunday, the last row's by the first's. The marks show (red or yellow), they do not
refuse:

| Rule | Mark |
|---|---|
| 11 hours of rest between two duties | red, on the day after the short rest |
| At most 6 days of work in a row | red, on the 7th |
| Two days off together in each row (its Saturday and the next row's Sunday count) | yellow |
| A row's week within 20 % of the hours aimed at (the contract week - 37 h at the Danish price level, else 39 - or the rota's own, "Hours a row aims at"): a week may be a day longer or shorter | yellow |
| The cycle's average within 5 % of the hours aimed at - what counts over the cycle | yellow |
| A driver's contract within 5 % of the rota's average | yellow |
| A weekend day off somewhere in the cycle | yellow |
| A duty that runs that day, on one row only | red |

## A driver's next weeks and the exceptions

A driver clicked shows his next four weeks as the rota has them. A day of them clicked can be
changed for that date only, the rota as it is (`Exception`, `set_exception`): a day off, on
holiday, ill, or another duty of that weekday ("Swapped" - the swap's other driver is changed the
same way). "As in the rota" takes it back. Exceptions of days gone are dropped.

## The day runs it

With RosterFlow Pro chosen, the day's plan (`plan::day_plan`, `pro_turnus::on_day`) gives a driver
on a row of a rota begun by then the duty of his row that day - or of his exception -, and nothing
else of the weekly roster: his day off is off. An exception "ill" or "on holiday" leaves his row's
duty open as ill or on holiday, for the central and the dispatcher's order of step 1. A driver
without a row, and a duty no row has, run as the weekly roster has them (the fall-back). Without
RosterFlow Pro no rota decides anything.

A frame of a driver's row makes him a reserve within it that day (`pro_turnus::frame_on`): the
first to cover an open duty in it (`plan::fill_tier`), listed in the day panel's "Frames this day"
(`plan::frames_on`), and paid the frame whether used or not, the work outside it on top
(`pay::day`). A reserve whose rota gives him no frame that day is paid the duties he drives.

## On the page

- **Left:** the rotas (name, rows, drivers, hours a week), "New rota" and "New reserve rota"; the
  rules the chosen rota keeps over its whole cycle; its name, the hours a row aims at, when it
  begins, its average and cycle, "Delete the rota"; the duties' colours.
- **The rota:** a row a week - its number, each day's duty with its times (early before 09:00
  yellow, day blue, late purple, split teal, reserve green), its hours, its marks (✓ or !, the
  reasons on hover) and the driver on it in the week shown. The week shown is chosen in the
  toolbar (‹ Week 46 ›).
- **Under it:** a driver's next weeks and their exceptions; the coverage - how many of the week's
  duties are in a rota, and those that are not, by day.
- **"Print rota"** (`rf_print` with `turnus`): the rota as a PDF file on A4 across - the rows, each
  day's duty and times, the hours and the driver of the week shown -, saved where the player
  chooses, as RosterFlow's graphs are.
- A rota is of the saved duties, as the days run them: when RosterFlow has changes not saved, the
  tab says so.
- A player of "Play together" with "Edit the planning" may edit the rotas (`rf_turnus_act`), who
  may look at the planning may look at them (`rf_turnus`).

## Not in step 3

An exception "ill" or "on holiday" is the rota's: it does not make the driver ill or give him a
holiday in the staff's records (the daily operations of step 4 count him away that day all the
same).

# Step 4: the tabs Daglig drift and Kalender (4 November 2026)

With RosterFlow Pro chosen, "4 Daglig drift" (Daily operations) and "5 Kalender" (Calendar) are
the last two Pro tabs of RosterFlow's title bar. Code: `co::pro_ops` (the core),
`omsihub/rosterflow/ops.rs` (the commands `rf_cal`, `rf_ops` and `rf_ops_act`),
`launcher/ui/js/omsihub/rosterflow/ops.js` (the page). What they keep - the duties given for a
date and the drivers asked - is the company's planning's (`Planning::ops`), saved at once, not in
RosterFlow's draft; an older file has none, and days gone are dropped every night.

## 5 Kalender (Calendar)

A week from Sunday to Saturday - or 14 days, or a month (five weeks) - from the company's week
on ("‹ This week ›"). One row per driver, grouped permanent (full and part time), reserves with
a frame, hourly paid (filter: all, permanent, frame, hourly):

- **The driver:** his name and his number ("chf. 112": his number in the company, which never
  changes), the chip Fast, Ramme or Timeløn, and his week ("37 t"), his frame ("ramme
  05:00–13:00") or "timeløn".
- **Each day:** the duty he drives (its number of the Vagter tab) and when he signs on ("møde
  05:05"; its whole time on hover), coloured as the rota colours it (early, day, late, split);
  "fri", "Syg", "Ferie" (his own illness or holiday, or his rota's exception), his frame when he
  has no duty in it (an "R" in the corner when his duty lies in his frame), or "–" before he was
  employed. A day already gone is greyed.
- **Hours this week** on the right ("31,8 t af 37"): the minutes of his week as the pay reckons
  them (`pay::planned_minutes`: signing on and off, the bus's checks, the duty's floor, a
  reserve's frame paid whether used or not, an hourly-paid driver's three hours, nothing for a day
  ill or on holiday). The bus company's week begins on Sunday: in the company's own week the days
  gone are what he was paid so far (`Employee::week_paid`), the rest as planned
  (`pro_ops::week_minutes`). Over his week it shows orange. In 14 days or a month, one figure per
  week.
- **Duties without a driver**, the top row: how many of each day's duties have no driver ("2
  mangler"), or a tick. A day clicked - its heading, its count or a cell - opens its daily
  operations.

A date outside the company's week (from the week after next, or the days of this week gone) is
planned with the timetable of its weekday in the company's week, with that date's rotas,
exceptions, duties given, illnesses and holidays (`ops::plan_on`).

## 4 Daglig drift (Daily operations)

The daily operations of a date (the company's day, or a day chosen in the calendar, "‹ Today ›"):

- **Left:** the day's duties without a driver - their number, times, line and sign-on, and why
  (ill, on holiday, no driver planned, partly: some of its pieces have one) and how many were
  asked -, the frames of the day (who, the frame, what of it is used) and who is free all day.
- **The duty chosen:** every driver in the order an open duty is filled in (`plan::fill_tier`): a
  reserve within his frame, the colleagues (the standby first), the hourly paid, an agency driver
  booked for the day; then those who cannot, greyed with why (ill, on holiday, another duty then,
  too little rest to his other duty, his night's rest, his day too long, no licence). For each
  who can: where he comes in the order, whether he is free all day or only then, his hours this
  week, an hourly-paid driver's own hours ("kan oftest om eftermiddagen"), and **what he costs**
  (`pro_ops::price_of`, the wages without the on-costs):

| Driver | Costs |
|---|---|
| A reserve within his frame | Nothing more ("i hans ramme") |
| A permanent driver within his week | Nothing more: the week's guarantee pays it anyway |
| A permanent driver over his week | The overtime, orange: "overtid 2 t 40 × 1,5 = 1.070 kr." (`econ_uk::OVERTIME_RATE`; plain hours in Casual) |
| An hourly-paid driver | His hours at 10 % more, at least 3 hours on a day he is called in: "timeløn 4 t = 1.078 kr." |
| A reserve outside his frame | His hours on top |
| An agency driver booked for the day | The agency's price for the hours |

  Under the list: what an agency driver sent on the day would cost.
- **Right:** the answers a driver can give, the reserve's rule of noon the day before, and what
  each kind of driver costs.

## Asking a driver

The drivers ticked are asked ("Spørg 3"). An AI driver's answer is decided when he is asked
(`pro_ops::ai_reply`: the same for the same company, day, duty and driver) and comes in after a
few seconds ("tænker over det…", the page asks again every two seconds):

| Answer | When |
|---|---|
| "Ja, den ligger i min ramme." | A reserve within his frame: the frame is the company's |
| "Jeg er stadig syg", "Jeg har ferie der", "Jeg må ikke køre den bus", "Så bliver min dag for lang" | Ill, on holiday, no licence, a working day over the longest |
| "Det er for tidligt for mig, jeg kan først fra 08:30." | His rest to the duty before (or his night's rest) is too short: from when he could; an hourly-paid driver's own hours begin later; a duty before 05:30 is sometimes too early for anyone |
| "Det er for sent for mig, jeg kan til 13:00." | His rest to the duty after is too short; his own hours end earlier; a duty after 23:00 is sometimes too late |
| "Jeg kan godt, hvis Hanne tager min 1003." | He drives another duty then, and a colleague free all day could take it: a swap (else "Jeg kører selv 1003 på det tidspunkt") |
| "Ja, det passer fint." / "Jeg har haft en lang uge, kan vi tage det en anden gang?" / "Beklager, jeg har planer den dag." | He can: yes or no by how content (`satisfaction`) and how stressed he is, his hours this week against his contract (over it: tired, mostly no), the hourly paid glad of the hours |

Each answer has a few ways to say it, so that it sounds human. "Giv ham vagten" gives the duty to
one who said yes (with its price on the button): it is his on that date (`pro_ops::give`, a
`Call`), whoever's row or roster had it - the day's plan runs it (`pro_turnus::on_day`, before the
rows). A swap gives his own duty to the colleague too. "Too early" and "too late" are recorded:
"Del vagten" opens the Vagter tab, to cut the duty at a relief point and ask for each piece.
"Tag tilbage" takes a duty given back: it runs as its row or the roster has it again. A reserve
(`Employment::Reserve`) given a duty from 12:00 the day before is told at once, as in step 1
(`plan::tell_reserves`).

**Players** of "Play together" (and Bus Company Online) who may drive are on the list for a duty
of the company's day. Asked, a player is told (`crew::notify`) and answers on his own page
(`co_crew` `pro_answer`: yes, no, too early from a time, too late until one, or his own words);
"Giv ham vagten" makes it his in his own game (`together::book::give`, the assigner's permission).

Asking, giving and taking back need "Edit the planning" for a player of "Play together"
(`rf_ops_act`); who may look at the planning sees both tabs.

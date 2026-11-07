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
| Reserve with a frame (e.g. RAM1 03:00–15:00, Monday to Friday; round L) | Its contract week (full time or fewer hours), as a permanent driver's - not the frame's window. | On a frame day: the hours driven, at least 7:24 (`pay::frame_day_minutes`). On other days: the duties. | Its contract week |
| Hourly paid | None. | Only the hours worked, at least 3 hours on a day called in (no duty floor), at 10 % more an hour. | The full week |

The 37-hour week applies when the company plays at the Danish price level: a server's
`price_level = dk`, or a company in Denmark (`econ_uk::contract_week_hours`). A wage stays the
reference week's, so an hour costs the same and the week guarantees fewer hours.

**The bus company's week begins on Sunday.** The contract week (the guarantee, the overtime
and "hours this week") runs Sunday to Saturday for every company, and the guarantee is settled
on Saturday night (`pay::week_ends`). The drivers' five working days (`staff::WEEK_DAYS`) still
count from Monday. In the first week after updating, a save's pay week runs Monday to Saturday
once.

**When a change of a driver's kind applies** (`staff::change_employment`, `employment_from`;
it was a month's notice first, changed after the player's comments the same day - a month is
longer than most contracts of Bus Company Online):

- **From hourly paid up to part or full time:** from the next company day, everywhere (a better
  contract for the driver).
- **On a server of Bus Company Online:** after the notice its admin sets,
  `employment_notice_days` in `company-server.cfg` (the admin panel's "Driving yourself" part;
  default 2 days, 0: from the next company day - `live_pay::LiveCfg::notice_days`; the setting is
  added to an older file, `live_pay::NOTICE_CFG`).
- **Anywhere else** (offline, Play together): from the next Sunday, when the bus company's week
  begins - a change made on a Sunday from the Sunday after (`staff::notice_day`).

The old kind holds until the new one applies as the company's day begins
(`staff::employment_due`). On the day a driver was hired, a change applies at once. Choosing
the kind the driver already has takes back a change given notice. A change given a month's notice
in an older file still applies on its day. The driver's page says when a change made now would
apply, e.g. "A change applies from the next Sunday ...: from Sun 8 Nov".

**The player hires hourly paid** (`staff::HIRED_AS`): the Staff page's hiring window starts on
"Hourly paid", and the player picks Full time, Part time or Reserve himself. A hire without a
kind is hourly paid. The game's own hiring - the autopilot, an AI company, Casual's quick hire
(`staff::hire`, `hire_best`) - stays full time, so AI companies run as before. Hiring and a change
of employment stay on the Staff page; RosterFlow Pro's tabs have none of it.

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
(`plan::frames_on`), and paid the hours driven, at least 7:24 (round L: every frame, named or of
its own times - `pay::frame_day_minutes`). A reserve whose rota gives him no frame that day is paid the duties he drives.

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
  frame day's hours driven, at least 7:24, an hourly-paid driver's three hours, nothing for a day
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
  week, an hourly-paid driver's own hours ("kan oftest om eftermiddagen"; round F: drawn for the
  company, the driver and the date - `pro_ops::own_hours`, `OWN_DAYS`: all day 05-24 35 %, the
  morning 05-14 20 %, the afternoon 11-20 20 %, the evening 14-24 15 %, not that day 10 % - "Jeg kan
  ikke den dag, desværre." - until the drivers say themselves when they can), and **what he costs**
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

# After the player's comments (4 November 2026)

The player tried the four steps and found them confusing. Changed:

## Only the five tabs

With RosterFlow Pro chosen, RosterFlow's title bar has only "1 Vognløb", "2 Vagter", "3 Turnus",
"4 Daglig drift" and "5 Kalender" - no Routes, Timeplan, Vehicles, Drivers or Roster, and no
"Go to" search or Optimise settings (they lead to the hidden tabs). RosterFlow Pro always opens on
one of its tabs (a hand-over to an old one goes to Vognløb, the duty launcher's to Vagter); a line's
timetable is not taken by RosterFlow then - it opens in the line editor (`index.js` `rfTakes`).
RosterFlow's tutorial walks the hidden tabs, so RosterFlow Pro's "?" opens its own short intro,
"How RosterFlow Pro works" (`pro_intro.js`): the five tabs in order, with what is done in each,
and that hiring and employment are on the Staff page. It opens by itself the first time RosterFlow
Pro is opened on a machine and when the planner was just chosen. Plain RosterFlow is unchanged.

## A rota is several weeks

The page says so: a row is a **rota week** ("Turnusuge" 1 ... N), "Add week" and "Remove week",
"12 weeks · 12 drivers", and a line over the rota: "A rota is several weeks in a row. Every week each
driver moves a week down, and after the last week he begins again at week 1." **"New rota"** opens a
window: the name, the kind and the weeks - suggested from the week's duties no rota has
(`pro_turnus::suggest_weeks`) -, made empty or made and filled at once (`rf_turnus_act` `new` with
`kind`, `weeks`, `fill`).

**The weekend.** The week begins on Sunday, so a weekend is the Saturday of one rota week and the
Sunday of the next - the last week's Saturday with the first week's Sunday, round the cycle
(`pro_turnus::weekend`). The Saturday and Sunday cells are marked as weekend, and a column after
Saturday, "Weekend", says "Off duty" (Fri), "Working" (Arbejde) or "Half" for the weekend after
each week. "Two days off together" says that a Saturday and the next week's Sunday count.

## Three kinds of rota (`pro_turnus::RotaKind`)

| Kind | Holds | Its weekends |
|---|---|---|
| Ordinary (Almindelig) | the whole and short duties of the week - no split duty | every other weekend off |
| Split duties (Delte vagter) | the split duties of Monday to Friday only | always off: Saturday and Sunday cannot be set |
| Reserve | reserve drivers' frames | as before |

An older rota is ordinary (or reserve); a split-duty rota is kept as `"split": true`.

**Every other weekend** (an ordinary rota, `Rule::EveryOther`): round the whole cycle the weekends
take turns, off and at work. A free weekend is both days off (the week's two days off together); a
worked weekend both days at work. A half weekend, or two weekends off or at work in a row, is red;
an odd number of weeks, where they cannot take turns, is yellow with "add a week". "Make the rota
automatically" makes this: an even number of weeks, enough that half the drivers cover the
weekend's duties (`normal_weeks`), a weekday off besides in each week - placed so that there are
never more than six days of work in a row -, the weekend duties first to the weeks bound on both
sides; a weekend with more drivers at work than duties gives the others a reserve's frame, so
their weekend stays whole. The duties it cannot place are listed under "Not in a rota".

**Split duties** have a rota of their own (the player's choice): a split-duty rota takes split
duties only, Monday to Friday, its weekends off; made automatically it fills as many weeks as the
busiest weekday has split duties, five days each. An ordinary rota does not offer split duties and
its automatic making leaves them out; the coverage card says how many split duties belong in a
split-duty rota, with "Make a split-duty rota" when there is none. **Split duties do not run at the
weekend:** "Make duties automatically" on a Saturday or a Sunday makes none
(`rosterflow::optimise_splits`), and one made by hand there gets a yellow mark "Split duties do not
run at the weekend" (`pro_duties::marks`).

# The calendar made clearer, and the rules of asking (4 November 2026)

The player found the calendar and the box beside it cluttered, and drew how it should be
(sketches h13-h21 of the plan "Rosteren – plan"). Changed:

## The calendar
Big cells: the duty's number and under it its working time ("04:45–12:55"; a split duty when its
halves begin, "06:15 / 14:30"); a reserve driving a duty given him for another shows it ("05:00 ·
for Ahmed"). A day off is an empty cell with "–"; ill or on holiday is **one bar across the days
in a row** ("Ferie mandag – torsdag"). The colours mean the kind of duty only - early, day, late,
split, frame, ill, holiday - with a legend under the grid. The drivers are grouped under
"Permanent drivers · N", "Reserves with a frame · N" and "Hourly paid · N"; under the name "chf.
N · fuldtid 37 t" (no column of its own). Today's heading is in the accent colour; the hours of
the week have a bar against the contract, orange over it. A search field finds a driver. The row
**"Mangler en chauffør"** above the grid says per day how many duties have no driver ("3 vagter
mangler", "1 vagt mangler") or "✓ alle besat"; a day clicked is outlined and opens the box.

## The box, one step at a time
Beside the calendar (and as the page of "4 Daglig drift"), one step at a time in the same place:

1. **The day's duties without a driver** - "1002  05:45–13:55", under it "Linje 2 · Ole Friis
   har ferie" - each with "Find chauffør ›". A duty cut for the date is one of them ("klippet i
   2 dele").
2. **Who can drive the duty:** "‹ Tilbage", "Vagt 1002", "Torsdag 05:45–13:55 · linje 2", then one
   list: **the permanent drivers first, then the reserves, then the hourly paid** (small labels
   FASTE, RAMME, TIMELØNNEDE), the cheapest first in each - only the order on screen; the
   dispatcher's own fill order (`plan::fill_tier`) is unchanged. A row is the name, under it the
   kind and the price ("Fast · overtid 2 t 40 = 1.070 kr.", "Ramme · ingen ekstra", "Timeløn ·
   871 kr."), and **one** thing on the right: "Sæt på", "Spørg", "Venter på svar …" with "Træk
   tilbage", or "Giv vagten" after a yes (gender-neutral: "Giv vagten, med byttet" for a swap).
   The driver's answer is a **box of its own** under his row: green "Ja" with his words, red
   "Nej", "For tidligt", "For sent" with the time he can, grey "Intet svar". Those who cannot
   are folded: "▸ 5 andre kan ikke køre den".

## The rules of asking (`pro_ops::Rule`, `rule_of`)

| Driver | How |
|---|---|
| A reserve whose frame holds the duty, before 12:00 the day before (company clock) | "Sæt på": no question, he is told (`plan::tell_reserves`). At 12:00 exactly or later he must be asked. |
| Anyone else: the hourly paid, a permanent driver (a day off or overtime), a reserve from 12:00 the day before | Always asked (`Rule::Ask`); **several at once**, ticked and asked with "Spørg de valgte" - round G took away the old rule of one permanent driver at a time. The duty goes to one who said yes; the others who said yes or have not answered yet get the message "Tak - vagt %{n} den %{date} er givet til en kollega" (`messages::DUTY_TAKEN`), nothing off their content; a no needs no word. "✕" (Træk tilbage) closes a question (`withdraw_ask`). |

"Giv vagten" works only after a yes (`give_asked`, `NEEDS_YES`), or for a reserve put on.

## The answer comes in game time (`pro_ops::answer_time`)
Normally 5-30 minutes on the company's clock; 2-10 when the duty begins within 3 hours. Nobody
answers between 23:00 and 06:00: asked at night he answers after 06:00 (within 20 minutes), or
cannot be reached when the duty begins first. An hourly-paid driver short of hours (under half a
full week) answers quicker (× 0.6), a tired or stressed one (stress over 60, contentment under 35,
or over his contract) slower (× 1.6) and sometimes not at all (15 %; others 3 %): after 2 hours
the box says "Intet svar" (`Reply::Silent`, `NO_ANSWER_AFTER`). The same question always gets the
same answer and time (`Rng` of the company, the day, the duty and the driver). A player answers
himself. Questions keep their time in the save (`Ask::due`); an older question by the launcher's
seconds.

## "Klip vagten": cutting a duty for its date (`co::pro_cut`)
When nobody can or will take the whole duty (nobody left to ask, all said no, too early, too late
or did not answer), the box shows a yellow note "Ingen kan køre hele vagten. Klip den i stykker,
så så meget som muligt bliver kørt." and **"Klip vagten ›"**. It cuts the duty at its relief
points (inside a piece where the Vagter tab could cut it, `studio::can_cut`, and between its
pieces) into **at most 4 parts** (`MAX_PARTS`; at most 12 places tried), choosing the cut that
covers most, then the fewest parts, then the cheapest (`propose`). Each part gets one suggestion
(`suggest`, one driver a part), best first:

1. a driver whose own duty **ends** at or just before it (at most 45 minutes) at the same or a
   **nearby** stop - named alike or with the same first word ("Bytorvet" and "Bytorvet Nord"; the
   map has no places to measure) - who goes on with a little overtime ("fortsætter efter 1001 ·
   overtid 1 t = 335 kr.", "1001 slutter 12:55 på Bytorvet"), or one whose own duty
   **begins** just after it and who begins earlier;
2. one who answered "for tidligt" / "for sent" with a time that fits the part ("Sagde: kan til
   09:30");
3. a reserve within his frame; 4. an hourly-paid driver; 5. a free colleague.

Rest, overlap, the longest day (`plan::can_take`) and the time to change from step 2
(`pro_duties::Change`) hold for each. The cut view shows "Klip vagt 1002", "05:45–13:55 · klippet
ved Station 09:20 og Bytorvet 12:55", a bar of the parts (green with a driver, red without), each
part with its suggestion and the same one-thing button, "Kører 4 t 35 af 8 t 10" and **"Saml vagten
igen"** (`uncut`, which also drops the parts given and their questions).

The cut lives only for its date (`Split` in `Ops::splits`, its tour cuts `Split::inner` applied
when the date's tours are laid out - `network::finish_tours` -, so each part is whole pieces of the
date's plan; the Vagter and Turnus tabs keep their week). A part cut off inside a piece is a duty
of its own that day; a part is given by its pieces (`Call::part`, `Call::pieces`) and the calendar
names it by the duty and its letter: "1001 + 1002C" (`pro_cut::label_of`, `DayIs::Duties::labels`).
Days gone are dropped every night. An older file has none.

## Agency drivers only when the setting is on
"Vikar" is hardly used in Denmark. The box offers **"Vikar <pris>"** (today the dispatcher's
agency driver, `plan::Fill::Agency`; a later day one booked, `agency::book`) only when the
effective setting is on; off, it says "Uden en chauffør udgår N ture". **On Bus Company Online only
the server's admin decides** (`agency_fallback`): the company's settings show "Bestemmes af
serverens admin" and the owner can no longer change it (`live_pay::set_agency` refuses,
`ADMIN_DECIDES`; an older choice of his no longer counts). Off the server, where there is no
admin, the owner chooses in the company's settings ("Duties nobody covers"), and none chosen means
**dropped** - before, a company off the server never sent duties to the agency, so only an owner
who now picks agency drivers sees a change.

Tested: `pro_ops::tests` (`the_rules_of_asking_a_reserve_the_hourly_paid_and_a_permanent_driver`,
`an_ai_driver_answers_in_game_time_and_not_at_night`), `pro_cut::tests`, `live_more::tests`
(`the_agency_fallback_by_the_server_admin_and_off_the_server_by_the_owner`) and the API's
`the_calendar_and_the_box_show_ask_and_give_a_duty`.

# The box in a popup, and drivers asked again after a no (4 November 2026)

## The popup
The player wanted the box as a popup rather than a column beside the calendar. The calendar has
its whole width again; a day clicked - its "Mangler en chauffør" cell, its heading or any cell of
it - opens a **popup** over the dimmed calendar (`ops.js`: `Pop`, RosterFlow's sheet style). Its
title is the day ("Torsdag 7 Mar", how many duties are missing), the duty ("Vagt 1002",
"Torsdag 05:15–13:30 · linje 5 · Ingen chauffør planlagt") or its cut, and inside are the box's
steps as before: the day's duties without a driver, who can drive one (Faste, Ramme, Timelønnede,
the answers in their boxes, "▸ N andre kan ikke køre den"), "Klip vagten" and its parts, the agency
driver when the setting is on. The popup stays open while answers come in (asked again every two
seconds) and closes with ✕, Escape or a click beside it. A duty given (Sæt på, Giv vagten) brings
the day's list back with a green line "Vagt 1002 er givet til Nina Bach"; when nothing is left,
"✓ Alle vagter er besat" and a "Luk" button. Daglig drift lists the day's missing duties on its
page; "Find chauffør ›" opens the same popup.

## Asked again the same day after a no (`pro_ops::Mood`)
A driver who said no today does not like being asked again **for the same day**. His **real
refusals** today of duties of that date (`refusals`, `Ask::refused_at`) count - round F, after the
player's report from beta: a no for a duty on the 15th leaves him calm when asked about the 16th;
the kept noes carry their duty's date (`Refusal::date`; an older file's without one do not count): an AI driver's no - tired, plans of his own, irritated,
angry - come before the question was closed. "Too early", "too late", "no answer", a no for what he
cannot (ill, on holiday, his licence, another duty, a day too long), an answer still to come, and
a question taken back or given another before he answered do not count; a no taken back after it
came does. A question asked again replaces the old one, and its no is kept (`Ops::noes`).

- **One no today - "😒 Irriteret · sagde nej kl. 09:12":** asked again for a duty of that day, his yes is
  much less likely (`IRRITATED_YES`, ×0.4; a reserve within his frame from 12:00 the day before
  too) and grudging ("Okay da… men så er det også sidste gang i dag."), his no irritated ("Igen?
  Nej.", "Jeg har allerede sagt nej i dag."); each such question costs 2 points of contentment
  (`IRRITATED_COST`).
- **Two - "😠 Sur":** asked again he says no, angry ("Nej! Lad mig nu være i dag."), 5 points
  (`ANGRY_COST`).
- **Three (his angry no among them):** he answers no more questions about that day today -
  "Svarer ikke mere om den dag i dag" in his row instead of "Spørg", and the command refuses
  (`ANSWERS_NO_MORE`); a duty of another day he is asked about calmly.

The popup shows the tag beside his name before he is asked, and "Spørg" says why on hover ("Har
allerede sagt nej i dag"). Each of several hourly-paid drivers asked at once counts for himself.
A new day - the date of the company's clock changing at midnight (no driver answers between 23:00
and 06:00 anyway) - starts calm; the day's noes are dropped with the days gone (`tidy`).
Contentment stays between 0 and 100, and nobody leaves for this alone. The same every time
(`Rng`: "pro-mood").

Tested: `pro_ops::tests` (`an_irritated_driver_says_yes_much_less_and_grudgingly`,
`an_angry_driver_always_says_no_and_then_answers_no_more_today`,
`only_real_refusals_of_the_day_count_and_a_new_day_starts_calm`).

## A driver calls in sick on the day (`co::pro_sick`, 4 November 2026)

Besides the night's illnesses (`staff::after_day`, known in the morning), a driver planned for the
company's day can call in sick before his first duty of it. The day's calls are drawn when the
clock writes the day (`clock::script` → `pro_sick::draws`, `What::SickCall`; `Rng`: "pro-sick" -
the same for the same company, day and driver). The chance (`pro_sick::chance`) is `CHANCE` (2.5 %
a planned day) by the game mode's share of illness, more for a stressed, an unhappy and an
unreliable driver: a few a week in a company of ten drivers. When the call comes, spread before
each kind of duty:

- an early duty (from 08:00 or before): 20-75 minutes before it, never before 04:00;
- a day duty (before 12:00): half in the morning (05:00-07:30), half 30-120 minutes before it;
- a late duty (from 12:00): mostly 30 minutes to 3 hours before it (a 16:00 duty: 13:00-15:30),
  now and then in the morning (06:00-09:00).

A call (`pro_sick::call`): the driver is ill from today for 1-3 days (`Employee::sick_until`, the
same illness the staff card and the calendar show: his duties are open "Ill"), his message to the
control room, and the call is kept (`Ops::sick`) until the player has seen it. The feed and the
notice say "Sofie Juul has called in sick – duty 1006 is missing in 40 min" ("… is missing now"
once it has begun); "Find a driver ›" opens the calendar's popup on the duty, "Later" closes it.
While the player simulates the clock stops (`Ask::SickCall`); while the game's clock leads it goes
on and the notice shows all the same (`pro_sick::unseen`, the shell's `asking`). Nobody calls once
his first duty has begun, nor when ill or away already.

## The rest rules (`co::pro_rest`, 4 November 2026)

When a duty is put on, asked for or given (a part of a cut duty too), the box checks the Danish and
EU drivers' rules against the driver's other days (`pro_rest::breaches`, the API's `breaches_of`):

- 11 hours' rest between two working days (`DAILY_REST`), reduced to 9 (`REDUCED_REST`) at most 3
  times a week (`REDUCED_TIMES`, `reduced_in` counting the week's reduced rests); on the company's
  day the night's rest since his last duty stays the game's own hard rule (`plan::can_take`);
- a working day of at most 13 hours from its first sign-on to its last sign-off (`SPREAD_MAX`);
- at most 48 hours a week (`WEEK_MAX`; a duty within a reserve's frame adds nothing - the frame is
  paid).

The game's hard rules keep a driver out as before ("N others cannot drive it"). These are the
planner's to keep or to break: the row says it in red ("⚠ Only 9 h rest after duty 1005", "⚠ Over
13 h on the day", "⚠ Over 48 h in the week"), comes after the others of its group, and its button
is "Put on anyway" / "Give anyway" - asked first in the popup with what it costs. Given anyway
(`rf_ops_act` give with `anyway`; refused without it): the driver is less content (`BREAK_COST`)
and more stressed (`BREAK_STRESS`), and the company pays the fine when one is set
(`pro_rest::fine`, booked as a fine): on Bus Company Online the server's admin's `rest_fine`
(kroner, default 0; `live_pay::LiveCfg::rest_fine`, `REST_CFG`), off the server the owner's in the
company's settings ("Breaking the rest rules": none, 500, 1,000 or 2,500).

## The fair share of the extra duties (`co::pro_rest`, 4 November 2026)

Every duty given a driver beyond his own work - overtime, a day off, an hourly-paid driver's extra
hours; not a reserve within his frame - is counted for its month (`Ops::extras`, `count_extra`;
taken back, no longer: `uncount_extra`). The popup shows it by the name ("3 extra this month") and
sorts the rows "Cheapest first" or "Fewest extra first" (`rf_ops` `sort`: "fair"). A driver given
more than `TIRED_FROM` (4) in a month is a little more stressed with each one more. From the
month's second week, from 10:00 of the company's day (`clock::on_minute` → `pro_rest::grumbles`), a
driver who wants hours (hourly paid or part time) and was given none while another was given
`OTHERS_MANY` (3) or more tells the control room "I never get asked about extra duties." - once a
week at most - and is a little less content (`GRUMBLE_COST`).

Tested: `pro_ops::tests` (`a_driver_calls_in_sick_before_his_first_duty_and_the_player_is_told`,
`a_late_duty_gets_its_sick_call_in_the_afternoon_shortly_before_it`,
`the_rest_rules_eleven_hours_a_day_of_thirteen_a_week_of_forty_eight`,
`breaking_the_rest_rules_anyway_costs_content_and_the_fine`,
`the_extra_duties_of_the_month_are_counted_and_the_forgotten_grumble`), the API's
`rosterflow::ops::tests::the_box_warns_of_the_rest_rules_asks_first_and_counts_the_extra_duties`
and `company::dialogs::tests::a_drivers_sick_call_waits_as_a_notice_with_its_duty_even_while_the_clock_runs`.

# The drivers' wishes, time off in lieu and what the week costs (round B, 5 November 2026)

## The drivers' wishes (`co::pro_wish`)

The drivers send wishes of their own. Once a company day from 09:00 (`clock::on_minute` →
`pro_wish::daily`, `Ops::wished` the day drawn) each driver may send one - the same for the same
company, day and driver (`Rng`), by `CHANCE` (6 %) more often when less content and less when
friendlier (`chance`), at most `OPEN_MOST` (2) open at once; a handful a week in ten drivers:

- **a day off** on a date 4 to 14 days on ("I'd like the day off on 9/3, if possible."),
- **early duties**, **no split duty** (permanent drivers),
- **more hours** (mostly the hourly paid and part-timers).

Each comes as his message to the control room (the feed says "Mette Krogh has sent a wish.") and is
listed in the calendar's "Wishes (N)" (`rf_wishes`: the open ones, the earliest first, each "Grant"
or "Refuse" - `rf_wish_act`; a day off says which duty is left open). A day off wished is marked ★
on his day in the calendar and on his day in the Turnus tab (the row's week driver, and his next
weeks).

- **Granted** (`grant`): he is more content (`GRANTED`, 4). A day off becomes his free day
  (`model::FreeDay`, `FreeWhy::Wish`; `Employee::absent`, `plan::day_unavailable` →
  `Cannot::DayOff`): the plan keeps him off, his rota's duty that day is open (`pro_turnus::decided`
  shows the free day as an exception "wish", off), the calendar shows "Off (wish)" ★, and the page
  opens who can drive the duty left open. The others hold for `HOLDS_DAYS` (28).
- **Refused** (`refuse`, `REFUSED` 3) or **left open** until its day (a day off) or a week
  (`lapse`, `LAPSED` 4): less content; three in a row refused or lapsed (`refused_in_a_row`) cost
  `IN_A_ROW` (3) more each.
- **Asked anyway** (`bias`, `notes`): asked for a duty on the day he wished off he says yes far
  less (×0.35) and is a little less content (`ASKED_ANYWAY`, `pro_ops::ask_part`); the popup says
  "★ Wished the day off this day" by his name. One who wished early duties is glad of one signing on
  by 09:00 (×1.3) and rather not of one from 12:00 (×0.7); one who wished no split duty rather not of
  a duty spanning more than 11 hours (`SPLIT_SPAN`, ×0.5); one who wants hours glad of any (×1.4).

## Time off in lieu ("afspadsering", `co::pro_toil`)

- **Saved, not paid**: when a permanent driver (full or part time) is given a duty of overtime, the
  row has "Pay out" / "Time off in lieu" (the company's setting decides which is first:
  `Ops::toil_default`, "Overtime given in the calendar" in the company's settings, on the server
  too). Saved (`rf_ops_act` give with `toil`, `pro_ops::set_toil`, `Call::toil`): that day's
  overtime is not paid but added to his `Employee::toil`, hour for hour (`pay::day`, `Paid::saved`)
  - the row says "Time off in lieu 2 h 40" instead of its overtime's price.
- **Shown**: on the staff card ("Time off in lieu: 6,5 h") and under the name in the calendar
  ("toil 6,5 h").
- **A day of it** (`give_day`, from the day's popup: "Give time off in lieu (7,4 h)", `rf_ops_act`
  `toil_day`): a working day's hours (his contract week's fifth, fewer when fewer are saved, at least
  `LEAST` - an hour) taken from them; he is off (`FreeWhy::Toil`: the calendar's own colour, legend
  "Time off in lieu"), his duty that day open (the popup offers who can drive it), the day's hours
  paid at the normal rate, no guarantee owed for it (`pay::toil_day`). Taken back before the day
  (`take_back`, `toil_back`): the hours his again.
- **Too many**: above a full week saved (`TOO_MANY`, 37 h) he is less content every day
  (`TOO_MANY_COST`); with `ASKS_FROM` (12 h) saved he asks the control room now and then (`daily`, a
  quarter of the days, at most once a week): "Can I have my 12 hours of time off in lieu soon?".

## What the week costs (`co::pro_cost`) and the supplements (`pay::supplement_rate`)

With RosterFlow Pro the planner the drivers' hours of the evening, the night and the weekend pay a
supplement - the pay model had none (the server's admin's K11 supplements, `live_more`, are a
player's live pay): evening 18-23 15 % (`EVENING`), night 23-06 25 % (`NIGHT`), Saturday from 14:00
25 % (`SATURDAY`), Sunday all day 35 % (`SUNDAY`), the highest for each minute; a public holiday is
not known to the game and pays as its weekday. They are booked with the day's wages (`pay::day`); a
company planning otherwise is paid as before.

Above the calendar (`rf_cal` `cost`): the first week shown as planned - "Wages of the week 48.210
kr. · of it overtime 3.120 kr. · supplements 2.450 kr." (and the hours saved as time off in lieu):
each driver's week (`pro_cost::week`) by his employment - a permanent driver his contract week at
least, the hours above it as overtime, a reserve his frames and the rest, the hourly paid their
hours -, the supplements on the pieces of the days to come, the days of time off in lieu at the
normal rate; the overtime of duties given as time off in lieu saved. The wages without the
on-costs, as the popup's prices.

In the popup each row says what the duty costs that driver extra (`price_words`): "+0 kr. – within
the contract", "+0 kr. – within the frame", "+1.240 kr. overtime (2 h 40)", "+2.441 kr. for 7 h 30"
(the hourly paid), "+X kr. outside the frame", "+X kr. agency", with its supplements on top
("· +180 kr. supplements", `pro_cost::duty_supplement`) - or, saved, "Time off in lieu 2 h 40".

Tested: `pro_ops::tests` (`drivers_send_wishes_now_and_then_the_same_every_time_and_unanswered_they_lapse`,
`a_day_off_granted_frees_the_driver_and_leaves_his_duty_open`,
`a_drivers_wishes_change_his_answer_and_being_asked_anyway_annoys_him`,
`overtime_saved_as_time_off_in_lieu_is_not_paid_and_a_day_of_it_is`,
`too_many_hours_of_time_off_in_lieu_make_a_driver_ask_for_them`,
`the_evening_the_night_and_the_weekend_pay_a_supplement_with_rosterflow_pro`,
`what_a_week_costs_as_planned`), the API's
`rosterflow::ops::tests::the_wishes_time_off_in_lieu_and_what_the_week_costs` and
`company::people::tests::a_drivers_page_shows_the_time_off_in_lieu_saved_with_rosterflow_pro`.

## Round C (5 November 2026): undo in the Turnus tab, ill and on holiday one record, a swap of both drivers

### Undo and redo in the Turnus tab (`omsihub/rosterflow/turnus.rs`)

The toolbar's "↶ Undo" and "↷ Redo" (Danish "Fortryd" / "Gentag"), and Ctrl+Z / Ctrl+Y (or
Ctrl+Shift+Z) while the tab is shown - not while typing in a field, nor with "New rota" open -,
undo the tab's edits step by step and make them again: a duty put in or taken out of a day of a
row, a week added or taken out, a rota made, renamed, deleted, made automatically, its hours, its
start, a driver put on a row, a driver's day changed for a date (a day off, ill, on holiday, a
swap). Each edit (`rf_turnus_act`) keeps the rotas and their exceptions, the swaps and each
driver's illness and holidays before and after it (`Snap`); undone, they are put back as they were
(`undo_redo`) and the corner says what: "Undone: duty 1002 taken out of rota week 2, Wednesday"
("Fortrudt: vagt 1002 fjernet fra turnusuge 2, Onsdag"). The buttons' tooltips say what they would
undo or do again; greyed, there is nothing. At most 60 edits are kept, the planner's own beside the
company's session (`Rf::turnus_undo`/`turnus_redo`), forgotten when another company is opened; a
new edit forgets what was undone. A rota is the company's at once (there is no draft to save), so
the steps hold as long as nothing else changed the rotas: when another planner, the night (the days
gone tidied away) or the Kalender's box changed them since, the undo is refused - "The rotas were
changed elsewhere meanwhile: there is nothing to undo" - and the steps are forgotten.
On Bus Company Online `undo`/`redo` need the planning permission (`together::relay`).

### Ill and on holiday: the staff card's own (`co::pro_absence`)

A driver's day set "Ill" or "On holiday" in his next weeks is no longer an exception of the rota of
its own but his illness or holiday on the staff card - one record:

- **Ill**: from today on (or the day after an illness that runs until the day before) it lengthens
  `Employee::sick_until`; a day further ahead is a day he is known to be ill
  (`Employee::sick_planned`, a sick note ahead), which becomes his illness of the day when it comes
  (`pro_absence::tidy`, the night's). The planning (`plan::unavailable`, `day_unavailable`), the
  pay (`pay::away_paid_otherwise`) and the calendar (`pro_ops::driver_day`) ask `Employee::ill_on`.
- **On holiday**: an approved holiday of his by the planner (`holidays::Request`): next to one of
  his not begun it lengthens it, else one of its own. Its working days are taken from what is left
  of his year's when it begins (`holidays::night`); with none left it is refused ("They have not
  enough holiday days left for it.").
- **As in the rota** takes the day out of both (a holiday begun gives the day back); any other
  choice for the day too. A day gone cannot be set.
- **The other way round**: the rota's day (`pro_turnus::decided`) is off - "Ill", "On holiday" -
  whenever his records say so, whatever set them: the staff card, a sick call (`pro_sick`), a
  holiday asked for. The Turnus tab and the Kalender show it.
- **The staff card** lists the spells from today on (`pro_absence::spells`): "Ill 6.–9. Mar",
  "On holiday 4.–7. Mar" ("Syg 6.–9. Mar", "Ferie 4.–7. Mar").
- **Older files**: an exception "Ill" or "On holiday" of the Turnus tab is moved into the records
  when the company is opened (`pro_absence::migrate`, `store::migrate`); one that cannot be (no
  holiday days left) stays a day off.

### A swap changes both drivers at once (`co::pro_swap`)

- **In the Turnus tab**: a driver's day given another duty ("swap with its driver that day too")
  takes it from the driver whose rota has it that day, and that driver takes his day instead - his
  duty, his frame or his day off -: two exceptions "Swapped" made together (`swap_rota`). The
  corner says so, with the rest rules either would break in red ("⚠ Only 9 h rest after duty
  1005"). Nobody's rota has the duty: it is the driver's alone, as before.
- **In the who-can-drive box**: a driver on another duty then, with a free colleague who could take
  it, can be asked ("⇄ Drives 1005 then - Kasper Dahl could take it"); his answer may be a swap.
  The row then says who takes his duty ("⇄ Kasper Dahl takes duty 1005 instead: both changed at
  once"), and the rest rules are checked for both - the colleague's breaches by his name; broken,
  "Give it anyway" asks first, and both pay for it (`pro_rest::broke`). "Give the duty, with the
  swap" gives both duties in one act (`pro_ops::give_part`) and keeps the swap
  (`pro_swap::after_give`).
- **Both are told** (`SWAPPED`, `SWAPPED_OFF`, `SWAPPED_TO_OFF`), and the calendar marks both days
  "⇄" with whom (`rf_cal` cells' `swap`); their hours and costs follow from the days as they are.
- **Undone** in the day's box ("Swaps this day", "Undo the swap" - "Fortryd bytte", `rf_ops_act`
  `unswap`) until the first of the two duties begins (`can_undo`): the exceptions as they were, the
  two duties given taken back; both told (`UNSWAPPED`). A swap no longer as made (a duty given
  another way since) is forgotten, as are those of days gone (`pro_swap::tidy`).

Tested: `pro_ops::tests` (`ill_set_in_the_turnus_tab_is_the_staff_cards_illness_and_the_other_way_round`,
`a_holiday_set_in_the_turnus_tab_is_an_approved_holiday_of_his_and_counts_against_his_days`,
`an_older_files_turnus_ill_and_holiday_days_move_into_the_staffs_records`,
`a_swap_in_the_turnus_tab_changes_both_drivers_at_once_and_is_undone_before_its_day`,
`a_swap_of_the_daily_operations_is_kept_told_and_undone_with_both_duties`) and the API's
`rosterflow::turnus::tests::the_turnus_tab_undoes_and_redoes_and_ill_and_holiday_are_the_staff_cards`.

## Round D (5 November 2026): holiday pay, early and late people, tired drivers

### The public holidays (`co::pro_holiday`)

- **Which days** (`name`, `is`): a company playing in Denmark (`econ_uk::country_of`) keeps the
  Danish holidays (`service::danish_public_holiday`): New Year's Day, Maundy Thursday, Good Friday,
  Easter Sunday, Easter Monday, Ascension Day, Whit Sunday, Whit Monday, Christmas Day and the Second
  Day of Christmas, Great Prayer Day until 2023. Every other company keeps the common German and
  Dutch ones (`service::common_public_holiday`): New Year's Day, Good Friday, Easter Monday, Labour
  Day, Ascension Day, Whit Monday and the two days of Christmas. Christmas Eve and New Year's Eve
  are no public holidays in either. These are the same functions the timetables' day types use.
- **The calendar** (`rf_cal`): a holiday's day header has its name under the date ("✦ Easter
  Sunday", `days[].holiday`, tooltip "Easter Sunday: a public holiday - paid as a Sunday (35 %)"),
  and the popup's duty line names it.
- **The pay** (`pay::supplement_rate_on`, `supplement_minutes_on`): a holiday is paid as a Sunday
  (`pay::SUNDAY`, 35 %) all day; the highest supplement of a minute counts, as before, so a holiday
  evening is 35 %, not 50 %. A duty past midnight is paid by the next day's (`pro_holiday::days`:
  the day and the next). The night's pay (`pay::day`), the week's cost (`pro_cost::week`: its days
  are now dates, `WeekCost::holidays` the holidays' share: "of it holiday pay 1.230 kr.") and a
  driver's price in the popup ("· +410 kr. holiday pay") all count it.
- **The drivers**: asked for an extra duty on a holiday a driver says yes less often
  (`pro_holiday::bias`, ×0.75). Each holiday a driver works is kept (`Employee::holidays_worked`,
  twelve months, `after_day`); from the fourth in twelve months (`MANY` = 3) each costs two points
  of satisfaction (`COST`). The staff card says "Public holidays worked this year".

### Early or late people (`co::pro_pref`)

- **Who is which** (`Employee::shift_pref`): early, late or neither, about a third each. A driver
  hired gets theirs drawn from their number and name (`seed`, the same every time); a driver of an
  older file gets the same drawn when the company is opened (`seed_all`, `store::migrate`).
- **Early or late** (`kind`): a duty signing on before 08:00 (`EARLY_BEFORE`) is early, one signing
  on at 13:00 or later (`LATE_FROM`) late, the day duties between are neither.
- **What it does**: a duty they like gives a yes more often (`bias`, ×1.25), one they do not less
  often (×0.7); a day worked on a duty they like adds 0.4 points of satisfaction (`after_day`,
  `GLAD`), one they do not takes 0.6 (`SORRY`). The dispatcher's own choice between two drivers
  otherwise equal (`plan::day_plan`'s score, before experience) takes the one who likes the duty
  (`fit`). The rotas are not changed by it: every driver goes through all the weeks of a rota.
- **Shown**: on the staff card ("Preference: 🌅 Prefers early duties", or "No preference") and by
  each name in the popup (`rows[].pref`: green when the duty fits, orange when not).

### Tired drivers (`co::pro_tired`)

- **How tired** (`assess`): from the working days up to the day (each its first sign-on and last
  sign-off), those since the last day off - the streak -, how many there are, how many long (over
  9 hours from sign-on to sign-off, `LONG`) and how many rests between them short (under 11 hours,
  `pro_rest::DAILY_REST`):
  * **Tired** ("😮‍💨 Tired"): 6 working days in a row, 4 long days or 2 short rests.
  * **Worn out** ("🥱 Worn out"): 8 working days in a row, 6 long days, 4 short rests, or 6 days in
    a row of which 4 long.
  * The reason said: "6 long days in a row" (the last long days, 4 or more), "5 long days without a
    day off", "7 working days in a row", "2 short rests without a day off".
  * A day off ends the streak; one day off alone after a hard streak leaves them a step less tired
    ("only one day off after a hard week"), two and they are fresh.
- **The days** (`on_day`): the days gone as worked (`Employee::worked`, the night's pieces kept for
  two weeks, `after_day` from `day::close_day`), the company's day and those ahead as planned. The
  calendar reads the plans of the two weeks before the first day shown (from the company's day on);
  the popup those of the duty's day, the duty asked for on top.
- **What it does**: a yes to an extra duty less often (`yes`: tired ×0.8, worn out ×0.55, in
  `pro_ops::ai_reply` through `Situation::tired`); a sick call on the day more often (`sick`: ×1.6,
  ×2.5, `pro_sick::draws`); more minutes lost on the way, late more often (`slow`: ×1.4, ×1.8,
  `lateness::model_with`'s driver share).
- **Shown**: 😮‍💨 or 🥱 in the corner of the driver's cell of the day with the reason
  (`rf_cal` cells' `tired`), in the popup's row (`rows[].tired`: "😮‍💨 Tired – 6 long days in a
  row"), and on the staff card ("Tiredness", by the days up to yesterday). A part of a cut duty is
  not given it.

Tested: `pro_ops::tests` (`the_public_holidays_have_their_names_and_are_paid_as_a_sunday`,
`working_many_holidays_makes_a_driver_less_content`, `early_and_late_people_like_their_duties`,
`many_long_days_make_a_driver_tired_and_days_off_rest_him`,
`a_tired_driver_or_a_duty_not_liked_gets_fewer_yeses`).

## Round E (5 November 2026): drivers propose swaps, the seasons, a Monday report

### The drivers propose swaps themselves (`co::pro_trade`)

- **When** (`daily`): once a company day from 10:00 (`clock::advance`, which reads the duties of the
  seven days ahead from the world for it, as the Vagter tab numbers them), at most one new
  proposal: a chance of 12 % (`CHANCE`), 10 % more for each driver with a reason in the days ahead
  (`PER_REASON`), 60 % at most (`MOST_CHANCE`) - two to four a week in a company of ten. At most
  three open at once (`OPEN_MOST`). The same for the same company and day (`Rng`).
- **What** (`Offer`): two drivers on their rotas swap their duties of a day one to six days ahead
  (`Swap`), or one gives a duty to a colleague whose rota has the day off (`Give`; not a free day
  of a wish or time off in lieu, not ill or away). Drivers already in a swap or a proposal that day
  are left out.
- **Why** (`Why`), as round D's drivers are: a tired or worn-out driver (`pro_tired`, the days gone as
  worked, those ahead as the rota has them) gives a duty to a colleague off that day who is fresh and
  stays so ("Lone Berg is tired and would like the day off", weight 3, worn out 5); an early person
  with a late duty swaps with a colleague whose early duty they like more, the other not worse off
  ("Ahmed Ali would rather drive early, Nina Bach late" when both gain, weight 3, else 2); and now
  and then, without a reason, an appointment ("… has an appointment that day", `REASONED`: 80 % of
  the days with a reason take one). Of the drawn order the first legal for both under the rest
  rules (`pro_swap::breaches`: rest before and after, the day's span) is proposed; when none is, the
  first, and the popup shows the ⚠ by the driver's name.
- **Told**: the first driver's message to the control room ("Nina Bach and I would like to swap
  duties on 9/3: I drive 2002, Nina Bach drives 3003. Is that all right?", or "… has offered to take
  my duty 1004 on 9/3"), the feed ("… and … propose a swap.").
- **Decided** in the calendar's "Swap proposals (N)" (`rf_trades`, `rf_trade_act`; the planning's
  permission in multiplayer): each its two drivers, what ("⇄ Mette Krogh and Ahmed Ali want to swap
  on saturday 9 Mar: 2002 ⇄ 3003", "⇄ Lone Berg wants to give duty 1003 on thursday 7 Mar to Peter
  Sand"), why, the rest rules broken, when it was sent and by when to answer.
  * **Approve** (`approve`): the swap of round C (`pro_swap::swap_rota` - both told, ⇄ in both cells,
    "Undo the swap" in the day's box until its first duty begins); both drivers +2 satisfaction
    (`APPROVED`). A proposal whose days changed since (a duty given another way, a day off set) is no
    longer listed and cannot be approved ("The duties have changed since …"); it lapses.
  * **Refuse** (`refuse`): both −2 (`REFUSED`).
  * **Left open** until its first duty begins or three days (`LAPSE_DAYS`, `lapse` on the day's draw
    and the night's tidy): both −1 (`LAPSED`).
- Kept 45 days after they are decided (for the Monday report), dropped with a driver who leaves.

### The seasons (`co::pro_season`)

- **Winter** (1 December to the end of February): 40 % more sick calls on the day
  (`WINTER_SICK`, in `pro_sick::draws` with the round D tiredness).
- **Spring** (1 March to 19 June): the drivers ask for their summer holiday - 50 % more holiday
  requests (`SPRING_ASKS`, in `holidays::ask`).
- **Summer holidays** (20 June to 15 August): 80 % more holiday requests (`SUMMER_ASKS`), and a
  request for the summer (its reason, or one of its days in the summer holidays) refused costs 50 %
  more satisfaction (`SUMMER_REFUSAL`, on `holidays::refusal_cost` in `holidays::answer`).
- **Shown**: a chip in the calendar's head by the week ("❄ Winter – more call in sick", "🌷 The
  summer holiday is planned – more holiday requests", "☀ Summer holidays – many want time off",
  `rf_cal`'s `season`, by the first day shown), its tooltip the effect in numbers.
- RosterFlow Pro only: the other planners keep the year as it was.

### The Monday report (`co::pro_report`)

- **Counted** night by night (`after_day`, from `day::close_day` after the pay): the wages paid
  (`pay::Paid::cost`), of them the overtime (hours, and its cost: `Paid::overtime_cost`) and the
  supplements (`Paid::supplements`), the drivers' days ill, their calls in sick on the day, the duties
  left without a driver (`DayReport::uncovered`); in the day the drivers who grumbled of no extra
  duties (`complained`, from `clock::on_minute`). Per week from Sunday (`Ops::tallies`, this week's
  and the one before).
- **Made** (`monday`, `clock::on_minute`): on Monday from 06:00 - or the first full hour after, later
  in the week, if the clock jumped Monday - the week before's report (`make`): the tally, the
  wishes granted and refused or left that week (`pro_wish`), the swap proposals approved and refused
  (`pro_trade::decided_in`), the most tired driver by the Saturday as worked (the more tired, then
  the larger number of its reason) and the least content driver now. A week nothing was counted has
  none (a company new to RosterFlow Pro). The last eight are kept (`Ops::reports`).
- **Told**: the feed ("📋 Monday report for week 12 is ready"), the inbox (Staff: the week's wages,
  overtime, supplements, days ill, duties without a driver), and a notice that waits until seen
  (`unseen`; the company's dialog "report": "Open ›" opens the calendar with the report, "Later" -
  `report_seen`).
- **The report** (`rf_reports`, the calendar's "Reports" - a red dot while one is unseen; opened, it
  is seen): tiles - Wages paid (of them supplements), Overtime (hours, cost: orange when any), Days
  ill (red when any; how many called in sick on the day), Without a driver (red when any), Wishes
  ("1 granted · 2 refused", red when more refused), Swap proposals, Complaints (red when any), Most
  tired (red worn out, orange tired, green "Nobody"), Least content (red under 35, orange under 55) -
  and arrows to the older ones.

Tested: `pro_ops::tests`
(`a_tired_driver_proposes_to_give_a_duty_to_a_fresh_colleague_and_approved_it_is_a_swap`,
`early_and_late_people_propose_to_swap_and_refused_or_left_they_are_less_content`,
`a_proposal_no_longer_as_made_lapses_when_approved`,
`winter_brings_more_sick_calls_and_the_summer_more_holiday_requests_and_dearer_refusals`,
`the_monday_report_counts_the_week_before_and_is_made_once_on_monday_morning`), and the page's
`rosterflow::ops::tests::the_drivers_swap_proposals_the_season_and_the_monday_reports`.

## Round F (5 November 2026): named frames RAM1-RAM4, the noon notice, a mixed team

### Named frames ("rammer", `pro_turnus::NamedFrame`)

- **The list**: the company's named frames, kept in `Rotas::frames` (None in a file never changed:
  `DEFAULT_FRAMES`) - RAM1 04:00-14:00, RAM2 07:00-18:00, RAM3 11:00-22:00, RAM4 15:00-06:00 (`to`
  30 * 60: past midnight, into the next morning). The Turnus tab's "Frames" on the left changes a
  name or its times where they are, adds one ("New frame") and deletes one (`rf_turnus_act`
  `frame_put` / `frame_remove`, planning permission in "Play together"; undone like the tab's other
  edits). A changed frame brings every rota day and exception with it to its new times; a deleted
  one leaves them as frames of their own times (`ram` 0).
- **In a day**: a rota's day can be a named frame as it can be a duty (`Cell::Frame { from, to, ram
  }`, `ram` the frame's id; its times taken from the list). The rota shows "RAM2 07:00–18:00", the
  calendar "RAM2" on the driver's day (a small "RAM2" tag on a duty driven in it), the day panel and
  the who-can-drive box the frame's name for the reserve.
- **It holds** the duties in its window - and RAM4's past midnight: a duty at 01:00 is the frame's
  of the day before (`pro_turnus::frame_holds`, used by `plan::fill_tier` and the box's prices).
- **Pay**: a day in a named frame pays the hours driven, at least 7,4 (`NAMED_LEAST` = 444
  minutes, a fifth of a 37-hour week) - not the window (`pay::framed_paid`). A frame of its own
  times is paid whole as before. The rota's hours count a named frame's day as 7,4 h, and a reserve
  rota of named frames aims at five such days, 37 h. A 15-hour frame is a window, not work: only what
  is driven counts for the day's length and the rest (round A's rules, when a duty is given).
- **"Make the rota automatically"** for a reserve rota: the named frames forward week by week -
  week 1 RAM1, week 2 RAM2, week 3 RAM3, week 4 RAM4, week 5 RAM1 ... -, five days a week, the two
  days off together a day later each week (RAM1 Sunday and Monday, RAM2 Monday and Tuesday, RAM3
  Tuesday and Wednesday, RAM4 Wednesday and Thursday, `named_off`), so at most 6 days run into the
  next week and the cycle comes back to RAM1 after a Sunday off. With no named frames left (all
  deleted) the reserves' own frames and the early and late frames of before.
- **The rules** between two named frames: windows overlapping (RAM4 to 06:00, then RAM1 from 04:00
  the next day) are red ("The frame the day before runs until 06:00, this one begins at 04:00: they
  overlap"); a gap under 11 hours between two different ones yellow (RAM2 then RAM1: "10:00 between
  the two frames: the duties in them must leave at least 11:00"); the same frame day after day is
  not marked (the duties in it are checked when given).

### The noon notice (`pro_ops::rule_of`, `before_noon`)

- A reserve whose frame holds the duty, put on before 12:00 the day before (`plan::RESERVE_TELL_FROM`,
  12 * 60): no question - "Sæt på" - and told: a message to the driver "You drive duty %{n} on
  %{date} in your %{frame}" ("Du kører vagt 1004 den 2024-03-05 i din RAM2"; without a named frame
  "… in your frame", `messages::RESERVE_SET_NAMED` / `RESERVE_SET`).
- From 12:00 the day before on (or the same day): asked first, the row saying "⏰ Short notice – must
  be asked" ("⏰ Sent varslet – skal spørges"). The reserve says yes 85 % of the time
  (`LATE_NOTICE_YES`), else no - plans of their own - and a no counts for the moods of round 1
  (irritated, angry) as any other.

### A mixed team ("Blandet hold", `RotaKind::Mixed`)

- A kind of rota next to Ordinary, Split duties and Reserve (`Turnus::mixed`, saved only when set).
  Its duties are early, middle, late or night by their times (`pro_turnus::shift_of`): night when
  signing on at 21:00 or later or off after 01:00 (`NIGHT_FROM`, `NIGHT_AFTER`), else early before
  08:00, late from 13:00 (`pro_pref`'s), middle between.
- **"Make the rota automatically"** (`auto_mixed`): as many weeks as an ordinary rota of the same
  duties, rounded up to two weeks for each kind the week has - weeks 1-2 early, 3-4 middle, 5-6
  late, 7-8 night, then early again. A driver moves a row down a week: forward from early to night,
  so the rest holds. Of each two weeks one works the Saturday and the other the Sunday (every other
  weekend off; each kind on both weekend days), and the second night week's Saturday and the next
  early week's Sunday are off: two days off after the nights. Each day its weeks take the duties of
  their kind (else the nearest kind, the rest kept as far as it can); a weekday with fewer duties
  than weeks gives the day off where a kind has more weeks than duties.
- **Shown**: each week's kinds counted in coloured chips by its number (early, middle, late, night -
  "2 × Night"); night duties have their own colour. The rules: an ordinary rota's, and at most 4
  night duties a week (`MOST_NIGHTS`; more: yellow, "6 night duties in one week: at most 4 is easier
  on the drivers"). Over the cycle every driver has every week, so the same of each kind.

Tested: `pro_turnus::tests` (`named_frames_ram1_to_ram4`, `a_mixed_team_takes_turns`,
`a_named_frame_pays_the_hours_driven_and_holds_the_night`, `a_reserve_rota_is_made_of_frames`),
`pro_ops::tests::an_ai_drivers_answer_is_human_and_the_same_every_time` (short notice), and the
page's `rosterflow::ops::tests` (the reserve told when put on).

## Round G (5 November 2026): tick and ask many, the answers beside them, a mark on the missing cell

**The who-can-drive popup, one line a driver** (`ops.js` `Who`/`Line`, the API's `row_json`
gives `chip` and `price` apart). Each line: a tick box (only for a driver to ask), the name with
its kind ("Fast", "RAM3", "Timeløn"), "N ekstra denne måned" and its signs - 😒/😠 the mood, ★
a wish, 🌅/🌙 early or late (green fits, orange not), 😮‍💨/🥱 tired, ⏰ short notice, ⇄ a swap
possible, ⚠ the rest rules, each with its words on hover -, the price (paid out or saved as
time off in lieu as the toggle says) and the answer column:

| State | The column |
|---|---|
| Not asked | "–" (tick it) |
| Waiting | "🕒 Venter… ca. N min" (from the answer's due on the company's clock) and "✕" to take it back |
| Yes | "Ja" (his words on hover) and "Giv" - "Giv alligevel" (red, confirmed first) when the rest rules break; the Udbetal/Afspadsering toggle beside it when it applies |
| A swap | "⇄ Bytte" (who takes his own duty on hover) and "Giv med byttet" |
| No | "Nej", "For tidligt", "For sent" (his words on hover) |
| No answer | "Intet svar" |
| A reserve in the frame before noon the day before | no tick box: "Sæt på" (round F) |

The groups (Fast, Ramme, Timeløn) are thin headings; those who cannot drive it stay folded. The
bar below: "N valgt · M andre kan ikke køre den" and "Spørg de valgte" - everyone ticked asked at
once (`rf_ops_act` `ask` with `drivers`/`guests`; one ticked who is asked already is left out:
"De valgte er allerede spurgt."). The popup is 780 px wide and fits about 12 drivers without
scrolling. The players of "Play together" are lines of their own, ticked the same way.

**A mark on "N vagter mangler"** (`pro_ops::mark_of`, `Mark`, the calendar's `days[].mark`): a
badge on the cell's corner - green "✓ N ja" for yes answers not seen yet, red "✗ alle sagde nej"
when every question of the day is answered and none said yes (some of those noes not seen yet),
grey "🕒 venter på N" while questions wait; green over red over grey. A click opens the day's box,
or the duty itself when the answers not seen are all for one. An answer is seen when the planner
opens its duty's box (`Ask::seen`, the act `seen` - the planning's permission; no word said, the
plans not made anew).

**A toast for a yes** (`AnswerWatch`, mounted wherever RosterFlow Pro is open; `rf_answers` every
4 seconds - RF_VIEW): "Nina Bach sagde ja til vagt 1004" once a yes, unless that duty's box is
open; the yes answers there when RosterFlow opens are not said. Any answer come makes the views
ask again (the marks).


## Round H (5 November 2026): the hourly paid say when they can work

The player's wish: "a tab where the hourly paid can wish for work - Per can work Tuesday 5 to
15, or all day - and the system must remember it when you ask him". Code: `co::pro_avail` (the
core), `omsihub/rosterflow/avail.rs` (the tab's commands `rf_avail`, `rf_avail_act`),
`launcher/ui/js/omsihub/rosterflow/avail.js` (the tab).

**A day** (`pro_avail::Can`, kept in `Ops::avail`, one record a driver a date):

| Says | Holds |
|---|---|
| All day ("Hele dagen") | any duty of the day |
| A window ("05–15", "05:30–15:00") | from-to within the day, 00:00 to 24:00, an hour at least |
| Not that day ("Kan ikke") | nothing |
| Nothing ("–") | the planner cleared the day: not filled in again |

Each record says who said it (`By::Driver` - the AI driver or the player himself - or
`By::Planner`) and when, on the company's clock.

**Filled in.** In the company's night (`pro_avail::daily`, from the company day's first minute,
`clock::on_minute`) every AI hourly-paid driver fills in the next 14 days (`AHEAD`) he has not
said yet, each day drawn for the company, the driver and the date (`drawn`, the same seed as round
F's interim draw, so an older company's drivers keep saying what they said):

| Day | Weight | With the wish "more hours" (round B) |
|---|---|---|
| All day | 35 | 50 |
| 05–14 | 20 | 16 |
| 11–20 | 20 | 16 |
| 14–24 | 15 | 14 |
| Not that day | 10 | 4 |

A day past the 14 (asked for a duty further ahead) answers by the same draw. Kept a week after its
day (`KEEP_DAYS`), dropped with the drivers gone or no longer hourly paid and the players gone
(`tidy`). Round F's `pro_ops::own_hours` is gone: the records are what the answers use.

**Remembered when asked** (`pro_ops::ai_reply`, `answer_time`):

- a duty all within what he said: his yes ×1.6 (`INSIDE_YES`), at most 0.97 (`INSIDE_MOST`), and
  he answers in half the time (`INSIDE_WAIT`); the "too early / too late for some" of very early
  and very late duties does not apply to him;
- outside it: too early ("I could from 11:00") or too late, the window's edge;
- not that day: the gentle no "Jeg kan ikke den dag, desværre." (`NoWhy::NotThatDay`);
- cleared ("–"): as a driver who said nothing - the usual chance.

**The who-can-drive list** (round G's popup): by an hourly-paid driver's name "✓ Kan 05–15" or
"✓ Kan hele dagen" (green) for a duty within it, "Kan kun 05–15" (orange) outside it, "Kan ikke den
dag" (grey) - that row greyed, without a tick box (asking him is a sure no). In the hourly group
those within come first, then nothing said, then outside, then not that day (`avail_of`); "Find
chauffør ›" after a sick call opens the same list. A player of Play together who said his day
shows it by his name too.

**The calendar:** an hourly-paid driver's empty day from today on that he said he can work is
faintly green with its times ("05–15", "hele dagen"; "Kan arbejde 05–15" on hover).

**The tab "6 Kan arbejde"** (after Kalender): the hourly paid and the players who drive, a row
each, the next 14 days (or the 14 after), each day "05–15", "Hele dagen", "Kan ikke" or "–" - a
day the planner set dashed. A day clicked opens a small box: "Hele dagen", "Kan ikke den dag",
"Fra … Til … Gem", "Ryd", with who said it ("Meldt af chaufføren 4. mar 06:00", "Sat af
planlæggeren"). Saved at once.

**Who may change a day** (`avail::may`): the boss and a player who may plan, every row; a player
who drives, only his own (the relay lets `rf_avail_act` through with `Plans` or `Drive`, the
command checks whose row). A player's own day is `By::Driver`, the planner's `By::Planner`.

**Also fixed in round H:** `rf_wishes`, `rf_wish_act`, `rf_trades`, `rf_trade_act`, `rf_reports`
and `rf_answers` (rounds B, E and G) were answered by `ops::run` but never routed to it from
`rosterflow::run` - the Ønsker, Bytteforslag and Rapporter popups and the answers' toast got no
answer in the launcher. They are routed now, with `rf_avail*`.


# Round I: the bus dispatch (5 November 2026)

Code: `co::pro_buses` (the core), `omsihub/rosterflow/buses.rs` (the commands `rf_buses`,
`rf_bus_pick`, `rf_bus_act`), `launcher/ui/js/omsihub/rosterflow/buses.js` (the tab "7
Busdisponering").

**One truth.** The dispatch's choice of a bus for a block on a date (`Ops::buses`, a
`BusPick`: a bus, or none at all) is what the day plan runs: `plan::day_plan` takes the choices
of the date first, then the weekly roster's bus (`RosterTour::bus`) for the blocks the dispatch
did not touch - with the same checks (in the fleet, not in the workshop, fit to run, one of the
line's buses, free then; else the block has no bus and says why). So the world drives the bus
chosen, the pages and the day's close see it. A choice holds for its date only and is dropped a
week after it (`KEEP_DAYS`).

**Who can take a block** (`pro_buses::fit`, `check`): still the company's; not in the workshop
that day; not one that did not start this morning; condition 20 or more; one of the line's buses
(`ownline::line_allows`); as big as the tour asks for (`ownline::wanted`, a bigger one warns); what
the line's contract asks of its buses (`bus_contracts::on_line`: zero emission, its `Needs`' fuel,
floor and age); free then, with ten minutes between two blocks (`staff::BUS_MARGIN`); and enough in
the tank or battery for the block's kilometres and a tenth more (`RANGE_MARGIN`). The level: full
from the depot in the morning (today: as last known, `Vehicle::fuel` at `fuel_at`), less the bus's
blocks before it that day at its fuel use (`economy::consumption`, `fuel::tank`); an electric bus
charges at the depot in a gap of 30 minutes or more (`CHARGE_GAP`) at 150 kW (`CHARGE_KW`).
Warnings, not refusals: a service due within 500 km after the block (`SERVICE_SOON_KM`), dirty
inside (under 40, `DIRTY_BELOW`), three breakdowns or more (`MANY_BREAKDOWNS`), bigger than asked.

**The day** (`rf_buses`): a row for every bus of the fleet in number order - "Bus 104", "12 m ·
diesel · 92 %" (an electric one "12 m · el · 64 % · ca. 180 km"; a day to come: full), "service om
240 km" (orange, under 500 km), "værksted til fre" (red; the row a yellow "På værksted"), "Brudt ned
kl. 10:42" (a bus that broke down on its way today) - and its blocks of the day on a time axis in
the line's colour ("5/01 06:00–13:50"; one chosen by hand underlined), an electric bus's charging
gaps dashed "Lader", the day's kilometres. On top in red "Mangler en bus · N vognløb": the blocks
without a bus, dashed, with why on hover ("Ikke planlagt", "Bus 104 er på værksted", "Bus 104
startede ikke", "Ingen bus, som valgt"). Filters Alle / Diesel / El / På værksted; ‹ dag ›, "I dag".

**The week** (Dag / Uge): the buses against Sunday to Saturday, a cell the blocks of that bus that
day ("5/01, 7/02"), "Værksted" or "–"; on top "Mangler en bus" with the count a day (a day clicked
opens it).

**A block clicked** (`rf_bus_pick`): "Vognløb 7/03 · 06:30–12:45 · linje 7" with its kilometres,
the size it wants and its bus now; the buses that can take it ("på linjen i dag" first, their
warnings in orange) with "Sæt på"; a bus busy then whose block this bus could take in turn with
"Byt med 5/02" (both blocks' buses swapped); "Fjern" (no bus at all) and "Tilbage til
vagtplanens" (the dispatch's choice dropped); folded under "N busser kan ikke tage den" the others
with why ("Kører vognløb 5/01 da", "På værksted til 8. mar", "For lidt på tanken: 54 km af 100
km", "For lille: vognløbet vil have 18 m", "Kontrakten kræver en nulemissionsbus" ...).

**"Fordel automatisk"** (`pro_buses::auto`; the day, or the week in the week's view): every block
without a bus gets the best bus that can take it, the blocks in the order they begin - one already
on the line that day first, then the size asked for, an electric bus for a short block (at most
60 % of its range, `SHORT_SHARE`) and a diesel one for a long one, no service due, the fewest
kilometres that day so far. A block the planner left without a bus on purpose ("Fjern") is not
filled. "Fordel alt igen" (asked first) does every block again, the hand's choices too.

**Undo:** every act answers with the blocks' choices before it (`undo`), and the tab's note at the
bottom ("Bus 211 på vognløb 7/03." with "Fortryd") puts them back (`act: "restore"`).

**A bus that falls out** (`downs_of`, `tell`, `clock::What::BusDown`): a bus that did not start this
morning (`plan::disruptions`) or is in the workshop (a repair; its service, which the day's close
sends) leaves its blocks without a bus - the central still covers one that came suddenly with a
free bus, as before. With the morning (half an hour before the first block) the player is told
once a day for each such bus: "Bus 104 startede ikke i morges – vognløb 5/01 mangler en bus",
"Bus 104 er på værksted – vognløb 5/01 mangler en bus", or "… – centralen satte bus 211 på
vognløb 5/01", in the feed and as a notice like a sick call (`Ask::BusDown`, waiting while the
game's clock leads) with "Senere" and "Find bus ›" (the dispatch opens on that block's popup).
A bus that breaks down on its way keeps the existing choice of a spare or a rental bus for its
trips (`clock::Broken`); the dispatch shows it on the bus's row.

**Multiplayer:** `rf_buses` and `rf_bus_pick` need RF_VIEW, `rf_bus_act` the planning (PLANS);
the commands are routed in `rosterflow::run` (tested through it).

# Round J (7 November 2026): the Vagter tab made stronger

The player chose to keep the game's blocks and make the Vagter tab better (not to build blocks
from the timetable's trips). Code: `co::pro_dutyplan` (the figures, the price, the checklist, the
game's plan, the short names), `co::pro_duties::auto` and `Goals`, `rosterflow::optimise_aimed`,
`omsihub/rosterflow/pro.rs` (`add_day`), `launcher/ui/js/omsihub/rosterflow/pro.js`.

## The day in figures

Over the duties a bar shows the day: the duties, the paid hours, the driving's share of the paid
time (every trip, with passengers or not), the price of the day in the company's currency, the
average duty, and how many duties keep every rule (✓), break a soft one (!) or a hard one (✗).
Each figure has its difference from **the game's own plan**: the same day with nothing built on
it - the timetable's blocks, the duties RosterFlow makes of them by itself
(`pro_dutyplan::game_plan`). Green is better (fewer duties, fewer hours, more driving, cheaper,
fewer red), orange worse; under it "Spillets plan: 44 vagter, 337 t, 23 røde".

## The duty's break, price and rules

The table has two more columns. **Pause** is the duty's meal break ("30 min · 08:10"), a split
duty's gap ("delt 3:40"), an unpaid gap of a whole duty ("ubetalt 0:49"), or "–". The breaks a
driving period ends at are P blocks on the axis (the meal break stronger). **Pris** is what the
duty costs (`pro_dutyplan::price`): its paid hours at what an hour of the company's drivers costs
on average (the market's wage without drivers), the supplements of the evening, the night and the
weekend on the time it works (a public holiday as a Sunday), and the employer's on-costs. The
rules are a chip: "✓ alle", "! 1" (soft rules broken), "✗ 2" (hard ones).

## The checklist

A duty clicked shows beside the table (it stays in view while the list scrolls) "Vagt 1002 ·
Hel", where it signs on and off, its pieces and changes of bus, and each rule a line: the meal
break, driving without a break, the working day against the longest duty, overtime, the target
length (told, not a rule broken), the changes of bus (the time to change, a change of stops in a
hurry, too many changes), the shortest piece, a split duty's spread, rest between pieces, where it
begins and ends, its type - every issue of the duty in one of them or a line of its own. Under it
the price: wage × hours, supplements, the employer's part, the total.

## Goals for "Make duties automatically"

Under the rules: the target length (7:24 to 8:30), the longest duty (10:00), the shortest piece
(1:00; RosterFlow's own `min_piece`) and "Helst samme linje hele vagten" (on). Split duties stay
Monday to Friday. Saved with the company (`Prefs::goals`, `pro_duties::Goals`).

"Make duties automatically" now makes the day nine ways - the cost alone as before, and the goals
weighed lightly to strongly, with and without one line all the duty
(`rosterflow::optimise_aimed`, `Aim`) - and keeps the one that scores best
(`pro_duties::score`): Optimise's weight (the cost, the soft rules, a hard rule as a bus a day),
Pro's red marks and a duty longer than the longest as dear as a hard rule, each minute past the
target twice a minute's wage. With the goals, no duty is made longer than the longest; a break
counts without the way to another stop (30 minutes with 10 of walking is no meal break); the
working-time rules' soft ones (overtime, a change of stops in a hurry) are weighed as the day's
issues weigh them. A block's last piece that would be shorter than the shortest piece and cannot
join the one before moves the cut before it earlier (`cut_block`, also for RosterFlow).

The duties are then numbered in the order they sign on, 1001 the first
(`pro_duties::number_by_sign_on`); an edit by hand keeps the numbers as before. The table lists
the duties in the order they sign on.

## Shorter names

A block is "131 · H02", not "131/131H02" (`pro_dutyplan::short_tour`); a piece "131 · H02 A". A
piece whose trips run other lines says so: "131 → NE2 🌙 · H09 C" (`piece_lines`; a night line by
its name or all its trips between 22:00 and 05:00).

## Compare with the game's plan

"⇄ Sammenlign med spillets plan" in the toolbar puts your duties and the game's plan side by side,
each with its figures and its duties (the game's read only). The list of blocks folds away with a
click on its heading; each viewer's choice is remembered in the browser.

## With the real timetable

Lines 131 and 153 of Ahlheim on a Monday (36 blocks, 316 trips): the game's plan has 44 duties,
337 paid hours, 23 red and 9 yellow. Before round J "Make duties automatically" made 45 duties,
352 hours, 7 red, 19 yellow, the longest 12:40, eight over ten hours. See FORK.md for the figures
after it.

# Round K (7 November 2026): the Afgange tab, a duty's own name, the route in the rota

The player asked for a place to see every departure of the day and build the duties from it, one
duty at a time, to name the duties, and to see in the rota which route a duty runs and whether
it is a split duty. RosterFlow Pro has eight tabs now: "3 Afgange" comes after Vagter, and the
tabs after it moved one number on (4 Turnus … 8 Busdisponering).

## The Afgange tab (`afgange.js`, `pro.rs` `departures_json`)

Every trip of the day's blocks in order of departure: its time, the line its displays show (a
night line run inside a day line's tour - NE2 in 131's - with its moon), its first and last stop,
its arrival, its block ("131 · H01") and the duty it is in (its name or number; "—" for a piece in
the pool). A stop drivers cannot change at is marked ◇. Filters: all or only those without a duty,
a line, a search box (stop, line, block or duty), and the runs without passengers (hidden unless
asked for or part of the duty being built).

## A duty built of departures (`pro_duties::build_duty`, `runs_of`, `run_of`)

A click puts a departure into "Duty being built" (again: out). On each bus the duty takes the
trips from its first departure chosen to its last, and the runs without passengers just before
and after them (the pull-out, the run empty to the first trip, the pull-in) - `run_of`. Two runs
at the same time are refused (`BUILD_OVERLAP`). The panel shows the duty as it would be - asked of
the Rust side on a copy of the day, nothing kept (`pro_build_preview`): its sign-on and sign-off,
pay, type, break, the round J checklist and its price - and takes a name.

"Save duty" (`pro_build`) cuts each bus before and after its run at whatever trip
(`studio::set_cuts_free`: a relief point or not), takes the pieces out of the duties and the pool
they were in (a duty that had them keeps the rest; a piece a new cut leaves after the run waits in
the pool, as a cut in Vagter does) and makes them one duty (a joined duty when it has pieces of
several buses). It is an edit of the draft: Undo, Save and Discard as in Vagter.

A piece begun or ended where drivers may not change (a relief point turned off, `relief_ok`) is
no longer refused with RosterFlow Pro: the duty gets a yellow mark (`M_NOT_RELIEF_ON`,
`M_NOT_RELIEF_OFF`, in the checklist under "Where it begins and ends"), and the bus's own issue
(`I_RELIEF_PLACE`) is yellow instead of red. Without Pro it stays red.

## A duty's own name (`pro_duties::DutyName`, `set_name`, `name_of`)

A duty can be given a name of at most 12 characters ("M131-1") on its checklist card in Vagter or
in Afgange; no other duty of the day may have it (whatever its case), and an empty name gives the
number back. The name is kept by the duty's first piece (`Sheet::names`), so it follows the duty
however its pieces and number change; it goes with a copy of a day's plan, and a name whose piece
is gone goes with it. It is shown instead of the number everywhere the duty is: Vagter, Afgange,
the rota's cells and pickers, the calendar, the daily operations (the open duties, the duty's
popup), and the messages about a duty (an ill driver's call, the reserve told, "the duty is
taken", a player asked). Internally the duty keeps its number (`DutyNumber`): the rota, the
calls and the asks name it by that. `ProDuty::label` / `shown()`, `pro_duties::label_of` (with the
timetable) and `shown_number` (without it, for messages).

## The route in the rota (`ProDuty::lines`, `from_stop`, `to_stop`)

Each duty of the rota - a cell, the duty picker, the exceptions' list - shows the lines it runs
under its time ("131, 153", "131, NE2 🌙") and a small "delt" mark when it is a split duty. Hovering
shows its times, its route (first stop → last stop), the lines and the type.

## Round L: a reserve's frame is a window, their week a contract (NexiconDK, 7 November 2026)

A reserve's frame ("RAM1 03:00–15:00") is only the window a duty may come in; it is not their
hours. Before, a frame of five days of 12 hours made a 60-hour contract week and paid 12 hours a
day whether used or not.

- **Their week is a contract** (`Frame::contract`, minutes a week; `Frame::contract_minutes`):
  full time (37 hours at the Danish price level, else 39) or fewer hours, chosen when hired or
  when the employment changes. A file of before has none: the window's week, at most a full one -
  a 60-hour frame week is a full week, a frame week under it that many hours (part time).
  `Employment::week_hours` is the contract: the calendar's "31,8 t af 37", the staff card, the
  week's price (`pro_cost::week`), the price of a duty outside the frame (`pro_ops::price_of`: as a
  permanent driver's, overtime above the contract week) and a reserve rota's target hours
  (`pro_turnus::target_hours`: the drivers' contract weeks) all take it.
- **A frame day pays the hours driven, at least 7:24** (`pay::frame_day_minutes`, `frame_least`:
  a part-timer's contract week shared over the frame's days when that is less - 30 hours over five
  days at least 6:00 -, a window shorter than that at least the window): the reserve's own frame and every frame of a rota,
  named (round F) or of its own times, by one rule. The contract week is guaranteed as a
  permanent driver's (a seventh a day there, the rest paid at the week's end); overtime above it.
- **A named frame** (`Frame::ram`): a reserve hired in RAM1 has RAM1's times, and follows when
  RAM1's times change (`pro_turnus::bring_reserves`); a deleted named frame leaves them its times
  as a frame of their own. The staff card shows "Reserve · RAM1 03:00–15:00 Mon–Fri · 37 h a week".
- **The hiring window and the change of employment** (`people.js` `ReserveFields`, `people.rs`
  `employment_of`): Reserve shows the frame (RAM1 … RAM4 with their times, or "Own frame" with
  from and to), the weekdays, the contract (Full time 37 h / Fewer hours with the hours) and the
  line "The frame is the window a duty may come in - paid the hours driven, at least 7:24 a day".
  With a reserve rota there, "Put on a rota" puts them on its first free row, or a new row
  (`pro_turnus::place`); a change that applies later is put on it on the Turnus page once it
  applies (the page says so).

## Round M: the answers in the day, a busy answer's duty, the other weekdays (NexiconDK, 7 November 2026)

- **The day's mark by duty** (`pro_ops::mark_of`, `Mark::yes_of`; `ops.rs` `mark_json`): the
  calendar's mark on a day with answers not seen says the yes answers by duty - "✓ 2 ja · 2003",
  "✓ 2 ja · 2003, 1 ja · 2005" (two on the cell, every one in the tooltip), the duty by its own
  name. It counts every yes to a duty with an answer not seen, the ones seen too: it counted only
  the yes answers not seen, so the planner who opened the box after the first yes saw "1 ja"
  when two had said it.
- **Who was asked, in the day's list** (`ops.rs` `asked_json`): each duty without a driver says
  "Spurgt 3 · ✓ 2 ja · ✗ 1 nej · 🕒 1 venter" (the parts not 0, the names in the tooltip), its
  button "Se svar ›" (green with a yes); the duties with a yes first, then those with noes, then
  those waiting.
- **A busy answer names the duty** (`Reply::No::own`, `pro_ops::words_by`): a driver who has
  another duty then (`Cannot::OnDuty` without a swap, the rest rules without a time he can) says
  which - "Jeg har allerede M131-1 den dag, beklager", by the duty's own name
  (`pro_duties::shown_number`); an answer of before, without it, says "Jeg har en anden vagt den
  dag, beklager" (`SAY_BUSY_ANY`). A swap's duty is named the same way. `%{own}` was left in the
  text. A test says every way to say every answer leaves no place unfilled in Danish.
- **Each weekday its plan** (`pro.rs` `slot_name`, `pro_duties::unplanned_peers`, `use_for`): the
  Vagter and Afgange tabs say the weekday they edit ("Dag: Mandag") - not "Mandag til fredag":
  RosterFlow keeps a plan a weekday, so "Lav vagter automatisk" on a Monday left Tuesday to
  Friday on the game's plan (and a split-duty rota with Monday's split duties only). While the
  other weekdays Monday to Friday have no plan of their own, a bar says so with "Brug også for
  tirsdag – fredag": a day of the same timetable (every trip matched, none left over:
  `plandays::copy`) gets the plan copied as it is, its duties' names and pool with it; a day of
  another timetable (Friday's night tours) gets its duties made automatically on its own
  timetable (`auto`). Each is numbered in the order its duties sign on; it is the draft's, so
  SAVE keeps it.

## Round N: the lines' service inside Pro, the Roster tab that cannot be shown (NexiconDK, 7 November 2026)

The player: "Har jeg ingen mulighed for at Start Service i RosterFlow Pro? Det er der i den normale
RosterFlow", and later "Den vil først starte dem fra i morgen - jeg vil have 131 i drift NU".

- **The lines' strip** (`roster::lines_view`, `rf_roster_lines`; `pro_service.js`): the Vognløb
  tab and Daglig drift show every line of the company with its state today - "Ikke i drift", "I
  drift fra søn 8 nov 00:00" (a start to come), "I drift siden …" - and how many of its tours are
  planned, with "Start drift…" (the Roster tab's dialog: now, from tomorrow, from a date) or the
  stop button. A line won in a tender whose concession has begun but that does not run yet (or
  runs only later) is said in a banner on every Pro tab that shows the strip.
- **Starting now** moves a start set for later to now (`network::start_service` takes the moment
  asked, never before its concession begins): a line the player set for tomorrow is started today
  with "Nu". Tested with the player's own company file and through the relay of Bus Company
  Online (`company::relayed`): the start lands at the moment chosen.
- **The Roster tab** (normal RosterFlow's "Vagtplan") shows the words of a refusal - the company's
  launcher out of reach, a role that may not - with "Try again", instead of a page that broke on
  the toolbar a refusal does not carry (`roster.js`).

## Tests

`pro_duties_tests`: a duty built of departures cut out of its buses and named, a duty of two buses,
the overlap refused; a name short and unique, kept after a renumbering, taken away; a duty begun
where no relief point is marked yellow (the bus's issue yellow).

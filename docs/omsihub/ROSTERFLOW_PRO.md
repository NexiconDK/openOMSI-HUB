# RosterFlow Pro

RosterFlow Pro is the third planning tool of a bus company, next to the standard planning and
RosterFlow. The approved plan ("Rosteren – plan") gives it five tabs: rosters, a calendar of
what every driver does, duties, asking drivers, and frames. It is built in steps. This page
describes step 1, "Ansættelse og rammer" (employment and frames), done on 4 November 2026
(NexiconDK).

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

## Not in this step

Players of "Play together" as reserves, the drivers' answers ("too early", "too late"), sign-on
times and driver numbers in the calendar, and the cost of overtime when asking a full-time
driver come in later steps.

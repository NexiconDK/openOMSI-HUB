use super::super::market::{self, MarketBus, Payment};
use super::super::model::{CompanyLine, GameMode, Licence};
use super::super::network::tests::trip;
use super::super::network::TourOfDay;
use super::super::plan::{day_plan, disruptions, set_bus, set_driver, weekday_of};
use super::super::staff::{applicants, hire};
use super::super::together::book::Claim;
use super::super::{found, Founding};
use super::*;

fn tour(no: &str, from: i32, n: i32) -> TourOfDay {
    TourOfDay { line: "Linie5".into(), number: "5".into(), tour: no.into(), ai_group: String::new(), trips: (0..n).map(|k| trip(from + k * 60, from + 50 + k * 60, 12)).collect(), unplanned: false, cuts: None }
}

/// A company with line 5, `buses` buses and `people` drivers, nobody away, no disruption of the
/// day, the dispatcher's own filling off (only the roster drives).
fn company(buses: usize, people: usize) -> Company {
    let mut c = found(&Founding { name: "Crew".into(), game_mode: GameMode::Normal, date: "2024-03-04".into(), map: "maps/Grundorf/global.cfg".into(), ..Default::default() }, "Luc");
    c.lines.push(CompanyLine { name: "Linie5".into(), number: "5".into(), numbers: vec!["5".into()], added: c.date.clone(), service_from: Some(0), ..Default::default() });
    c.planning.auto = false;
    c.cash += 5_000_000_00;
    let bus = MarketBus { file: "Vehicles/Citaro/Citaro.bus".into(), name: "Citaro".into(), ..Default::default() };
    for _ in 0..buses {
        market::buy_new(&mut c, &bus, Payment::Cash, "").unwrap();
    }
    let id = c.id.clone();
    while c.staff.len() < people {
        for a in applicants(&c) {
            if c.staff.len() < people && a.licence == Licence::D {
                hire(&mut c, &a).unwrap();
            }
        }
        c.id.push('x');
        c.taken = Default::default();
    }
    for e in c.staff.iter_mut() {
        (e.sick_until, e.holiday_until, e.training_until, e.last_end) = (None, None, None, None);
        e.leave = Default::default();
        e.endorsements = super::super::licences::Endorsement::ALL.to_vec();
        // (willing: the swaps' own chance says yes)
        (e.friendliness, e.satisfaction) = (100.0, 100.0);
    }
    for n in 0.. {
        c.id = format!("{id}{n}");
        if disruptions(&c, &c.date).is_empty() {
            break;
        }
    }
    c.clock.minute = 5 * 60;
    c
}

/// The player on tour 2 (06:00-08:50) with bus 1, the first colleague on tour 3 (09:00-11:50)
/// with bus 2.
fn rostered() -> (Company, Vec<TourOfDay>) {
    let mut c = company(2, 3);
    let wd = weekday_of(&c.date);
    let (b1, b2, e1) = (c.fleet[0].id, c.fleet[1].id, c.staff[0].id);
    set_bus(&mut c, wd, "Linie5", "2", Some(b1));
    set_bus(&mut c, wd, "Linie5", "3", Some(b2));
    set_driver(&mut c, wd, "Linie5", "2", 0, Some(Who::Player));
    set_driver(&mut c, wd, "Linie5", "3", 0, Some(Who::Staff(e1)));
    (c, vec![tour("2", 6 * 60, 3), tour("3", 9 * 60, 3)])
}

/// The company's day of the tests.
const DAY: &str = "2024-03-04";

fn duty(t: &str) -> DutyRef {
    DutyRef { date: DAY.into(), line: "Linie5".into(), tour: t.into(), duty: 0 }
}

fn who_on(p: &DayPlan, t: &str) -> Option<Who> {
    p.tours.iter().find(|x| x.tour.tour == t).and_then(|x| x.duties[0].who)
}

#[test]
fn a_handover_note_is_left_on_the_bus_for_the_day() {
    let mut c = company(1, 0);
    let bus = c.fleet[0].id;
    let id = add_note(&mut c, bus, "Luc", Some(NoteTemplate::DoorSticks), "rear door").unwrap();
    add_note(&mut c, bus, "Luc", None, "Heating\u{7} on full\n").unwrap();
    let notes = notes_for(&c, bus);
    assert_eq!(notes.len(), 2);
    assert_eq!((notes[0].id, notes[0].says().as_str(), notes[0].by.as_str()), (id, "A door sticks: rear door", "Luc"));
    // (typed text is cut and cleaned)
    assert_eq!(notes[1].says(), "Heating on full");
    // the next driver hears of it
    let told = take_notifications(&mut c);
    assert_eq!(told.len(), 2);
    assert_eq!(told[0].template, NOTE_LEFT);
    assert!(told[0].args.iter().any(|(k, v)| k == "bus" && *v == c.fleet[0].number));
    assert!(take_notifications(&mut c).is_empty());
    // nothing to say, a bus not ours, too many: refused
    assert_eq!(add_note(&mut c, bus, "Luc", None, "   "), Err("The note is empty."));
    assert_eq!(add_note(&mut c, 999, "Luc", None, "x"), Err("This bus is not in the fleet."));
    for _ in 2..NOTES_MAX {
        add_note(&mut c, bus, "Luc", Some(NoteTemplate::Dirty), "").unwrap();
    }
    assert_eq!(add_note(&mut c, bus, "Luc", Some(NoteTemplate::Dirty), ""), Err("This bus has enough notes for today."));
    let long: String = "x".repeat(400);
    c.crew.notes.clear();
    add_note(&mut c, bus, "Luc", None, &long).unwrap();
    assert_eq!(notes_for(&c, bus)[0].text.chars().count(), TEXT_MAX);
    // the notes expire after their day
    night(&mut c, "2024-03-04");
    assert!(c.crew.notes.is_empty());
    // (a note of another day is not shown)
    c.crew.notes.push(Note { id: 99, vehicle: bus, date: "2024-03-01".into(), text: "old".into(), ..Default::default() });
    assert!(notes_for(&c, bus).is_empty());
}

#[test]
fn a_defect_becomes_a_workshop_job_and_a_critical_one_takes_the_bus_off_the_road() {
    let (mut c, tours) = rostered();
    let (b1, b2) = (c.fleet[0].id, c.fleet[1].id);
    // a job waiting already for bus 2: a critical defect's repair goes before it
    let other = depot::order(&mut c, b2, JobKind::Service).unwrap();
    // the ticket machine: a repair ordered, the bus still runs
    let d = report_defect(&mut c, b1, DefectKind::TicketMachine, "prints nothing", "Luc", false).unwrap();
    let job = c.crew.defects.iter().find(|x| x.id == d).unwrap().job.unwrap();
    let j = c.site.jobs.iter().find(|j| j.id == job).unwrap();
    assert_eq!((j.vehicle, j.kind, j.started.is_some()), (b1, JobKind::Repair, false));
    assert!(!c.fleet[0].in_workshop(&c.date));
    assert_eq!(who_on(&day_plan(&c, &c.date, tours.clone(), &[], &[], false), "2"), Some(Who::Player));
    // the doors: the same job, the bus off the road today, the repair first in the queue
    let d2 = report_defect(&mut c, b1, DefectKind::Doors, "", "Luc", false).unwrap();
    let x = c.crew.defects.iter().find(|x| x.id == d2).unwrap();
    assert_eq!((x.job, x.held), (Some(job), true));
    assert_eq!(c.site.jobs.iter().filter(|j| j.vehicle == b1).count(), 1);
    assert!(c.fleet[0].in_workshop(&c.date));
    assert_eq!(c.site.jobs[0].id, job);
    assert_eq!(c.site.jobs[1].id, other);
    let p = day_plan(&c, &c.date, tours, &[], &[], false);
    let t2 = p.tours.iter().find(|t| t.tour.tour == "2").unwrap();
    assert_eq!((t2.bus, t2.bus_problem), (None, Some(Problem::Workshop)));
    // the same defect again that day is the first report
    assert_eq!(report_defect(&mut c, b1, DefectKind::Doors, "again", "Luc", false), Ok(d2));
    // the workshop and the driver are told
    let told = take_notifications(&mut c);
    assert_eq!(told.iter().filter(|n| n.template == DEFECT_LOGGED).count(), 2);
    assert_eq!(told.last().unwrap().priority, Priority::High);
    // open while its job is; the Fleet page's list, the latest first
    assert!(defects_of(&c, b1).iter().all(|d| d.open(&c)));
    assert_eq!(defects_of(&c, b1)[0].kind, DefectKind::Doors);
    c.site.jobs.retain(|j| j.id != job);
    assert!(defects_of(&c, b1).iter().all(|d| !d.open(&c)));
    // kept a month after its day, then let go
    night(&mut c, "2024-03-20");
    assert_eq!(c.crew.defects.len(), 2);
    night(&mut c, "2024-04-10");
    assert!(c.crew.defects.is_empty());
    // no cash for the repair: the defect is kept, no job yet
    c.cash = -1_000_000_00;
    let d3 = report_defect(&mut c, b2, DefectKind::Tyres, "", "Luc", false).unwrap();
    assert_eq!(c.crew.defects.iter().find(|x| x.id == d3).unwrap().job, Some(other));
    assert_eq!(report_defect(&mut c, 999, DefectKind::Other, "", "Luc", false), Err("This bus is not in the fleet."));
}

#[test]
fn a_breakdown_of_the_company_s_model_is_logged_with_the_bus() {
    let mut c = company(1, 0);
    let bus = c.fleet[0].id;
    log_fault(&mut c, bus, BREAKDOWN, "2024-03-05");
    let d = &defects_of(&c, bus)[0];
    assert!(d.fault && d.held && d.job.is_none());
    assert!(d.open(&c));
    c.date = "2024-03-06".into();
    assert!(!defects_of(&c, bus)[0].open(&c));
}

#[test]
fn a_colleague_of_the_staff_answers_a_swap_by_the_roster_rules() {
    let (mut c, tours) = rostered();
    let e1 = c.staff[0].id;
    let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    assert_eq!((who_on(&p, "2"), who_on(&p, "3")), (Some(Who::Player), Some(Who::Staff(e1))));
    // a duty that is not his, or none at all
    assert_eq!(ask_swap(&mut c, &p, Who::Player, &duty("3"), Who::Staff(e1)).unwrap_err(), NOT_YOURS);
    assert_eq!(ask_swap(&mut c, &p, Who::Player, &duty("9"), Who::Staff(e1)).unwrap_err(), NO_DUTY);
    // accepted: for that day the plan has them swapped, the roster stays
    let s = ask_swap(&mut c, &p, Who::Player, &duty("2"), Who::Staff(e1)).unwrap();
    assert_eq!((s.state, s.theirs.as_ref().map(|t| t.tour.as_str())), (SwapState::Accepted, Some("3")));
    let after = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    assert_eq!((who_on(&after, "2"), who_on(&after, "3")), (Some(Who::Staff(e1)), Some(Who::Player)));
    let wd = weekday_of(&c.date);
    assert_eq!(plan::roster(&c, wd, "Linie5", "2").unwrap().duties[0], Some(Who::Player));
    let next_week = dates::add(&c.date, 7);
    assert_eq!(who_on(&day_plan(&c, &next_week, tours.clone(), &[], &[], false), "2"), Some(Who::Player));
    // the strip says so; asked again, refused
    let ti = after.tours.iter().position(|t| t.tour.tour == "3").unwrap();
    assert_eq!(changes(&c, &after, ti, 0), vec![Change::Swapped { with: c.staff[0].name.clone() }]);
    let e2 = c.staff[1].id;
    assert_eq!(ask_swap(&mut c, &p, Who::Player, &duty("2"), Who::Staff(e2)).unwrap_err(), ASKED_ALREADY);
    let told = take_notifications(&mut c);
    assert_eq!((told[0].template.as_str(), told[0].to), (SWAP_ACCEPTED, Some(Who::Player)));
}

#[test]
fn a_colleague_says_no_when_the_rules_say_no() {
    let (base, tours) = rostered();
    let (e1, e2) = (base.staff[0].id, base.staff[1].id);
    let p = day_plan(&base, &base.date, tours.clone(), &[], &[], false);
    let ask = |c: &mut Company, with: u32| {
        let p = day_plan(c, &c.date, tours.clone(), &[], &[], false);
        let s = ask_swap(c, &p, Who::Player, &duty("2"), Who::Staff(with)).unwrap();
        (s.state, s.why)
    };
    // ill that day
    let mut c = base.clone();
    c.staff[1].sick_until = Some(c.date.clone());
    assert_eq!(ask(&mut c, e2), (SwapState::Refused, OFF_THAT_DAY.to_string()));
    // a licence for small buses only
    let mut c = base.clone();
    c.staff[1].licence = Licence::D1;
    assert_eq!(ask(&mut c, e2), (SwapState::Refused, CANNOT_DRIVE.to_string()));
    // two duties that day
    let mut c = base.clone();
    let wd = weekday_of(&c.date);
    let mut three = tours.clone();
    three.push(tour("4", 15 * 60, 2));
    let b1 = c.fleet[0].id;
    set_bus(&mut c, wd, "Linie5", "4", Some(b1));
    set_driver(&mut c, wd, "Linie5", "4", 0, Some(Who::Staff(e1)));
    let p3 = day_plan(&c, &c.date, three.clone(), &[], &[], false);
    let s = ask_swap(&mut c, &p3, Who::Player, &duty("2"), Who::Staff(e1)).unwrap();
    assert_eq!((s.state, s.why.as_str()), (SwapState::Refused, TWO_DUTIES));
    // their duty clashes with the player's other work
    let mut c = base.clone();
    let mut clash = tours.clone();
    clash.push(tour("5", 9 * 60 + 30, 1));
    set_bus(&mut c, wd, "Linie5", "5", Some(b1));
    set_driver(&mut c, wd, "Linie5", "5", 0, Some(Who::Player));
    let p5 = day_plan(&c, &c.date, clash, &[], &[], false);
    let s = ask_swap(&mut c, &p5, Who::Player, &duty("2"), Who::Staff(e1)).unwrap();
    assert_eq!((s.state, s.why.as_str()), (SwapState::Refused, YOU_CANNOT));
    // a free colleague takes the duty over (no duty of theirs in return)
    let mut c = base.clone();
    let s = ask_swap(&mut c, &p, Who::Player, &duty("2"), Who::Staff(e2)).unwrap();
    assert!(s.state == SwapState::Refused && s.why == RATHER_NOT || (s.state == SwapState::Accepted && s.theirs.is_none()), "{s:?}");
    // begun already
    let mut c = base.clone();
    c.clock.minute = 6 * 60 + 5;
    assert_eq!(ask_swap(&mut c, &p, Who::Player, &duty("2"), Who::Staff(e1)).unwrap_err(), BEGUN);
    // nobody refused is planned other than before
    assert_eq!(who_on(&day_plan(&base, &base.date, tours, &[], &[], false), "2"), Some(Who::Player));
}

#[test]
fn two_players_of_the_session_swap_the_duties_they_claimed() {
    let (mut c, tours) = rostered();
    let wd = weekday_of(&c.date);
    set_driver(&mut c, wd, "Linie5", "2", 0, None);
    set_driver(&mut c, wd, "Linie5", "3", 0, None);
    let a = c.together.guest_of("id-a", "Anna").unwrap();
    let b = c.together.guest_of("id-b", "Ben").unwrap();
    let claim = |r: DutyRef, g: u32| Claim { duty: r, guest: g, state: ClaimState::Planned, ..Default::default() };
    c.together.claims.push(claim(duty("2"), a));
    c.together.claims.push(claim(duty("3"), b));
    let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    assert_eq!((who_on(&p, "2"), who_on(&p, "3")), (Some(Who::Guest(a)), Some(Who::Guest(b))));
    // a player asks the staff or the boss: not between players
    let e1 = c.staff[0].id;
    assert_eq!(ask_swap(&mut c, &p, Who::Guest(a), &duty("2"), Who::Staff(e1)).unwrap_err(), MIXED);
    let s = ask_swap(&mut c, &p, Who::Guest(a), &duty("2"), Who::Guest(b)).unwrap();
    assert_eq!((s.state, s.theirs.as_ref().map(|t| t.tour.as_str())), (SwapState::Asked, Some("3")));
    assert_eq!(take_notifications(&mut c).last().unwrap().to, Some(Who::Guest(b)));
    // only the one asked answers
    assert_eq!(answer_swap(&mut c, &p, s.no, Who::Guest(a), true), Err(NOT_YOURS));
    assert_eq!(answer_swap(&mut c, &p, s.no, Who::Guest(b), true), Ok(SwapState::Accepted));
    let after = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    assert_eq!((who_on(&after, "2"), who_on(&after, "3")), (Some(Who::Guest(b)), Some(Who::Guest(a))));
    assert_eq!(answer_swap(&mut c, &p, s.no, Who::Guest(b), true), Err(NO_SWAP));
    // a no leaves the duties where they were
    let s = ask_swap(&mut c, &after, Who::Guest(a), &duty("3"), Who::Guest(b));
    assert_eq!(s.unwrap_err(), ASKED_ALREADY);
}

#[test]
fn the_players_holiday_is_answered_by_the_staffs_limit() {
    let (mut c, tours) = rostered();
    let today = c.date.clone();
    // today is a sick call; bad or too long days are refused
    assert_eq!(ask_leave(&mut c, Who::Player, &today, &today, Reason::Break), Err(SICK_TODAY));
    let (d7, d8) = (dates::add(&today, 7), dates::add(&today, 8));
    assert_eq!(ask_leave(&mut c, Who::Player, &d8, &d7, Reason::Break), Err(BAD_DAYS));
    assert_eq!(ask_leave(&mut c, Who::Player, &d7, &dates::add(&today, 40), Reason::Break), Err(TOO_LONG));
    assert_eq!(ask_leave(&mut c, Who::Player, "soon", &d7, Reason::Break), Err(BAD_DAYS));
    // within the limit: approved, and his duty of that day open for the dispatcher
    let (no, yes) = ask_leave(&mut c, Who::Player, &d7, &d8, Reason::Break).unwrap();
    assert!(yes);
    assert_eq!(c.crew.leave.approved_on(&d7).map(|r| r.no), Some(no));
    let p = day_plan(&c, &d7, tours.clone(), &[], &[], false);
    let t2 = p.tours.iter().find(|t| t.tour.tour == "2").unwrap();
    // (covered by a free colleague, as a driver's holiday known ahead is)
    assert_eq!(t2.duties[0].problem, Some(Problem::Holiday));
    assert!(matches!(t2.duties[0].who, Some(Who::Staff(_)) | Some(Who::Agency)));
    assert_eq!(player_off(&c, &d7), Some(Problem::Holiday));
    assert_eq!(player_off(&c, &today), None);
    assert!(c.inbox.messages.iter().any(|m| m.title == LEAVE_YES));
    // the same days again
    assert_eq!(ask_leave(&mut c, Who::Player, &d8, &d8, Reason::Personal), Err(HAVE_THOSE));
    // the limit reached by the staff on holiday: rejected; a date that cannot move goes one over
    holidays::set_limit(&mut c, Some(1));
    let d14 = dates::add(&today, 14);
    c.staff[0].holiday_until = None;
    c.staff[0].leave.requests.push(Request { no: 500, from: d14.clone(), until: d14.clone(), state: State::Approved, ..Default::default() });
    assert_eq!(ask_leave(&mut c, Who::Player, &d14, &d14, Reason::Break).map(|x| x.1), Ok(false));
    assert!(c.inbox.messages.iter().any(|m| m.title == LEAVE_NO));
    let d15 = dates::add(&today, 15);
    c.staff[0].leave.requests.push(Request { no: 501, from: d15.clone(), until: d15.clone(), state: State::Approved, ..Default::default() });
    assert_eq!(ask_leave(&mut c, Who::Player, &d15, &d15, Reason::Family).map(|x| x.1), Ok(true));
    // the year's days used up
    c.crew.leave.requests.push(Request { no: 502, from: "2024-01-08".into(), until: "2024-02-28".into(), state: State::Approved, ..Default::default() });
    assert_eq!(ask_leave(&mut c, Who::Player, &dates::add(&today, 20), &dates::add(&today, 20), Reason::Break), Err(NO_DAYS));
}

#[test]
fn a_sick_call_opens_the_players_duties_for_the_dispatcher() {
    let (mut c, tours) = rostered();
    let today = c.date.clone();
    assert_eq!(call_in_sick(&mut c, Who::Player, 0), Err(BAD_DAYS));
    assert_eq!(call_in_sick(&mut c, Who::Player, SICK_MAX + 1), Err(BAD_DAYS));
    assert_eq!(call_in_sick(&mut c, Who::Player, 2), Ok(dates::add(&today, 1)));
    assert_eq!(call_in_sick(&mut c, Who::Player, 1), Err(ILL_ALREADY));
    // his duty open as an ill driver's: on the list of what fell out, which the central fills
    // by itself (a sudden problem) and the dispatcher can cover otherwise
    let p = day_plan(&c, &today, tours.clone(), &[], &[], false);
    let t2 = p.tours.iter().find(|t| t.tour.tour == "2").unwrap();
    assert_eq!(t2.duties[0].problem, Some(Problem::Sick));
    assert_ne!(t2.duties[0].who, Some(Who::Player));
    assert!(p.open_rows().iter().any(|r| r.problem == Some(Problem::Sick)));
    // (tomorrow too, the day after not)
    let tomorrow = day_plan(&c, &dates::add(&today, 1), tours.clone(), &[], &[], false);
    assert_ne!(who_on(&tomorrow, "2"), Some(Who::Player));
    assert!(c.inbox.messages.iter().any(|m| m.title == SICK_CALLED));
    assert_eq!(take_notifications(&mut c)[0].priority, Priority::High);
    // the day after next he is back; well again earlier, at once
    assert_eq!(player_off(&c, &dates::add(&today, 2)), None);
    night(&mut c, &dates::add(&today, 1));
    assert_eq!(c.crew.sick_until, None);
    call_in_sick(&mut c, Who::Player, 3).unwrap();
    assert!(back_at_work(&mut c));
    assert_eq!(player_off(&c, &today), None);
}

#[test]
fn a_player_of_the_session_who_calls_in_sick_gives_his_duties_back() {
    let (mut c, _) = rostered();
    let g = c.together.guest_of("id-g", "Greta").unwrap();
    let claim = |r: DutyRef, st: ClaimState| Claim { duty: r, guest: g, state: st, ..Default::default() };
    c.together.claims.push(claim(duty("2"), ClaimState::Planned));
    c.together.claims.push(claim(duty("3"), ClaimState::Driving));
    call_in_sick(&mut c, Who::Guest(g), 1).unwrap();
    // (the one under way stays: his game drives it)
    assert_eq!(c.together.claims.len(), 1);
    assert_eq!(c.together.claims[0].state, ClaimState::Driving);
    assert!(c.inbox.messages.iter().any(|m| m.title == GUEST_SICK));
    assert_eq!(c.crew.sick_until, None);
}

#[test]
fn every_crew_text_is_translated() {
    let mut missing = Vec::new();
    let tables: Vec<&str> = omsihub_tables();
    for t in texts() {
        if !tables.iter().any(|y| y.contains(&format!("\"{}\":", t.replace('"', "\\\"")))) {
            missing.push(t);
        }
    }
    assert!(missing.is_empty(), "not in crates/omsihub-locales/locales/crew.yml: {missing:?}");
}

/// The launcher's translation tables (the crew's are in `crew.yml`).
fn omsihub_tables() -> Vec<&'static str> {
    vec![include_str!("../../../omsihub-locales/locales/crew.yml")]
}

// --- round R (9 November 2026): swaps between players approved by the boss, their rules, wishes --

/// Two players of a company on Bus Company Online's server: Anna on tour 2 (06:00-08:50), Ben on
/// tour 3 (09:00-11:50).
fn on_server() -> (Company, Vec<TourOfDay>, u32, u32) {
    let (mut c, tours) = rostered();
    let wd = weekday_of(&c.date);
    set_driver(&mut c, wd, "Linie5", "2", 0, None);
    set_driver(&mut c, wd, "Linie5", "3", 0, None);
    c.online.on_server = true;
    let a = c.together.guest_of("id-a", "Anna").unwrap();
    let b = c.together.guest_of("id-b", "Ben").unwrap();
    let claim = |r: DutyRef, g: u32| Claim { duty: r, guest: g, state: ClaimState::Planned, ..Default::default() };
    c.together.claims.push(claim(duty("2"), a));
    c.together.claims.push(claim(duty("3"), b));
    (c, tours, a, b)
}

#[test]
fn on_the_server_a_swap_the_players_agreed_on_waits_for_the_boss() {
    let (mut c, tours, a, b) = on_server();
    let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    let s = ask_swap(&mut c, &p, Who::Guest(a), &duty("2"), Who::Guest(b)).unwrap();
    assert_eq!(answer_swap(&mut c, &p, s.no, Who::Guest(b), true), Ok(SwapState::Waiting));
    assert_eq!(waiting(&c).count(), 1);
    assert!(c.inbox.messages.iter().any(|m| m.title == INBOX_TITLE), "the boss is told");
    let still = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    assert_eq!((who_on(&still, "2"), who_on(&still, "3")), (Some(Who::Guest(a)), Some(Who::Guest(b))), "nothing changed before the approval");
    // a no: both keep theirs, both told
    let mut no = c.clone();
    take_notifications(&mut no);
    assert_eq!(decide_swap(&mut no, &p, s.no, false), Ok(SwapState::Refused));
    assert_eq!(take_notifications(&mut no).len(), 2);
    assert_eq!(who_on(&day_plan(&no, &no.date, tours.clone(), &[], &[], false), "2"), Some(Who::Guest(a)));
    // a yes: swapped
    assert_eq!(decide_swap(&mut c, &p, s.no, true), Ok(SwapState::Accepted));
    let after = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    assert_eq!((who_on(&after, "2"), who_on(&after, "3")), (Some(Who::Guest(b)), Some(Who::Guest(a))));
    assert_eq!(decide_swap(&mut c, &p, s.no, true), Err(NOT_WAITING));
    // begun before the boss decided: not made
    let (mut c, tours, a, b) = on_server();
    let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    let s = ask_swap(&mut c, &p, Who::Guest(a), &duty("2"), Who::Guest(b)).unwrap();
    answer_swap(&mut c, &p, s.no, Who::Guest(b), true).unwrap();
    c.clock.minute = 6 * 60 + 1;
    assert_eq!(decide_swap(&mut c, &p, s.no, true), Ok(SwapState::Refused));
    assert_eq!(c.crew.swaps.iter().find(|x| x.no == s.no).unwrap().why, BEGUN);
    // "swap without approval": made at once
    let (mut c, tours, a, b) = on_server();
    c.together.swaps_free = true;
    let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    let s = ask_swap(&mut c, &p, Who::Guest(a), &duty("2"), Who::Guest(b)).unwrap();
    assert_eq!(answer_swap(&mut c, &p, s.no, Who::Guest(b), true), Ok(SwapState::Accepted));
    assert_eq!(who_on(&day_plan(&c, &c.date, tours, &[], &[], false), "2"), Some(Who::Guest(b)));
}

#[test]
fn a_swap_between_players_keeps_the_rest_the_days_in_a_row_and_the_split_rules() {
    use super::super::player_wish::WorkDay;
    let yesterday = super::super::dates::add(DAY, -1);
    // Ben worked until 23:00 yesterday: Anna's 06:00 duty would leave him 7 hours
    let (mut c, tours, a, b) = on_server();
    c.together.days.push(WorkDay { guest: b, date: yesterday.clone(), from: 15 * 60, to: 23 * 60, minutes: 8 * 60 });
    let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    assert_eq!(ask_swap(&mut c, &p, Who::Guest(a), &duty("2"), Who::Guest(b)).unwrap_err(), REST_SHORT);
    // ... with the company's least rest at 9 hours it would keep (7 h still too little) - 06:00
    // after 21:00 is 9 hours
    c.together.days[0].to = 21 * 60;
    super::super::pro_watch::set_rest_min(&mut c, 9 * 60).unwrap();
    assert!(ask_swap(&mut c, &p, Who::Guest(a), &duty("2"), Who::Guest(b)).is_ok());
    // six days in a row before it: a seventh is too many (the most: 6)
    let (mut c, tours, a, b) = on_server();
    for n in 1..=6 {
        c.together.days.push(WorkDay { guest: b, date: super::super::dates::add(DAY, -n), from: 9 * 60, to: 12 * 60, minutes: 180 });
    }
    let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    assert_eq!(ask_swap(&mut c, &p, Who::Guest(a), &duty("2"), Who::Guest(b)).unwrap_err(), TOO_MANY_DAYS);
    // a split duty: Monday to Friday only, never with night-bus work, at most 12:30
    let (c, mut tours, a, _) = on_server();
    tours.push(tour("4", 12 * 60, 3));
    tours.push(tour("9", 23 * 60, 2));
    let mut p = day_plan(&c, &c.date, tours, &[], &[], false);
    let at = |p: &DayPlan, t: &str| (p.tours.iter().position(|x| x.tour.tour == t).unwrap(), 0);
    let (t2, t4, t9) = (at(&p, "2"), at(&p, "4"), at(&p, "9"));
    assert_eq!(player_rules(&c, &p, a, &[t2, t4]), Ok(()), "a split duty on a Monday");
    assert_eq!(player_rules(&c, &p, a, &[t2, t9]), Err(SPLIT_NIGHT));
    assert_eq!(player_rules(&c, &p, a, &[t2, at(&p, "3")]), Err(YOU_CANNOT), "08:50 and 09:00: no rest between");
    p.weekday = 5;
    assert_eq!(player_rules(&c, &p, a, &[t2, t4]), Err(SPLIT_WEEKEND));
    assert_eq!(player_rules(&c, &p, a, &[t4]), Ok(()), "one duty is no split");
}

#[test]
fn a_player_names_the_duty_he_would_drive_instead() {
    let (mut c, mut tours, a, b) = on_server();
    tours.push(tour("4", 13 * 60, 2));
    // (Ben's 13:00 duty listed first: his first, unless another is named)
    c.together.claims.insert(0, Claim { duty: duty("4"), guest: b, state: ClaimState::Planned, ..Default::default() });
    let p = day_plan(&c, &c.date, tours, &[], &[], false);
    let s = ask_swap_for(&mut c, &p, Who::Guest(a), &duty("2"), Who::Guest(b), Some(duty("3")), false).unwrap();
    assert_eq!(s.theirs.as_ref().map(|t| t.tour.as_str()), Some("3"));
    assert_eq!(ask_swap_for(&mut c, &p, Who::Guest(b), &duty("4"), Who::Guest(a), Some(duty("4")), false).unwrap_err(), NOT_YOURS, "not one of the colleague's");
    // offered for nothing in return: the colleague takes it (Anna then drives 06:00 and 13:00, a
    // split duty on a Monday: allowed)
    assert_eq!(ask_swap_for(&mut c, &p, Who::Guest(b), &duty("4"), Who::Guest(a), None, true).map(|s| s.theirs), Ok(None));
}

#[test]
fn a_players_wishes_are_told_where_a_duty_does_not_keep_them() {
    use super::super::player_wish::{self as pw, Kind, Miss, PlayerWish};
    let (mut c, mut tours, a, b) = on_server();
    tours.push(tour("9", 23 * 60, 2));
    // checked: a kind both wished and avoided, too many hours, a date past
    let bad = |w: PlayerWish| w;
    let mut cc = c.clone();
    assert_eq!(pw::set(&mut cc, a, bad(PlayerWish { prefer: vec![Kind::Early], avoid: vec![Kind::Early], ..Default::default() })), Err(pw::BOTH));
    assert_eq!(pw::set(&mut cc, a, PlayerWish { max_week: Some(70 * 60), ..Default::default() }), Err(pw::BAD_HOURS));
    assert_eq!(pw::set(&mut cc, a, PlayerWish { dates: vec!["2024-03-01".into()], ..Default::default() }), Err(pw::BAD_DAY));
    assert_eq!(pw::set(&mut cc, 99, PlayerWish { off: vec![0], ..Default::default() }), Err(pw::NO_PLAYER));
    // Anna would rather drive day duties, avoids nights, wishes Wednesdays and 6 March off, 10 h a week
    pw::set(&mut c, a, PlayerWish { prefer: vec![Kind::Day], avoid: vec![Kind::Night], off: vec![2], dates: vec!["2024-03-06".into()], max_week: Some(10 * 60), ..Default::default() }).unwrap();
    // the date off said in RosterFlow Pro's "Kan arbejde" too
    assert_eq!(super::super::pro_avail::guest_can(&c, a, "2024-03-06"), Some(super::super::pro_avail::Can::Not));
    let p = day_plan(&c, &c.date, tours.clone(), &[], &[], false);
    let at = |t: &str| (p.tours.iter().position(|x| x.tour.tour == t).unwrap(), 0);
    // her own 06:00 duty: early, not a day duty - one wish not kept
    let (t2, k2) = at("2");
    assert_eq!(pw::misses_of(&c, &p, a, t2, k2), vec![Miss::NotWished(vec![Kind::Day])]);
    // Ben's 09:00 duty would keep them (a day duty) - but beside her own it makes a split one,
    // with too little rest between: only the day kind counts here, as it is
    let (t3, k3) = at("3");
    assert!(pw::misses_of(&c, &p, a, t3, k3).iter().all(|m| !matches!(m, Miss::Avoids(_))));
    // the night tour: avoided, and not a day duty
    let (t9, k9) = at("9");
    assert!(pw::misses_of(&c, &p, a, t9, k9).contains(&Miss::Avoids(Kind::Night)));
    // Ben said nothing: nothing to tell
    assert!(pw::misses_of(&c, &p, b, t9, k9).is_empty());
    // the hours of her week: 9:30 driven since Monday (none - it is Monday) - the 10 hours kept
    assert!(!pw::misses_of(&c, &p, a, t2, k2).iter().any(|m| matches!(m, Miss::Hours { .. })));
    // on a Wednesday: the weekday off wished
    let wed = pw::misses(&c, a, "2024-03-13", 9 * 60, false, false, 180, 0);
    assert_eq!(wed, vec![Miss::DayOff(Some(2))]);
    // and the hours of the week past the most
    let heavy = pw::misses(&c, a, "2024-03-05", 9 * 60, false, false, 180, 9 * 60);
    assert_eq!(heavy, vec![Miss::Hours { have: 12 * 60, max: 10 * 60 }]);
    // met: of her one duty of the day, none keeps them all
    assert_eq!(pw::met(&c, Some(&p)), (0, 1));
    // the snapshot tells whoever gives the night duty that Anna avoids it
    let mut w = PlayerWish::default();
    w.avoid.push(Kind::Night);
    pw::set(&mut c, a, w).unwrap();
    assert!(super::super::pro_avail::guest_can(&c, a, "2024-03-06").is_none(), "the date no longer wished, cleared in \"Kan arbejde\"");
    assert_eq!(pw::met(&c, Some(&p)), (1, 1), "her early duty keeps \"no nights\"");
}

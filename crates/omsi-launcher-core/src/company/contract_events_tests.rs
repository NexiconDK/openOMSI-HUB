//! (NexiconDK, 3 November 2026) The contracts' events H1-H3: a contract changed by the authority,
//! the server's calendar, roadworks.

use super::*;
use crate::company::bus_contracts::{Kind, Period};
use crate::company::concessions::tests::line;
use crate::company::{found, Founding};
use crate::service::{self, ServerCalendar};

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("omsihub-events-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn code(date: &str) -> i32 {
    omsi_map::date_code(date).unwrap()
}

/// A company of the server holding line `Linie5` ("5") on a two-part contract until the 30th:
/// ten timetable hours a day, 312 days, two contract buses.
fn holder() -> Company {
    let mut c = found(&Founding { name: "Holder".into(), date: "2026-11-03".into(), map: "maps/Ahlheim/global.cfg".into(), ..Default::default() }, "x");
    c.online.on_server = true;
    let work = Work { hours: 10.0, km: 150.0, journeys: 20, buses: 2, days: 312 };
    let mut k = bc::make(&mut c, "Linie5", "5", Kind::Gross, Period::Day, 3_000_000_00, &work, "2026-11-01", "2026-11-30");
    (k.per_hour, k.per_bus_day, k.buses) = (130_00, 270_00, 2);
    c.concessions.contracts.push(k);
    c.concessions.held.push(Concession { line: "Linie5".into(), number: "5".into(), from: "2026-11-01".into(), until: "2026-11-30".into(), price: 0.9, direct: false, after: String::new(), options: 0, package: String::new(), with: Vec::new() });
    c
}

/// The map's week: line 5 with `tours` tours, line 6 with three, every day.
fn week(tours: usize) -> Vec<Vec<LineInfo>> {
    (0..7).map(|_| vec![line("Linie5", "5", tours, true), line("Linie6", "6", 3, true)]).collect()
}

/// H2: Denmark's public holidays (2026: Easter on 5 April) and school holidays (weeks 7 and
/// 42, Easter, the summer, Christmas), the ISO weeks, and a server calendar's tour bits.
#[test]
fn the_danish_calendar() {
    for d in ["2026-01-01", "2026-04-02", "2026-04-03", "2026-04-05", "2026-04-06", "2026-05-14", "2026-05-24", "2026-05-25", "2026-12-25", "2026-12-26", "2023-05-05"] {
        assert!(service::danish_public_holiday(code(d)), "{d}");
    }
    for d in ["2026-06-05", "2026-12-24", "2026-05-01", "2026-03-04", "2024-04-26"] {
        assert!(!service::danish_public_holiday(code(d)), "{d}");
    }
    assert_eq!(service::iso_week(dates::parse("2026-02-09").unwrap()), (2026, 7));
    assert_eq!(service::iso_week(dates::parse("2026-01-01").unwrap()), (2026, 1));
    assert_eq!(service::iso_week(dates::parse("2027-01-01").unwrap()), (2026, 53));
    for d in ["2026-02-09", "2026-02-15", "2026-10-12", "2026-10-18", "2026-03-28", "2026-04-06", "2026-06-27", "2026-07-15", "2026-08-09", "2026-12-21", "2027-01-02"] {
        assert!(service::danish_school_holiday(code(d)), "{d}");
    }
    for d in ["2026-02-16", "2026-10-19", "2026-06-26", "2026-08-10", "2026-03-27", "2026-12-20", "2027-01-04"] {
        assert!(!service::danish_school_holiday(code(d)), "{d}");
    }
    let map = omsi_map::Calendar::default();
    let dk = ServerCalendar { danish: true, periods: vec![(code("2026-11-10"), code("2026-11-11"), "Strike".into())] };
    // (a public holiday: the holiday bit; a period of the admin's: the school holidays' bit)
    assert_eq!(dk.bits(code("2026-12-25"), &map), (1 << 7, 1 << 9));
    assert_eq!(dk.bits(code("2026-11-10"), &map), (1 << 1, 1 << 9));
    assert_eq!(dk.bits(code("2026-11-12"), &map), (1 << 3, 1 << 8));
    assert_eq!(dk.period_of(code("2026-11-11")), Some("Strike"));
    // (the map's own with the admin's periods on top)
    let own = ServerCalendar { danish: false, periods: dk.periods.clone() };
    assert!(!own.public_holiday(code("2026-12-25"), &map) && own.school_holiday(code("2026-11-10"), &map));
}

/// H2: the server's settings and the admin's periods make its calendar - none while it keeps the
/// maps' own and names no period -, added and taken away, read again when changed.
#[test]
fn the_servers_calendar_and_its_holidays() {
    let data = temp("calendar");
    assert_eq!(calendar_of(&data), None);
    assert!(add_period(&data, "Strike", "2026-11-12", "2026-11-10").is_err(), "the last before the first");
    let p = add_period(&data, "Strike", "2026-11-10", "2026-11-11").unwrap();
    assert_eq!(calendar_of(&data), Some(service::ServerCalendar { danish: false, periods: vec![(code("2026-11-10"), code("2026-11-11"), "Strike".into())] }));
    std::fs::write(data.join(super::super::main_server::CFG_FILE), "calendar = dk\n").unwrap();
    assert!(calendar_of(&data).is_some_and(|c| c.danish && c.periods.len() == 1));
    remove_period(&data, p.id).unwrap();
    assert_eq!(calendar_of(&data).map(|c| c.periods.len()), Some(0));
    // (read for the process - the maps' own here: the other tests' days are not touched)
    std::fs::write(data.join(super::super::main_server::CFG_FILE), "calendar = map\n").unwrap();
    assert_eq!(calendar_of(&data), None);
    assert!(refresh_calendar(&data));
    assert_eq!(service::server_calendar(), None);
    assert!(!refresh_calendar(&data), "unchanged: not read again");
    let _ = std::fs::remove_dir_all(&data);
}

/// H1: the map's new timetable measured against the contract as agreed; told, applied on its day
/// (the contract's work and buses the timetable's), and - more service than the server's share a
/// company must run - declinable for a day: declined, the contract ends the day before.
#[test]
fn a_new_timetable_changes_the_contract() {
    let data = temp("timetable");
    let mut c = holder();
    let ch = measure(&c, ChangeKind::Timetable, "Linie5", "", &week(4), "2026-11-03", None).unwrap();
    assert_eq!((ch.from.as_str(), ch.lines.clone(), ch.name.as_str()), ("2026-11-05", vec!["Linie5".to_string()], "5"));
    assert!((ch.hours_before - 60.0).abs() < 1e-6, "{}", ch.hours_before);
    assert!(ch.hours_after > 0.0 && ch.buses_after == 4 && ch.buses_before == 2 && ch.decline_by.is_empty(), "{ch:?}");
    let mut run = Run::default();
    received(&mut c, ch.clone(), &mut run, 0);
    assert_eq!(c.online.changes.len(), 1);
    assert_eq!(step(&mut c), (false, false), "not its day yet");
    c.date = "2026-11-05".into();
    assert_eq!(step(&mut c), (true, false));
    let k = c.concessions.contracts.iter().find(|k| k.line == "Linie5").unwrap();
    assert_eq!((k.buses, k.work.map(|w| w.buses)), (4, Some(4)));
    assert_eq!(c.online.changes[0].state, "applied");
    // (with a share to run: more than it - declinable until the next day; declined, it ends)
    let mut c = holder();
    let ch = measure(&c, ChangeKind::Timetable, "Linie5", "", &week(8), "2026-11-03", Some(10.0)).unwrap();
    assert_eq!(ch.decline_by, "2026-11-04");
    received(&mut c, ch, &mut Run::default(), 0);
    let id = c.online.changes[0].id;
    decline(&data, &mut c, id).unwrap();
    assert_eq!(c.concessions.held[0].until, "2026-11-04");
    assert!(c.concessions.contracts.iter().all(|k| k.until == "2026-11-04"));
    assert_eq!(decline(&data, &mut c, id), Err(NO_CHANGE));
    // (less service: never declinable)
    let c = holder();
    let ch = measure(&c, ChangeKind::Timetable, "Linie5", "", &week(1), "2026-11-03", Some(10.0)).unwrap();
    assert!(ch.decline_by.is_empty() && ch.hours_after < ch.hours_before);
    let _ = std::fs::remove_dir_all(&data);
}

/// H1: a line added - in the package with the contract's (one made of a line that ran alone), its
/// own contract at the package's prices from the change's day to the contract's end, taken on -;
/// declined, it goes out to tender on its own; a line taken out ends the day before at once.
#[test]
fn a_line_added_to_a_contract_and_taken_out() {
    let data = temp("lines");
    let mut c = holder();
    assert!(measure(&c, ChangeKind::AddLine, "Linie5", "Linie5", &week(2), "2026-11-03", None).is_err(), "its own");
    assert!(measure(&c, ChangeKind::RemoveLine, "Linie5", "Linie5", &week(2), "2026-11-03", None).is_err(), "its only line");
    assert_eq!(measure(&c, ChangeKind::Timetable, "Linie9", "", &week(2), "2026-11-03", None).err().as_deref(), Some(NO_CONTRACT));
    let ch = measure(&c, ChangeKind::AddLine, "Linie5", "Linie6", &week(2), "2026-11-03", None).unwrap();
    assert!(ch.hours_after > ch.hours_before && ch.buses_after == 5 && ch.number == "6", "{ch:?}");
    received(&mut c, ch, &mut Run::default(), 0);
    c.date = "2026-11-05".into();
    assert_eq!(step(&mut c), (true, true));
    let six = c.concessions.contracts.iter().find(|k| k.line == "Linie6").unwrap();
    assert_eq!((six.from.as_str(), six.until.as_str(), six.per_hour, six.per_bus_day, six.buses), ("2026-11-05", "2026-11-30", 130_00, 270_00, 3));
    let held: Vec<(&str, &str, usize)> = c.concessions.held.iter().map(|h| (h.line.as_str(), h.package.as_str(), h.with.len())).collect();
    assert_eq!(held, vec![("Linie5", "5", 2), ("Linie6", "5", 2)]);
    // (now a package of two: line 6 taken out - its last day the day before the change)
    let ch = measure(&c, ChangeKind::RemoveLine, "Linie5", "Linie6", &week(2), "2026-11-05", Some(1.0)).unwrap();
    assert!(ch.decline_by.is_empty() && ch.buses_after == 2, "{ch:?}");
    received(&mut c, ch, &mut Run::default(), 0);
    assert_eq!(c.concessions.held.iter().find(|h| h.line == "Linie6").map(|h| h.until.as_str()), Some("2026-11-06"));
    c.date = "2026-11-07".into();
    step(&mut c);
    assert!(c.concessions.held.iter().all(|h| !h.with.iter().any(|l| l == "Linie6")));
    // (an added line declined: out to tender on its own)
    let mut c = holder();
    let ch = measure(&c, ChangeKind::AddLine, "Linie5", "Linie6", &week(2), "2026-11-03", Some(10.0)).unwrap();
    received(&mut c, ch, &mut Run::default(), 0);
    decline(&data, &mut c, 1).unwrap();
    assert_eq!(c.online.changes[0].state, "declined");
    let asks = std::fs::read_to_string(super::super::shared_tenders::dir(&data).join("ahlheim.asks.jsonl")).unwrap_or_default();
    assert!(asks.contains("Linie6"), "{asks}");
    c.date = "2026-11-05".into();
    assert_eq!(step(&mut c), (false, false), "declined: not applied");
    let _ = std::fs::remove_dir_all(&data);
}

/// H3: roadworks on a stretch - the stops between closed, kept with the server's; each company of
/// the map running the line gets them as a diversion and the inbox's event, the minutes allowed
/// and paid; ended, they go.
#[test]
fn roadworks_close_a_stretch() {
    let data = temp("roadworks");
    let lines = vec![line("Linie5", "5", 2, true)];
    let l = &lines[0];
    assert_eq!(stretch(l, "s3", "S1"), Some(("S3".to_string(), "S1".to_string(), vec!["S2".to_string()])));
    assert_eq!(stretch(l, "S1", "S9"), None);
    let rw = Roadwork { map: "ahlheim".into(), line: "linie5".into(), from_stop: "S1".into(), to_stop: "S3".into(), from: "2026-11-04".into(), until: "2026-11-06".into(), minutes: 4, ..Default::default() };
    assert!(add_roadwork(&data, Roadwork { from_stop: "S1".into(), to_stop: "S9".into(), ..rw.clone() }, &lines).unwrap_err().contains("its stops: S0"));
    let rw = add_roadwork(&data, rw, &lines).unwrap();
    assert_eq!((rw.id, rw.line.as_str(), rw.closed.clone()), (1, "Linie5", vec!["S2".to_string()]));
    let mut c = holder();
    c.lines.push(crate::company::model::CompanyLine { name: "Linie5".into(), number: "5".into(), ..Default::default() });
    assert!(roadworks_step(&data, &mut c));
    assert!(!roadworks_step(&data, &mut c), "unchanged");
    let d = c.ontime.diversions.iter().find(|d| d.id == ROADWORK_IDS + 1).unwrap();
    assert_eq!((d.closed.clone(), d.lines.clone()), (vec!["S2".to_string()], vec!["Linie5".to_string()]));
    let e = c.inbox.events.iter().find(|e| e.id == ROADWORK_IDS + 1).unwrap();
    assert_eq!((e.key.as_str(), e.line.as_deref(), e.demand), ("roadworks", Some("5"), ROADWORK_DEMAND));
    assert_eq!((extra_minutes(&c, "2026-11-05", "LINIE5"), extra_minutes(&c, "2026-11-07", "Linie5")), (4, 0));
    assert_eq!((judged(10, 4), judged(3, 4), judged(-2, 4)), (6, 0, -2));
    // (a player's departures after the stretch: 4 minutes more allowed - 18 minutes late after
    // it is no fine, before it is)
    use crate::{StopVisit, TripRun};
    let stop = |i: usize, name: &str, dep: f64, late: f64| StopVisit { index: i, name: name.into(), arr: dep, dep, arrived: Some(dep + late), left: Some(dep + late) };
    let t = TripRun { visits: vec![stop(0, "S0", 21600.0, 18.0 * 60.0), stop(1, "S1", 22200.0, 18.0 * 60.0), stop(2, "S3", 22800.0, 18.0 * 60.0), stop(3, "S4", 23400.0, 18.0 * 60.0)], ..Default::default() };
    assert_eq!(super::super::server_fines::departures(&c, "2026-11-05", &t, Some(("Linie5", "1"))), (0, 2));
    assert_eq!(super::super::server_fines::departures(&c, "2026-11-07", &t, Some(("Linie5", "1"))), (0, 4), "no roadworks that day");
    // (ended: they go)
    end_roadwork(&data, 1).unwrap();
    // (the file's time in whole seconds: another stamp)
    c.online.roadworks_at = 0;
    assert!(roadworks_step(&data, &mut c));
    assert!(!c.ontime.diversions.iter().any(|d| d.id >= ROADWORK_IDS) && !c.inbox.events.iter().any(|e| e.id >= ROADWORK_IDS) && c.online.roadworks.is_empty());
    let _ = std::fs::remove_dir_all(&data);
}

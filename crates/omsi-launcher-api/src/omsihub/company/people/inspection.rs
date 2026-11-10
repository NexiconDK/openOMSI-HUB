//! The Staff page's Ticket inspection tab (NexiconDK, ticket inspection round S, 9 November 2026;
//! `co::inspection`, `co::dodgers`): today's teams and the lines they check, the checks of a day
//! closed (when, the line, the trip, the team, the passengers checked, those caught and their
//! fines), the totals of the last 7 and 30 days, and every line's share of fare dodgers with its
//! trend and why. Before the research, and without inspectors, it says how to start.

use super::super::kit::Go;
use super::super::words::{eur, grouped, local_day, num, tr, trf};
use super::super::RESEARCH_TAB;
use omsi_launcher_lib::company::research::keys;
use omsi_launcher_lib::company::{self as co, Company, Role};
use serde_json::{json, Value};

/// The days of a line's trend.
const TREND_DAYS: usize = 30;
/// The closed days to pick from.
const PICK_DAYS: usize = 7;

fn hhmm(m: i32) -> String {
    co::clock::hhmm(m.rem_euclid(1440) as i64)
}

fn pct(x: f64) -> String {
    format!("{} %", num(100.0 * x, 1))
}

/// "Anna Berg & Ole Hansen".
fn team(names: &[String]) -> String {
    names.iter().filter(|n| !n.is_empty()).cloned().collect::<Vec<_>>().join(" & ")
}

/// The tab: `ui.day` the closed day picked (0 the last).
pub fn part(c: &Company, ui: &Value) -> Value {
    let researched = co::research::has(c, keys::TICKET_INSPECTORS);
    let inspectors = c.staff.iter().filter(|e| e.role == Role::Inspector && e.employed_on(&c.date)).count();
    let each = co::inspection::fine(c);
    let easy = co::autopilot::on(c);
    let empty = if !researched {
        Some(json!({
            "text": tr("Ticket inspectors check the passengers' tickets on your lines: fewer people ride without one, and those caught pay a fine. Research ticket inspection first; then they can be hired on the Hire employees tab."),
            "go": if easy { Go::ToNormal(RESEARCH_TAB).json() } else { Go::Research.json() },
        }))
    } else if inspectors == 0 {
        Some(json!({
            "text": tr("The company may hire ticket inspectors now. They work in teams of two, each team on a line of its own - the busiest lines first - and a line they check keeps fewer dodgers for about two weeks."),
            "hire": tr("Hire ticket inspectors"),
            "hire_role": Role::ALL.iter().position(|r| *r == Role::Inspector).unwrap_or(0),
        }))
    } else {
        None
    };
    // (today's teams)
    let patrols = co::staff::patrols(c, &c.date);
    let teams: Vec<Value> = patrols
        .iter()
        .map(|p| {
            let names: Vec<String> = p.ids.iter().filter_map(|id| c.employee(*id).map(|e| e.name.clone())).collect();
            let number = c.lines.iter().find(|l| l.name == p.line).map(|l| l.number.clone()).unwrap_or_else(|| p.line.clone());
            json!({ "line": number, "who": team(&names), "time": format!("{}–{}", hhmm(p.from), hhmm(p.to)), "alone": p.ids.len() < 2 })
        })
        .collect();
    // (the closed days with checks to pick from, the last first)
    let days: Vec<&co::inspection::Day> = c.inspection.days.iter().rev().filter(|d| !d.checks.is_empty()).take(PICK_DAYS).collect();
    let pick = ui.get("day").and_then(Value::as_u64).unwrap_or(0) as usize;
    let pick = pick.min(days.len().saturating_sub(1));
    let checks: Vec<Value> = days
        .get(pick)
        .map(|d| {
            d.checks
                .iter()
                .map(|k| {
                    json!({
                        "time": format!("{}–{}", hhmm(k.dep), hhmm(k.arr)), "line": k.number,
                        "trip": if k.from.is_empty() { k.to.clone() } else { format!("{} → {}", k.from, k.to) },
                        "who": team(&k.names), "checked": k.checked, "caught": k.caught,
                        "fines": if k.caught > 0 { eur(k.caught as i64 * k.fine) } else { "–".into() },
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let day_sum = days.get(pick).map(|d| {
        let checked: u32 = d.checks.iter().map(|k| k.checked).sum();
        let caught: u32 = d.checks.iter().map(|k| k.caught).sum();
        trf("%{n} trips checked  ·  %{checked} passengers  ·  %{caught} without a ticket  ·  %{written} written, %{paid} paid", &[("n", &d.checks.len().to_string()), ("checked", &grouped(checked as f64)), ("caught", &caught.to_string()), ("written", &eur(d.written)), ("paid", &eur(d.paid))])
    });
    // (the totals: up to the last day closed)
    let last = c.inspection.days.last().map(|d| d.date.clone()).unwrap_or_else(|| co::dates::add(&c.date, -1));
    let totals: Vec<Value> = [(7, "Last 7 days"), (30, "Last 30 days")]
        .iter()
        .map(|(n, label)| {
            let (k, checked, caught, written, paid) = co::inspection::totals(c, &last, *n);
            let rate = if checked > 0 { pct(caught as f64 / checked as f64) } else { "–".into() };
            json!({ "label": tr(label), "checks": k, "checked": grouped(checked as f64), "caught": caught, "rate": rate, "written": eur(written), "paid": eur(paid) })
        })
        .collect();
    // (every line's share now, its trend and why)
    let watched = co::staff::inspected(c, &c.date);
    let mut lines: Vec<(f64, Value)> = c
        .lines
        .iter()
        .filter(|l| co::network::in_service(l, co::clock::moment(&c.date, 24 * 60 - 1)) || c.inspection.days.iter().rev().take(TREND_DAYS).any(|d| d.lines.iter().any(|x| x.line == l.name)))
        .map(|l| {
            let why = co::dodgers::of_line(c, &l.name);
            let trend: Vec<Value> = {
                let kept: Vec<&co::inspection::Day> = c.inspection.days.iter().rev().take(TREND_DAYS).collect();
                kept.into_iter().rev().map(|d| d.lines.iter().find(|x| x.line == l.name).filter(|x| x.passengers > 0).map_or(Value::Null, |x| json!((1000.0 * x.share()).round() / 1000.0))).collect()
            };
            let mut tags: Vec<Value> = Vec::new();
            // (the evening and night trips raise its share by a point or more)
            if why.raw - co::dodgers::BASE - co::dodgers::FULL * why.full >= 0.01 {
                tags.push(json!([tr("Evening and night buses"), "warn"]));
            }
            if why.full >= 0.1 {
                tags.push(json!([tr("Full buses"), "warn"]));
            }
            if let Some(x) = watched.iter().find(|x| x.0 == l.name) {
                tags.push(json!([trf("Checked today by %{n}", &[("n", &x.2.len().to_string())]), "ok"]));
            }
            if why.deterrence >= 0.02 {
                tags.push(json!([trf("Inspections keep %{n} off", &[("n", &format!("{:.0} %", 100.0 * why.deterrence))]), "ok"]));
            }
            let checked_days = c.inspection.days.iter().rev().take(14).filter(|d| d.lines.iter().any(|x| x.line == l.name && x.checked > 0.0)).count();
            (
                why.share,
                json!({
                    "number": l.number, "name": l.name, "share": pct(why.share), "frac": (why.share / co::dodgers::MAX).min(1.0),
                    "raw": trf("without inspections %{n}", &[("n", &pct(why.raw))]),
                    "tone": if why.share >= 0.07 { "danger" } else if why.share >= co::dodgers::BASE - 0.001 { "warn" } else { "ok" },
                    "tags": tags, "trend": trend,
                    "checked": trf("checked %{n} of the last 14 days", &[("n", &checked_days.to_string())]),
                }),
            )
        })
        .collect();
    lines.sort_by(|a, b| b.0.total_cmp(&a.0));
    json!({
        "kind": "inspection",
        "empty": empty,
        "today_title": format!("{}  ·  {}", tr("Today's teams"), local_day(&c.date, false)).to_uppercase(), "teams": teams,
        "no_teams": tr("No ticket inspectors at work today."),
        "alone": tr("alone"),
        "checks_title": tr("Checks").to_uppercase(),
        "days": days.iter().map(|d| local_day(&d.date, false)).collect::<Vec<_>>(), "day": pick,
        "day_label": days.get(pick).map(|d| local_day(&d.date, true)),
        "day_sum": day_sum, "checks": checks,
        "no_checks": tr("No checks yet: the day's close shows what the inspectors saw."),
        "cols": [tr("Time"), tr("Line"), tr("Bus trip"), tr("Team"), tr("Checked"), tr("Caught"), tr("Fines")],
        "totals_title": tr("Totals").to_uppercase(), "totals": totals,
        "total_cols": [tr("Trips checked"), tr("Passengers"), tr("Without a ticket"), tr("Share caught"), tr("Fines written"), tr("Paid")],
        "lines_title": tr("Fare dodgers by line").to_uppercase(), "lines": lines.into_iter().map(|x| x.1).collect::<Vec<_>>(),
        "lines_note": trf("About %{base} of the passengers ride without a ticket by day, more in the evening, at night and on full buses. A line the inspectors check keeps fewer for about two weeks. Each one caught pays %{fine}; about two thirds of the fines get paid.", &[("base", &format!("{:.0} %", 100.0 * co::dodgers::BASE)), ("fine", &eur(each))]),
        "trend_tip": tr("The last 30 days"),
    })
}

#[cfg(test)]
mod tests {
    /// A day is written as each language writes it.
    #[test]
    fn days_are_written_as_each_language_writes_them() {
        use super::super::super::words::{local_day, set_language};
        set_language("da");
        assert_eq!(local_day("2026-12-05", false), "lø 5. dec.");
        assert_eq!(local_day("2026-12-05", true), "lø 5. dec. 2026");
        set_language("de");
        assert_eq!(local_day("2026-05-03", false), "So. 3. Mai");
        set_language("fr");
        assert_eq!(local_day("2026-12-05", false), "sam. 5 déc.");
        set_language("en");
        assert_eq!(local_day("2026-12-05", false), "Sat 5 Dec");
    }

    /// Every text of the tab and of the inspectors' daily message is translated.
    #[test]
    fn every_inspection_text_is_translated() {
        let own = include_str!("inspection.rs");
        let own = &own[..own.find("#[cfg(test)]").unwrap()];
        let core = include_str!("../../../../../omsi-launcher-core/src/company/inspection.rs");
        let mut texts: Vec<String> = Vec::new();
        for (src, calls) in [(own, &["tr(\"", "trf(\""][..]), (core, &["Message::new(Kind::Staff, \""][..])] {
            for call in calls {
                let mut rest = src;
                while let Some(at) = rest.find(call) {
                    rest = &rest[at + call.len()..];
                    let text: String = rest.chars().take_while(|c| *c != '"').collect();
                    texts.push(text.clone());
                    // (a message's text is its second literal)
                    if call.starts_with("Message") {
                        let after = &rest[text.len() + 1..];
                        let start = after.find('"').unwrap() + 1;
                        texts.push(after[start..].chars().take_while(|c| *c != '"').collect());
                    }
                }
            }
        }
        texts.extend(["Last 7 days", "Last 30 days", "Ticket inspection", "Ticket inspection fines"].map(String::from));
        assert!(texts.len() > 30, "{texts:?}");
        let mut missing = Vec::new();
        for lang in ["nl", "de", "fr", "ru", "uk", "pl", "da"] {
            let table = crate::i18n::table(lang);
            for t in &texts {
                if !table.contains_key(t) {
                    missing.push(format!("{lang}: {t:?}"));
                }
            }
        }
        assert!(missing.is_empty(), "{} missing:\n{}", missing.len(), missing.join("\n"));
    }
}

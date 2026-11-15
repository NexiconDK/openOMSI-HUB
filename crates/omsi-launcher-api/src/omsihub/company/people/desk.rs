//! (NexiconDK, 15 November 2026: "Drifts Assistent ... Evt. en vagtplan over Drift Assistenter")
//! The Staff page's Operations assistants tab (`people.js`'s `Desk`), on a server's company: who is
//! at the desk now, the week's shift plan (early, late, night for every day: an assistant or
//! nobody), filled at once or by hand, what the assistants may decide, and what they did.
//! The company's side is `co::ops_assistant`.

use super::super::words::{filled, local_weekday, tr, trf};
use super::super::{act as apply, Session};
use omsi_launcher_lib::company::model::Role;
use omsi_launcher_lib::company::{self as co, ops_assistant as oa, Company};
use serde_json::{json, Value};

/// "04:00".
fn hhmm(m: i32) -> String {
    format!("{:02}:{:02}", m.rem_euclid(1440) / 60, m.rem_euclid(60))
}

/// The tab's body.
pub fn part(c: &Company) -> Value {
    let people = oa::assistants(c, &c.date);
    let now = match (oa::on_shift(c, &c.date, c.clock.minute as i32), c.ops_desk.chiefs_online) {
        (Some((id, _)), false) => (trf("%{name} is at the desk and runs the day now.", &[("name", &c.employee(id).map(|e| e.name.clone()).unwrap_or_default())]), "ok"),
        (Some((id, _)), true) => (trf("%{name} is at the desk, but a boss is online: he leaves the day to you.", &[("name", &c.employee(id).map(|e| e.name.clone()).unwrap_or_default())]), "accent2"),
        (None, _) => (tr("Nobody is at the desk now: what comes up waits for you (and the duties nobody covers go as the server's rules say)."), "warn"),
    };
    let mut choices = vec![json!({ "id": 0, "name": tr("Nobody") })];
    choices.extend(people.iter().map(|e| json!({ "id": e.id, "name": e.name })));
    let rows: Vec<Value> = (0..oa::SHIFTS.len())
        .map(|s| {
            let (from, to) = oa::SHIFTS[s];
            json!({
                "name": tr(oa::SHIFT_NAMES[s]), "hours": format!("{}–{}", hhmm(from), hhmm(to)),
                "cells": (0..7).map(|wd| {
                    let id = c.ops_desk.who(s, wd);
                    // (one who left, or is no assistant any more: shown as nobody)
                    let id = if people.iter().any(|e| e.id == id) { id } else { 0 };
                    json!({ "id": id })
                }).collect::<Vec<Value>>(),
            })
        })
        .collect();
    let staff: Vec<Value> = people
        .iter()
        .map(|e| {
            let n = oa::shifts_of(c, e.id).len();
            let away = e.absent(&c.date).then(|| tr("Away today (ill, on holiday or on a course)"));
            json!({ "id": e.id, "name": e.name, "portrait": e.portrait, "shifts": trf("%{n} of %{m} shifts a week", &[("n", &n.to_string()), ("m", &oa::MOST_A_WEEK.to_string())]), "away": away })
        })
        .collect();
    let log: Vec<Value> = c
        .ops_desk
        .log
        .iter()
        .rev()
        .take(40)
        .map(|d| {
            let date = co::clock::date_of(d.at);
            json!({ "when": format!("{} {}", super::super::words::short_date(&date), hhmm(d.at.rem_euclid(co::clock::DAY) as i32)), "text": filled(&tr(&d.text), &d.args) })
        })
        .collect();
    let agency_allowed = co::live_pay::agency_runs_open(c);
    json!({
        "kind": "desk",
        "title": tr("Operations assistants"),
        "say": tr("An operations assistant runs the day while neither you nor a co-owner or manager is online: he gives the duties of drivers who called in sick to a colleague who can take them, sends the spare bus at once when one breaks down, and sends for an agency driver when nobody of yours can. Give each shift of the week an assistant: at most five shifts each, eleven hours off between two."),
        "now": now.0, "now_tone": now.1,
        "days": (0..7).map(local_weekday).collect::<Vec<String>>(),
        "rows": rows, "choices": choices,
        "staff_caps": tr("Your operations assistants"), "staff": staff,
        "fill": tr("Fill automatically"), "fill_tip": tr("Every shift of the week to the assistant with the fewest, each keeping to his shift as far as the rules allow."),
        "clear": tr("Clear the plan"),
        "agency": !c.ops_desk.no_agency, "agency_label": tr("May send for an agency driver when nobody of yours can take a duty"),
        "agency_note": (!agency_allowed).then(|| tr("The server does not use agency drivers: a duty nobody of yours can take falls out.")),
        "log_caps": tr("What the assistants did"), "log": log, "no_log": tr("Nothing yet."),
        "none": people.is_empty().then(|| tr("You have no operations assistants yet. Hire one or more on the Hire employees tab: each works up to five shifts a week.")),
        "hire": tr("Hire operations assistants"),
        "hire_role": Role::ALL.iter().position(|r| *r == Role::Operations).unwrap_or(0),
    })
}

/// The tab's buttons: `desk_slot {shift, day, id}`, `desk_fill`, `desk_clear`, `desk_agency {on}`.
pub fn act(s: &mut Session, what: &str, args: &Value) -> Option<Result<Value, String>> {
    let n = |k: &str| args.get(k).and_then(Value::as_u64).unwrap_or(0);
    let done = |r: Result<(), Value>| Ok(r.err().map(|p| json!({ "popup": p })).unwrap_or(json!({})));
    Some(match what {
        "desk_slot" => {
            let (shift, day, id) = (n("shift") as usize, n("day") as usize, n("id") as u32);
            done(apply(s, |c| oa::set_slot(c, shift, day, id)))
        }
        "desk_fill" => match apply(s, |c| Ok(oa::auto_fill(c))) {
            Ok(k) => {
                let t = trf("%{n} of 21 shifts have an assistant.", &[("n", &k.to_string())]);
                s.say(t, false);
                Ok(json!({}))
            }
            Err(p) => Ok(json!({ "popup": p })),
        },
        "desk_clear" => done(apply(s, |c| {
            c.ops_desk.grid.clear();
            Ok(())
        })),
        "desk_agency" => {
            let on = args.get("on").and_then(Value::as_bool).unwrap_or(true);
            done(apply(s, |c| {
                c.ops_desk.no_agency = !on;
                Ok(())
            }))
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    /// (15 November 2026) Every text of the operations assistants' tab, of their desk in the core and
    /// of the fleet's Leasing tab, in the six languages.
    #[test]
    fn every_text_of_the_desk_and_the_leasing_is_translated() {
        let own = include_str!("desk.rs").split("#[cfg(test)]").next().unwrap_or("");
        let mut texts: Vec<String> = super::oa::texts().into_iter().map(String::from).collect();
        for lead in ["tr(\"", "trf(\""] {
            let mut rest = own;
            while let Some(k) = rest.find(lead) {
                let after = &rest[k + lead.len()..];
                rest = after;
                let t: String = after.chars().take_while(|c| *c != '"').collect();
                if t.chars().filter(|c| c.is_alphabetic()).count() >= 2 {
                    texts.push(t);
                }
            }
        }
        texts.extend(
            [
                "Leasing (%{n})", "%{n} months left", "%{n} days left", "Back now: %{amount}", "Runs today", "%{n} leased buses  ·  %{amount} a month in all", "Tick all", "Tick none", "Give back the ticked buses",
                "Buses %{list} go back to the leasing company today.", "From then on the company saves %{amount} a month.", "Give back %{n} leased buses?", "Give back for %{amount}", "%{n} leased buses went back (%{amount}).",
                "None of these buses is leased.", "The group's leased buses", "No depot has leased buses.", "Ordered: %{what} goes back to the leasing company", "Operations assistant", "Operations assistants",
                "This duty is over", "This duty is over: it ran without a driver and can no longer be given today.", "Give it a driver for the days to come.",
                "began at %{time} without a driver: whoever takes it now drives the rest", "Over without a driver (%{n})", "These ran today without a driver: they can no longer be given.",
            ]
            .map(String::from),
        );
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

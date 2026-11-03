//! (NexiconDK, Bus Company Online, decided 3 November 2026) The award criteria of the server's
//! tenders as Midttrafik weighed the bids in its 65th tender, the town buses of Viborg (the plan
//! calls it "the Viborg model"): the price counts half, the quality the other half - the operating
//! quality 25 %, the vehicles 10 %, the implementation plan 10 %, the working environment 5 %. A
//! dearer company that runs well may so beat a cheaper one that runs badly. The server's admin
//! may let a tender by the lowest price alone instead (`Award::Lowest`, as Midttrafik's 71st).
//!
//! Each part is 0 to 100 points, the weights make them the bid's points:
//! * **the price**: the lowest valid ask gets them all, another the share the lowest is of it;
//! * **the operating quality**: the company's track record (`bus_contracts::quality`);
//! * **the vehicles**: its buses - their age (new 100, `OLD_YEARS` or older 0), how clean they
//!   run (electric 100, CNG 80, diesel 50) and their floor (low floor 100, low entry 70, high
//!   floor 30), 50 / 30 / 20 %;
//! * **the implementation plan**: whether it is ready to start - room in its depot, buses and
//!   drivers for the work;
//! * **the working environment**: its staff's satisfaction, less those off sick.
//!
//! A company's parts are taken when it first bids (`Parts::of`, kept with its bid); an operator's
//! lie around its quality (`operator_parts`).

use super::concessions::Tender;
use super::model::{Company, Drive, Role};
use super::rng::Rng;
use serde::{Deserialize, Serialize};

/// The weights, in points of 100.
pub const PRICE: f64 = 50.0;
pub const OPERATION: f64 = 25.0;
pub const VEHICLES: f64 = 10.0;
pub const PLAN: f64 = 10.0;
pub const WORK: f64 = 5.0;

/// A bus this old or older has no vehicle points for its age.
pub const OLD_YEARS: f64 = 12.0;
/// (3 November 2026) The vehicle points' parts: the buses' age, their drive and their floor.
pub const AGE_SHARE: f64 = 0.5;
pub const DRIVE_SHARE: f64 = 0.3;
pub const FLOOR_SHARE: f64 = 0.2;

/// The drivers a bus of the work needs (two shifts and the days off between them).
pub const DRIVERS_PER_BUS: f64 = 2.2;

/// How a tender is awarded.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum Award {
    /// Price 50 %, quality 50 % (25 / 10 / 10 / 5).
    #[default]
    Viborg,
    /// The lowest valid price alone.
    Lowest,
    /// (NexiconDK, 3 November 2026) A price the authority fixed (a rail replacement it orders:
    /// `rail`): every valid bid gets the price points, the quality decides.
    Fixed,
}

impl Award {
    /// The settings' word (`tender_award`): `lowest` (or `price`), anything else Viborg's.
    pub fn of(word: &str) -> Award {
        match word.trim().to_lowercase().as_str() {
            "lowest" | "price" | "lowest price" => Award::Lowest,
            _ => Award::Viborg,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Award::Viborg => "viborg",
            Award::Lowest => "lowest",
            Award::Fixed => "fixed",
        }
    }

    /// The most points the price gives, and the quality.
    pub fn price_most(self) -> f64 {
        match self {
            Award::Viborg | Award::Fixed => PRICE,
            Award::Lowest => 100.0,
        }
    }

    pub fn quality_most(self) -> f64 {
        100.0 - self.price_most()
    }

    /// Whether a default value (the serde skip).
    pub fn is_default(&self) -> bool {
        *self == Award::Viborg
    }
}

/// The price points of an ask against the lowest valid one.
pub fn price_points(amount: i64, lowest: i64, award: Award) -> f64 {
    if amount <= 0 || award == Award::Fixed {
        return award.price_most();
    }
    award.price_most() * (lowest.max(1) as f64 / amount as f64).min(1.0)
}

/// A bidder's quality: each part 0 to 100.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Parts {
    pub operation: f64,
    pub vehicles: f64,
    pub plan: f64,
    pub work: f64,
}

impl Parts {
    /// Every part the same: a bid of before the parts (its one quality).
    pub fn even(quality: f64) -> Parts {
        let q = quality.clamp(0.0, 100.0);
        Parts { operation: q, vehicles: q, plan: q, work: q }
    }

    /// Whether any part was given (a bid of before them has none).
    pub fn given(&self) -> bool {
        self.operation > 0.0 || self.vehicles > 0.0 || self.plan > 0.0 || self.work > 0.0
    }

    /// Company `c`'s parts in tender `t` now.
    pub fn of(c: &Company, t: &Tender) -> Parts {
        Parts { operation: super::concessions::quality(c).clamp(0.0, 100.0), vehicles: vehicles(c), plan: plan(c, t), work: work(c) }
    }

    /// The points each part gives (operation, vehicles, plan, work) by the award.
    pub fn split(&self, award: Award) -> [f64; 4] {
        match award {
            Award::Lowest => [0.0; 4],
            Award::Viborg | Award::Fixed => [
                OPERATION * self.operation.clamp(0.0, 100.0) / 100.0,
                VEHICLES * self.vehicles.clamp(0.0, 100.0) / 100.0,
                PLAN * self.plan.clamp(0.0, 100.0) / 100.0,
                WORK * self.work.clamp(0.0, 100.0) / 100.0,
            ],
        }
    }

    /// The quality points by the award.
    pub fn points(&self, award: Award) -> f64 {
        self.split(award).iter().sum()
    }

    /// The parts' average (0 to 100), each by its weight: one figure of the quality.
    pub fn overall(&self) -> f64 {
        self.points(Award::Viborg) / Award::Viborg.quality_most() * 100.0
    }
}

/// The vehicle points of company `c`'s buses: each bus by its age (new 100, `OLD_YEARS` or older
/// 0), its drive (electric 100, CNG 80, diesel 50) and its floor (low floor 100, low entry 70,
/// high floor 30: `server_buses::Floor`), weighed `AGE_SHARE`, `DRIVE_SHARE` and `FLOOR_SHARE`;
/// their average. No buses yet: 50.
pub fn vehicles(c: &Company) -> f64 {
    let buses: Vec<f64> = c
        .fleet
        .iter()
        .map(|v| {
            let age = (1.0 - v.age_years(&c.date) / OLD_YEARS).clamp(0.0, 1.0) * 100.0;
            let drive = match v.kind.drive {
                Drive::Electric => 100.0,
                Drive::Cng => 80.0,
                Drive::Diesel => 50.0,
            };
            let floor = super::server_buses::floor_of(c, v).points();
            AGE_SHARE * age + DRIVE_SHARE * drive + FLOOR_SHARE * floor
        })
        .collect();
    if buses.is_empty() {
        return 50.0;
    }
    buses.iter().sum::<f64>() / buses.len() as f64
}

/// The implementation points of company `c` in tender `t`: room in its depot for the work's
/// buses (40 %), buses to run it (30 %) and drivers (30 %), each its share of what the work needs
/// besides what the company runs now (at most all of it).
pub fn plan(c: &Company, t: &Tender) -> f64 {
    let need = t.work.buses.max(t.day_tours).max(1) as f64;
    let held = c.fleet.len() as f64;
    let in_use = c.lines.iter().map(|l| l.tours.max(0) as f64).sum::<f64>();
    let places = super::depot::places(c) as f64;
    let room = ((places - held).max(0.0) + (held - in_use).max(0.0)) / need;
    let buses = (held - in_use).max(0.0) / need;
    let drivers = c.staff.iter().filter(|s| s.role == Role::Driver && s.notice_until.is_none()).count() as f64;
    let spare = drivers - in_use * DRIVERS_PER_BUS;
    let crews = spare.max(0.0) / (need * DRIVERS_PER_BUS);
    100.0 * (0.4 * room.min(1.0) + 0.3 * buses.min(1.0) + 0.3 * crews.min(1.0))
}

/// The working environment points of company `c`: its staff's satisfaction (0 to 100) less the
/// share of them off sick today. No staff yet: 50.
pub fn work(c: &Company) -> f64 {
    if c.staff.is_empty() {
        return 50.0;
    }
    let n = c.staff.len() as f64;
    let content = c.staff.iter().map(|s| s.satisfaction.clamp(0.0, 100.0)).sum::<f64>() / n;
    let sick = c.staff.iter().filter(|s| s.sick_until.as_deref().is_some_and(|d| d >= c.date.as_str())).count() as f64 / n;
    (content * (1.0 - sick)).clamp(0.0, 100.0)
}

/// An operator's parts in a tender: its quality, each part drawn a few points either side of it
/// (the same wherever it is reckoned: `seed`).
pub fn operator_parts(quality: f64, seed: &[&str], id: i64) -> Parts {
    let mut rng = Rng::of(seed, id);
    let mut part = || (quality + rng.range(-8.0, 8.0)).clamp(0.0, 100.0);
    Parts { operation: quality.clamp(0.0, 100.0), vehicles: part(), plan: part(), work: part() }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The weights make 100; the price points by the lowest valid ask; the lowest-price award
    /// counts the price alone.
    #[test]
    fn the_weights_and_the_points() {
        assert_eq!(PRICE + OPERATION + VEHICLES + PLAN + WORK, 100.0);
        assert_eq!(price_points(100, 100, Award::Viborg), 50.0);
        assert!((price_points(108, 100, Award::Viborg) - 46.296).abs() < 0.01);
        assert_eq!(price_points(200, 100, Award::Lowest), 50.0);
        let p = Parts { operation: 90.0, vehicles: 80.0, plan: 90.0, work: 80.0 };
        assert!((p.points(Award::Viborg) - 43.5).abs() < 1e-9, "{}", p.points(Award::Viborg));
        assert_eq!(p.points(Award::Lowest), 0.0);
        assert_eq!(Award::of(" Lowest "), Award::Lowest);
        assert_eq!(Award::of("viborg"), Award::Viborg);
        assert_eq!(Award::of(""), Award::Viborg);
        assert!(!Parts::default().given() && Parts::even(70.0).given());
        assert!((Parts::even(70.0).overall() - 70.0).abs() < 1e-9);
    }

    /// The server's rules name the award (`tender_award`, Viborg's when none); a bid of before
    /// the parts reads without them and writes none.
    #[test]
    fn the_rules_and_a_bid_of_before() {
        use super::super::shared_tenders::{MarketBid, TenderCfg};
        use std::collections::HashMap;
        let mut kv = HashMap::new();
        kv.insert("tender_award".to_string(), "lowest".to_string());
        assert_eq!(TenderCfg::of_keys(&kv).award, Award::Lowest);
        assert_eq!(TenderCfg::of_keys(&HashMap::new()).award, Award::Viborg);
        let old: MarketBid = serde_json::from_str(r#"{"tender":1,"company":"a","name":"A","quality":70.0,"at":5,"amount":100,"drive":"diesel"}"#).unwrap();
        assert!(!old.parts.given());
        assert!(!serde_json::to_string(&old).unwrap().contains("parts"));
    }

    /// The plan's example: firm B 8 % dearer but better wins - 89.8 against 76.
    #[test]
    fn a_dearer_better_company_wins() {
        let a = Parts { operation: 50.0, vehicles: 50.0, plan: 60.0, work: 50.0 };
        let b = Parts { operation: 90.0, vehicles: 80.0, plan: 90.0, work: 80.0 };
        let total = |amount: i64, p: &Parts| price_points(amount, 1000, Award::Viborg) + p.points(Award::Viborg);
        let (ta, tb) = (total(1000, &a), total(1080, &b));
        assert!((ta - 76.0).abs() < 0.01 && (tb - 89.8).abs() < 0.05, "{ta} {tb}");
        let lowest = |amount: i64, p: &Parts| price_points(amount, 1000, Award::Lowest) + p.points(Award::Lowest);
        assert!(lowest(1000, &a) > lowest(1080, &b));
    }

    /// A company's parts: no buses or staff yet 50; new electric buses the most vehicle points,
    /// old diesel ones the least; staff off sick lower the working environment.
    #[test]
    fn a_companys_parts() {
        use super::super::market::{self, MarketBus};
        use super::super::model::{BusKind, BusSize, Tenure};
        let mut c = super::super::found(&super::super::Founding { name: "V".into(), date: "2026-03-04".into(), ..Default::default() }, "x");
        c.staff.clear();
        c.fleet.clear();
        assert_eq!((vehicles(&c), work(&c)), (50.0, 50.0));
        let mut bus = |built: &str, drive: Drive| {
            c.fleet.clear();
            let b = MarketBus { file: "Vehicles/X/x.bus".into(), name: "X".into(), kind: BusKind { size: BusSize::Solo, drive }, ..Default::default() };
            market::add_vehicle(&mut c, &b, built.into(), 0.0, 1.0, Tenure::Owned { paid: 0, new_value: 0 }, "");
            vehicles(&c)
        };
        let new_electric = bus("2026-01-01", Drive::Electric);
        let old_diesel = bus("2010-01-01", Drive::Diesel);
        let old_cng = bus("2010-01-01", Drive::Cng);
        // (old: no age points; diesel 50 and CNG 80 of the drive's 30 %, a city bus's low floor 20)
        assert!(new_electric > 95.0 && (old_diesel - 35.0).abs() < 1e-9 && (old_cng - 44.0).abs() < 1e-9, "{new_electric} {old_diesel} {old_cng}");
        // (a high-floor coach: 30 of the floor's 20 %)
        c.fleet.clear();
        let coach = MarketBus { file: "Vehicles/Setra/S415GT-HD.bus".into(), name: "Setra S 415 GT-HD".into(), kind: BusKind { size: BusSize::Solo, drive: Drive::Diesel }, ..Default::default() };
        market::add_vehicle(&mut c, &coach, "2010-01-01".into(), 0.0, 1.0, Tenure::Owned { paid: 0, new_value: 0 }, "");
        assert!((vehicles(&c) - 21.0).abs() < 1e-9, "{}", vehicles(&c));
        let ops = operator_parts(80.0, &["k", "op"], 3);
        assert_eq!(ops, operator_parts(80.0, &["k", "op"], 3), "the same draw");
        assert!(ops.operation == 80.0 && (72.0..=88.0).contains(&ops.vehicles));
    }
}

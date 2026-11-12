//! (NexiconDK, ticket inspection round U, 11 November 2026) The company's ticket inspectors'
//! clothes in the game, made when a duty starts (`launch::start`): for a few figures of the
//! installed people (aXYZ's first, which the clothes know best), each look the boss chose
//! (`inspection_game::letters`) - a DDS and the `.cti` item `~Inspector_<look>_<company>` in
//! openOMSI's content folder's copy of the figure's `[CTC]` folder (`omsihub_livery::inspector`),
//! never in the OMSI 2 installation. Made again only when the look, the colour or the name
//! changed. The game's people wear them only as inspectors (`omsi_sim::people::inspect`).

use omsi_launcher_lib as core;
use omsi_launcher_lib::company::{inspection_game as ig, Company};
use omsihub_livery::inspector::{dress_figure, Dress};
use std::path::{Path, PathBuf};

/// The figures the clothes were seen on, best first.
const BEST: [&str; 5] = ["axyz/man02.hum", "axyz/woman02.hum", "axyz/woman03.hum", "axyz/man01.hum", "axyz/woman01.hum"];

/// The most figures dressed a look (two men and two women where there are).
const FIGURES: usize = 4;
/// The most tried (a figure the clothes do not know is passed over).
const TRIED: usize = 10;

/// The installed people's figures (`Humans/<group>/*.hum` of the installation and the content
/// folder; not the drivers), the ones the clothes know best first.
fn figures(root: &Path, content: Option<&Path>) -> Vec<PathBuf> {
    let mut out: Vec<(u8, String, PathBuf)> = Vec::new();
    for base in std::iter::once(root.join("Humans")).chain(content.map(|c| c.join("Humans"))) {
        let Ok(groups) = std::fs::read_dir(&base) else { continue };
        for g in groups.flatten().filter(|e| e.path().is_dir()) {
            let Ok(files) = std::fs::read_dir(g.path()) else { continue };
            for f in files.flatten() {
                let p = f.path();
                let name = p.to_string_lossy().to_ascii_lowercase();
                if !name.ends_with(".hum") || name.contains("driver") || name.contains("child") || name.contains("kid") || name.contains("rollstuhl") || name.contains("wheelchair") {
                    continue;
                }
                // (aXYZ's figures whose clothes come out cleanest first - woman01's back has
                // islands that take the vest's colour onto its shoulder -, then the rest)
                let rank = match BEST.iter().position(|b| name.replace('\\', "/").ends_with(b)) {
                    Some(k) => k as u8,
                    None if name.contains("axyz") => 10,
                    None => 20,
                };
                out.push((rank, name, p));
            }
        }
    }
    out.sort();
    out.dedup_by(|a, b| a.1 == b.1);
    out.into_iter().map(|x| x.2).collect()
}

/// `#rrggbb` as RGB.
fn hex(h: &str) -> Option<[u8; 3]> {
    let h = h.trim().trim_start_matches('#');
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let b = |k: usize| u8::from_str_radix(&h[k..k + 2], 16).ok();
    Some([b(0)?, b(2)?, b(4)?])
}

/// The company's clothes for each look it uses (`letters`).
pub fn dresses(c: &Company) -> Vec<Dress> {
    dresses_of(&c.short, &c.name, c.colours.first().map(String::as_str).unwrap_or(""), &ig::letters(c))
}

/// A company's clothes for each of its looks `letters`, from its short name (else its name), its
/// first colour (`#rrggbb`) - as the server lists the companies too (`dress_world`).
pub fn dresses_of(short: &str, name: &str, colour: &str, letters: &[char]) -> Vec<Dress> {
    let colour = hex(colour).unwrap_or([0xF5, 0x82, 0x20]);
    let name = if short.trim().chars().count() >= 2 { short.trim().to_string() } else { name.trim().chars().take(14).collect() };
    letters.iter().filter(|l| matches!(l.to_ascii_uppercase(), 'A' | 'B' | 'C')).map(|look| Dress { look: look.to_ascii_uppercase(), colour, company: name.clone(), text: "BILLETKONTROL".into() }).collect()
}

/// Dress the company's inspectors for the game (said in the launcher's log; the drive starts
/// whatever comes of it).
pub fn dress(c: &Company) {
    dress_all(&ig::slug(c), &dresses(c));
}

/// Dress the inspectors of the company tagged `slug` in clothes `dresses` (each made once: a
/// later call finds them made).
fn dress_all(slug: &str, dresses: &[Dress]) {
    let (Ok(root), Some(content)) = (core::root_dir(), core::content_dir()) else { return };
    let all = figures(&root, Some(&content));
    for d in dresses {
        let (mut done, mut men, mut women) = (0, 0, 0);
        for hum in all.iter().take(TRIED) {
            if done >= FIGURES {
                break;
            }
            let woman = hum.to_string_lossy().to_ascii_lowercase().contains("woman");
            // (two of each where there are)
            if (woman && women >= FIGURES / 2 && men < FIGURES / 2) || (!woman && men >= FIGURES / 2 && women < FIGURES / 2) {
                continue;
            }
            match dress_figure(hum, &root, &content, d, slug) {
                Ok(x) => {
                    done += 1;
                    if woman {
                        women += 1;
                    } else {
                        men += 1;
                    }
                    if !x.cached {
                        core::log_to_file(&format!("inspectors: {} for {} ({} files)", x.item, hum.display(), x.files.len()));
                    }
                }
                Err(e) => core::log_to_file(&format!("inspectors: {} not dressed: {e}", hum.display())),
            }
        }
        if done == 0 {
            core::log_to_file(&format!("inspectors: no figure for the look {} of {slug} (they board in their own clothes)", d.look));
        }
    }
}

/// (NexiconDK, 12 November 2026) Dress the inspectors of the company a joined player drives for,
/// as the boss's side sent them with his start (`DriveStart::uniform`, a row as the server lists
/// the companies: `world_dresses`) - before his game starts, so that it has them.
pub fn dress_listed(row: &serde_json::Value) {
    match world_dresses(row) {
        Some((slug, ds)) if !ds.is_empty() => dress_all(&slug, &ds),
        _ => core::log_to_file("inspectors: the company's clothes not told with the start"),
    }
}

/// How long a game joining a map's world waits for the inspectors' clothes of its companies
/// (the first time; made once, a later game finds them made). What is not made by then is made
/// on meanwhile and seen from the next game on.
const WORLD_WAIT: std::time::Duration = std::time::Duration::from_secs(20);

/// (NexiconDK, ticket inspection, 11 November 2026) A company's inspectors' clothes as the
/// server lists it (`server_api::inspectors_json` beside its short name and colours): its tag
/// and its clothes. None: a server of before.
pub fn world_dresses(row: &serde_json::Value) -> Option<(String, Vec<Dress>)> {
    let ins = row.get("inspectors")?;
    let slug = ins.get("slug").and_then(serde_json::Value::as_str).map(str::trim).filter(|s| !s.is_empty())?.to_string();
    let letters: Vec<char> = ins.get("looks").and_then(serde_json::Value::as_array).map(|a| a.iter().filter_map(|l| l.as_str()?.chars().next()).collect()).unwrap_or_default();
    let text = |k: &str| row.get(k).and_then(serde_json::Value::as_str).unwrap_or("").to_string();
    let colour = row.get("colours").and_then(|c| c.get(0)).and_then(serde_json::Value::as_str).unwrap_or("");
    Some((slug, dresses_of(&text("short"), &text("name"), colour, &letters)))
}

/// (NexiconDK, ticket inspection, 11 November 2026) Before a game joins the map's world of
/// company `company` (Bus Company Online): the clothes of the inspectors of every company driven
/// in that world (`bco::companies_of_world`) - so that this game draws theirs in their uniforms
/// as it sees them in their buses (`lan_world`'s descriptions name the clothes). Made beside
/// the figures in this computer's content folder, waited for at most `WORLD_WAIT`.
pub fn dress_world(company: &str) {
    let all: Vec<(String, Vec<Dress>)> = super::super::bco::companies_of_world(company).iter().filter_map(world_dresses).filter(|x| !x.1.is_empty()).collect();
    if all.is_empty() {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for (slug, ds) in &all {
            dress_all(slug, ds);
        }
        let _ = tx.send(());
    });
    if rx.recv_timeout(WORLD_WAIT).is_err() {
        core::log_to_file("inspectors: the world's companies' clothes not all made yet - made on meanwhile, seen from the next game on");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axyz_s_people_come_first_and_drivers_never() {
        let dir = std::env::temp_dir().join(format!("omsihub-inspectors-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for f in ["Humans/aXYZ/man01.hum", "Humans/aXYZ/woman01.hum", "Humans/aXYZ/man02.hum", "Humans/Other/man04.hum", "Humans/Other/driver01.hum", "Humans/Other/child01.hum"] {
            let p = dir.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "").unwrap();
        }
        let got: Vec<String> = figures(&dir, None).iter().map(|p| p.strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/")).collect();
        assert_eq!(got, ["Humans/aXYZ/man02.hum", "Humans/aXYZ/man01.hum", "Humans/aXYZ/woman01.hum", "Humans/Other/man04.hum"]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!((hex("#F58220"), hex("x")), (Some([0xF5, 0x82, 0x20]), None));
    }

    /// (NexiconDK, 11 November 2026) A company of the server's list: its inspectors' clothes as
    /// its own launcher makes them - the same tag, look, colour and name, so the same items.
    #[test]
    fn a_listed_company_is_dressed_as_its_own_launcher_dresses_it() {
        let row = serde_json::json!({ "id": "x", "name": "Umove Aarhus", "short": "UMOVE", "colours": ["#F58220", "#222222"], "map": "Lemmental", "inspectors": { "slug": "umove", "looks": ["A", "B", "C"] } });
        let (slug, ds) = world_dresses(&row).unwrap();
        assert_eq!(slug, "umove");
        assert_eq!(ds, dresses_of("UMOVE", "Umove Aarhus", "#F58220", &['A', 'B', 'C']));
        assert_eq!(ds.iter().map(|d| d.look).collect::<String>(), "ABC");
        assert!(ds.iter().all(|d| d.colour == [0xF5, 0x82, 0x20] && d.company == "UMOVE"));
        // (a server of before lists none)
        assert!(world_dresses(&serde_json::json!({ "id": "x", "name": "Old" })).is_none());
        // (a short name too short: the name's first letters)
        assert_eq!(dresses_of("U", "Citybus Nord", "#123456", &['C'])[0].company, "Citybus Nord");
    }
}

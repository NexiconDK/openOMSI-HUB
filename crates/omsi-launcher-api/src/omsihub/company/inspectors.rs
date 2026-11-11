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
    let colour = hex(&c.colours[0]).unwrap_or([0xF5, 0x82, 0x20]);
    let name = if c.short.trim().chars().count() >= 2 { c.short.trim().to_string() } else { c.name.trim().chars().take(14).collect() };
    ig::letters(c).into_iter().map(|look| Dress { look, colour, company: name.clone(), text: "BILLETKONTROL".into() }).collect()
}

/// Dress the company's inspectors for the game (said in the launcher's log; the drive starts
/// whatever comes of it).
pub fn dress(c: &Company) {
    let (Ok(root), Some(content)) = (core::root_dir(), core::content_dir()) else { return };
    let slug = ig::slug(c);
    let all = figures(&root, Some(&content));
    for d in dresses(c) {
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
            match dress_figure(hum, &root, &content, &d, &slug) {
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
            core::log_to_file(&format!("inspectors: no figure for the look {} (they board in their own clothes)", d.look));
        }
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
}

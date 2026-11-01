//! `.hum` humans (unit `mc_human`).

use omsi_cfg::CfgFile;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Human {
    pub path: PathBuf,
    pub model: String,
    pub seat_height: f32,
    pub feet_dist: f32,
    pub height: f32,
    pub links: Vec<f32>,
    pub voice: String,
    pub walk_param: [f32; 5],
    pub mass: f32,
    pub age: Option<i32>,
    /// (NexiconDK, 1 November 2026) How the person gets about, when not on foot alone
    /// (`[openomsi_mobility]`, which Omsi.exe skips): in a wheelchair, or pushing a pram.
    pub mobility: Option<Mobility>,
    /// A pram's mesh of its own, after `pram` (relative to the person's model folder): drawn
    /// with the person, on the body as a mesh of the model is.
    pub pram_mesh: Option<String>,
}

/// How a person gets about (`Human::mobility`): such a person waits standing, boards a bus
/// only where its wheelchair or pram place is free, rides there and gets off by the door
/// nearest it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mobility {
    /// Sits in a wheelchair (the figure sits already: it never sits down, its legs never walk).
    Wheelchair,
    /// Pushes a pram (her arms on its handle: she never sits, and stands behind it in the bus).
    Pram,
}

impl Human {
    pub fn load(path: &Path) -> Result<Human, omsi_cfg::CfgError> {
        let f = CfgFile::read(path)?;
        let mut h = Human { path: f.path.clone(), walk_param: [1.4, 66.0, 1.0, 1.0, 0.0], ..Default::default() };
        let mut r = f.reader().disabled_blocks();
        while let Some(k) = r.next_keyword() {
            match k.as_str() {
                "model" => h.model = r.str().to_string(),
                "seatheight" => h.seat_height = r.f32(),
                "humangeom" => {
                    h.feet_dist = r.f32();
                    h.height = r.f32();
                }
                "links" => h.links = (0..22).map(|_| r.f32()).collect(),
                "voice" => h.voice = r.str().to_string(),
                "walk_param" => h.walk_param = r.f32s::<5>(),
                "mass" => h.mass = r.f32(),
                "age" => h.age = Some(r.i32()),
                "openomsi_mobility" => {
                    h.mobility = match r.str().trim().to_ascii_lowercase().as_str() {
                        "wheelchair" | "rollstuhl" => Some(Mobility::Wheelchair),
                        "pram" | "kinderwagen" => Some(Mobility::Pram),
                        _ => None,
                    };
                    // (a pram's own mesh on the next line, if one is given there)
                    if h.mobility == Some(Mobility::Pram) {
                        let save = r.pos();
                        let l = r.str().trim().to_string();
                        if l.is_empty() || omsi_cfg::keyword_of(&l).is_some() {
                            r.seek(save);
                        } else {
                            h.pram_mesh = Some(l);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (NexiconDK, 1 November 2026) `[openomsi_mobility]`: a wheelchair, or a pram with its own
    /// mesh or without; a person without it, or with a word it does not know, walks.
    #[test]
    fn a_person_may_get_about_in_a_wheelchair_or_with_a_pram() {
        let dir = std::env::temp_dir().join(format!("omsi-hum-mobility-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let read = |text: &str| {
            let p = dir.join("p.hum");
            std::fs::write(&p, text).unwrap();
            Human::load(&p).unwrap()
        };
        let base = "[model]\r\nmodel\\p.cfg\r\n[seatheight]\r\n0.77\r\n";
        let h = read(&format!("{base}[openomsi_mobility]\r\nwheelchair\r\n[mass]\r\n80\r\n"));
        assert_eq!((h.mobility, h.pram_mesh.as_deref(), h.mass), (Some(Mobility::Wheelchair), None, 80.0));
        let h = read(&format!("{base}[openomsi_mobility]\r\npram\r\nKinderwagen.o3d\r\n\r\n[mass]\r\n60\r\n"));
        assert_eq!((h.mobility, h.pram_mesh.as_deref(), h.mass), (Some(Mobility::Pram), Some("Kinderwagen.o3d"), 60.0));
        let h = read(&format!("{base}[openomsi_mobility]\r\nPram\r\n[mass]\r\n60\r\n"));
        assert_eq!((h.mobility, h.pram_mesh.as_deref(), h.mass), (Some(Mobility::Pram), None, 60.0));
        let h = read(&format!("{base}[openomsi_mobility]\r\nskateboard\r\n"));
        assert_eq!(h.mobility, None);
        assert_eq!(read(base).mobility, None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

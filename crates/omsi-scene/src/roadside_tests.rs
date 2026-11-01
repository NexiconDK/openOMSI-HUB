//! The cameras and displays the game puts by the road (`roadside`), on a map made for it.
use super::*;
use omsi_sim::roadside as rs;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!("openomsi-roadside-{}-{}", std::process::id(), NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn write(&self, rel: &str, text: &str) {
        let p = self.0.join(rel.replace('\\', "/"));
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, text).unwrap();
    }

    /// An installed model: its object with a small box of a mesh.
    fn model(&self, rel: &str) {
        self.write(rel, "[mesh]\nbox.x\n");
        let dir = rel.rsplit_once('\\').map(|d| d.0).unwrap_or("");
        self.write(&format!("{dir}\\model\\box.x"), BOX);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

const BOX: &str = "xof 0303txt 0032\nMesh box {\n4;\n-0.2;0;0;,\n0.2;0;0;,\n0.2;2;0;,\n-0.2;2;0;;\n2;\n3;0,1,2;,\n3;0,2,3;;\n}\n";

/// A spline lane as the tile's would be (tile 0,0, its forward path), for finding ids.
fn road(id: i64) -> Lane {
    let pts: Vec<DVec3> = (0..=50).map(|k| DVec3::new(0.0, k as f64 * 2.0, 0.0)).collect();
    let mut l = LaneBuilder::polyline(pts, LaneKind::Street, 3.5);
    l.key = Some(LaneKey { tile: (0, 0), id, path: 0 });
    l.source = 1;
    l
}

fn spline(id: i64, x: f64, y: f64) -> String {
    format!("[spline]\n0\nSplines\\road.sli\n{id}\n0\n0\n{x}\n0\n{y}\n0\n100\n0\n0\n0\n0\n0\n0\n0\n0\n\n")
}

/// The roadside objects staged: (kind, object id, file name, position, heading, strings).
fn roadside(world: &World, fx: &Fixture) -> Vec<(rs::Kind, i64, String, DVec3, f64, Vec<String>)> {
    let (prepared, _) = world.prepare_tiles(&[(0, 0, fx.0.join("tile_0_0.map"))]);
    let mut out: Vec<_> = prepared[0]
        .objects
        .iter()
        .filter_map(|o| {
            let kind = rs::kind_of_id(o.map_id)?;
            let name = o.ot.sco.path.file_name().unwrap().to_string_lossy().into_owned();
            Some((kind, o.map_id, name, o.pos, Pose { pos: o.pos, rot: o.xf }.heading(), o.strings.clone()))
        })
        .collect();
    // (a place's parts in order: their ids go down from its model's)
    out.sort_by_key(|o| std::cmp::Reverse(o.1));
    out
}

/// Seen: at the places of the tile's roads and lights the models the player has installed -
/// a camera beside the outermost lane turned to the traffic, a display on its pole with its
/// bracket, a red-light camera before its crossing's stop line - none where another road
/// passes; the add-ons he lacks are named. Unseen, or off: nothing stands.
#[test]
fn the_models_installed_stand_at_the_places_of_the_roads() {
    let speed: Vec<i64> = (1..100_000).filter(|id| rs::site(&road(*id)).is_some_and(|s| s.kind == rs::Kind::Speed)).take(3).collect();
    let display = (1..100_000).find(|id| rs::site(&road(*id)).is_some_and(|s| s.kind == rs::Kind::Display)).unwrap();
    let light_lane = |id: i64| {
        let mut l = road(id);
        l.source = 2;
        l.traffic_light = Some((0, 0));
        l
    };
    let crossing = (100_000..200_000).find(|id| rs::site(&light_lane(*id)).is_some()).unwrap();
    let fx = Fixture::new();
    fx.write("global.cfg", "[name]\nRoadside\n[map]\n0\n0\ntile_0_0.map\n");
    fx.write("Splines\\road.sli", "[path]\n0\n1.75\n0\n3.5\n0\n[path]\n0\n-1.75\n0\n3.5\n1\n");
    fx.write("Splines\\bus.sli", "[path]\n0\n1.75\n0\n3.5\n0\n[path]\n0\n5.25\n0\n3.5\n0\n[path]\n0\n-1.75\n0\n3.5\n1\n");
    // two roads with a camera (the second with a bus bay of its own spline beside it), one in
    // town with a display, a crossing with a light
    let mut tile = String::from("[version]\n14\n\n");
    tile += &spline(speed[0], 50.0, 100.0);
    tile += &spline(speed[1], 250.0, 100.0);
    tile += &spline(77_777_777, 255.0, 100.0);
    tile += &spline(display, 150.0, 100.0);
    // a road with a bus lane beside its lane, closed to cars
    tile += &spline(speed[2], 200.0, 100.0).replace("Splines\\road.sli", "Splines\\bus.sli");
    tile += "[rule]\n1\nno_cars\n1\n0\n\n";
    tile += &format!("[object]\n0\ncrossing.sco\n{crossing}\n100\n250\n2\n0\n0\n0\n0\n\n");
    fx.write("tile_0_0.map", &tile);
    fx.write("crossing.sco", "[absheight]\n[trafficlight]\n[traffic_light]\nA\n[phase]\n0\n30\n[path]\n0\n0\n0.1\n0\n0\n10\n0\n0\n0\n3\n0\n0\n[use_traffic_light]\n0\n");
    fx.model("Sceneryobjects\\Herr-Z\\Zubehoer\\Blitzer\\Blitzer_Star.sco");
    fx.model("Sceneryobjects\\Herr-Z\\Zubehoer\\Blitzer\\Blitzer_Ampel.sco");
    fx.model("Sceneryobjects\\Reutlingen_SH\\Geschwindigkeitsanzeigeanlage_Typ1.sco");
    fx.model("Sceneryobjects\\Reutlingen_SH\\GAA_Halterung_Solarpanel.sco");
    fx.write("Fonts\\GAA_T1_SH.oft", "[newfont]\n");
    let world = World::open(&fx.0, &fx.0.join("global.cfg"), 20261101).unwrap();
    assert_eq!(world.roadside_missing(), vec!["Blitzer_SH (Reutlingen_SH)", "Herr-Z Blitzer v1.1", "Ampelblitzer_SH (Reutlingen_SH)"]);
    // (a world of a tool: none)
    assert!(roadside(&world, &fx).is_empty());
    world.forget_all_staged();
    world.settings.set_roadside(roadside::UNSEEN);
    assert!(roadside(&world, &fx).is_empty());
    world.forget_all_staged();
    world.settings.set_roadside(roadside::SEEN);
    let objects = roadside(&world, &fx);
    let names: Vec<(rs::Kind, &str)> = objects.iter().map(|o| (o.0, o.2.as_str())).collect();
    assert_eq!(
        names.iter().filter(|n| n.0 == rs::Kind::Speed).count(),
        2,
        "two cameras: the third road's stands in its bay ({objects:?})"
    );
    // beside the bus lane, not in it (its centre 5.25 m right of the spline)
    let bus_road = objects.iter().find(|o| o.0 == rs::Kind::Speed && o.3.x > 190.0 && o.3.x < 220.0).expect("the bus lane road's camera");
    assert!((bus_road.3.x - (200.0 + 5.25 + 1.75 + 1.4)).abs() < 0.05, "{:?}", bus_road.3);
    // the camera: beside the right lane (its centre 1.75 m right of the spline, half its 3.5 m
    // and 1.4 m on), facing south (the Herr-Z models face their own forward: turned round)
    let cam = objects.iter().find(|o| o.0 == rs::Kind::Speed && o.3.x < 100.0).unwrap();
    let along = rs::site(&road(speed[0])).unwrap().along as f64;
    assert!((cam.3.x - (50.0 + 1.75 + 1.75 + 1.4)).abs() < 0.05 && (cam.3.y - (100.0 + along)).abs() < 0.2 && cam.3.z.abs() < 0.05, "{:?} along {along}", cam.3);
    assert!((cam.4 - 180.0).abs() < 0.5, "{}", cam.4);
    // the display, its bracket and its pole (the game's own, with a mesh)
    let shown: Vec<_> = objects.iter().filter(|o| o.0 == rs::Kind::Display).collect();
    assert_eq!(shown.iter().map(|o| o.2.as_str()).collect::<Vec<_>>(), vec!["Geschwindigkeitsanzeigeanlage_Typ1.sco", "GAA_Halterung_Solarpanel.sco", "GAA_Mast.sco"]);
    assert_eq!(shown[0].5, vec!["".to_string(), "50".into(), "1".into()]);
    assert!((shown[0].3.z - rs::DISPLAY_HEIGHT as f64).abs() < 0.05 && (shown[0].3.x - 154.9).abs() < 0.05, "{:?}", shown[0].3);
    assert!(shown[2].3.z.abs() < 0.05, "the pole stands on the ground: {:?}", shown[2].3);
    let pole = world.object_type(rs::POLE).expect("the pole's object");
    assert!(!pole.meshes.is_empty() && pole.meshes[0].0.positions.len() >= 25, "the pole has its mesh");
    // the red-light camera, a step before the stop line beside the light's lane
    let red: Vec<_> = objects.iter().filter(|o| o.0 == rs::Kind::RedLight).collect();
    assert_eq!(red.len(), 1, "{objects:?}");
    assert_eq!(red[0].2, "Blitzer_Ampel.sco");
    assert!((red[0].3 - DVec3::new(100.0 + 1.5 + 1.4, 250.0 - 1.0, 2.1)).length() < 0.05, "{:?}", red[0].3);
    // ... the same place as the light's lane the traffic is given has
    let lanes = world.traffic_inbox.peek(omsi_sim::world_data::TrafficArrivals::lanes_of);
    let placed = lanes.iter().find(|l| l.source == 2).expect("the crossing's lane");
    assert_eq!(rs::object_id(&rs::site(placed).expect("its camera"), 0), red[0].1);
}

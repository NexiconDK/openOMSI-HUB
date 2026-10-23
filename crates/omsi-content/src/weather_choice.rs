//! How a session's weather is chosen and written down: OMSI's cloud kinds, a hand-set weather
//! (`custom:...`, the launcher's and the game menu's sliders), the METAR airports and the
//! physical model's `natural`. Plain data on `weather::Weather`, shared by the launcher and the game.

use crate::weather::Weather;

/// OMSI's cloud types, in one place: the name a weather file writes (`Weather/clouds.cfg`),
/// the name the game shows and how much of the sky the type covers. The game's weather list,
/// the launcher's panel, the sky (`clouds_of`) and a hand-set weather all read it, so one kind
/// cannot mean two things (it was written out five times, and the same sky was "Cumulus 3" in
/// the launcher and "Broken" in the game).
pub struct CloudKind {
    /// the name a weather file writes
    pub id: &'static str,
    /// the name the game shows
    pub label: &'static str,
    /// how much of the sky the type covers
    pub cover: f32,
}

pub const CLOUD_KINDS: [CloudKind; 5] = [
    CloudKind { id: "-1", label: "None", cover: 0.0 },
    CloudKind { id: "Cumulus 1", label: "Few clouds", cover: 0.35 },
    CloudKind { id: "Cumulus 2", label: "Scattered", cover: 0.55 },
    CloudKind { id: "Cumulus 3", label: "Broken", cover: 0.75 },
    CloudKind { id: "Overcast 1", label: "Overcast", cover: 1.0 },
];

/// The index in `CLOUD_KINDS` of the kind `name` names: the name a weather file writes
/// (`Overcast 1`), the kind without its number (`Overcast`), the game's label (`Broken`), and
/// the empty or `-1` of "no clouds". None for a name the table does not hold (a sky pack's
/// own kinds in `envir.cfg`): the callers keep their own fallback.
pub fn cloud_kind_index(name: &str) -> Option<usize> {
    let n = name.trim();
    if n.is_empty() || n.starts_with("-1") || n.to_ascii_lowercase().starts_with("no cloud") {
        return Some(0);
    }
    CLOUD_KINDS.iter().position(|k| {
        k.id.eq_ignore_ascii_case(n) || k.label.eq_ignore_ascii_case(n) || (!k.id.starts_with('-') && k.id.split_whitespace().next().is_some_and(|w| w.eq_ignore_ascii_case(n)))
    })
}

/// The kind whose cover is nearest `cover` - one with clouds for any cover above none (the
/// name says which picture the sky is drawn with, and "no clouds" has none).
pub fn cloud_kind_nearest(cover: f32) -> usize {
    let cover = cover.clamp(0.0, 1.0);
    if cover <= 0.0 {
        return 0;
    }
    (1..CLOUD_KINDS.len()).min_by(|a, b| (CLOUD_KINDS[*a].cover - cover).abs().total_cmp(&(CLOUD_KINDS[*b].cover - cover).abs())).unwrap_or(1)
}

/// The cover a weather means, 0..1: its own where it set one (a hand-set weather), else its
/// cloud type's. One definition, so that what the panel shows, the sky and a blend's halfway
/// point cannot come out three different things.
pub fn cover_of(w: &Weather) -> f32 {
    if let Some(c) = w.cloud_cover {
        return c.clamp(0.0, 1.0);
    }
    let kind = w.clouds.0.trim();
    if kind.is_empty() || kind.starts_with("-1") {
        return 0.0;
    }
    // (a sky pack's own kind keeps the middle of the road rather than turning the clouds off)
    cloud_kind_index(kind).map(|i| CLOUD_KINDS[i].cover).unwrap_or(0.5)
}
pub const CUSTOM_PRECIP: [&str; 3] = ["None", "Rain", "Snow"];

/// (NexiconDK) How strong a thunderstorm can be set (`CustomWeather::thunder`), and what the
/// launcher and the game call it: none, one passing in the vicinity (flashes and rumble far off,
/// a METAR's `VCTS`), a thunderstorm (`TS`), a heavy one (`+TS`).
pub const THUNDER_LEVELS: [(f32, &str); 4] = [(0.0, "Off"), (0.3, "In the vicinity"), (0.7, "Thunderstorm"), (1.0, "Heavy thunderstorm")];

/// The entry of `THUNDER_LEVELS` nearest `level`.
pub fn thunder_index(level: f32) -> usize {
    if level <= 0.0 {
        return 0;
    }
    (1..THUNDER_LEVELS.len()).min_by(|a, b| (THUNDER_LEVELS[*a].0 - level).abs().total_cmp(&(THUNDER_LEVELS[*b].0 - level).abs())).unwrap_or(2)
}

/// (NexiconDK) How strong a thunderstorm the weather `w` has (0 none .. 1). A weather set by
/// hand (`custom:`) and a METAR report say it themselves (`Weather::thunder`), none included:
/// a hand-set weather with thunder off has none, however hard it rains. An `.owt` file has no
/// such entry: its name says so ("Gewitter", "thunder" ...), or towering clouds over a warm
/// downpour do, as the ambience took a thundery sky.
pub fn thunder_level(w: &Weather) -> f32 {
    let custom = w.path.to_string_lossy().to_ascii_lowercase().starts_with("custom:");
    if w.thunder > 0.0 || custom || w.name.starts_with("METAR ") {
        return w.thunder.clamp(0.0, 1.0);
    }
    let kind = w.precip.first().copied().unwrap_or(0.0) as i32;
    let rain = if kind == 1 { (w.precip.get(1).copied().unwrap_or(0.0) / 255.0).clamp(0.0, 1.0) } else { 0.0 };
    let text = format!("{} {} {}", w.name, w.clouds.0, w.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()).to_lowercase();
    let named = ["gewitter", "thunder", "onweer", "orage", "burza", "гроза", "torden", "åska", "temporale", "tormenta"].iter().any(|n| text.contains(n));
    if named {
        return (0.55 + 0.45 * rain).clamp(0.3, 1.0);
    }
    let c = w.clouds.0.to_ascii_lowercase();
    if (c.contains("cumulus 3") || c.contains("cumulonimbus")) && rain > 0.45 && w.temp.0 > 8.0 {
        return (0.4 + 0.6 * (rain - 0.45) / 0.55).clamp(0.3, 1.0);
    }
    0.0
}

#[derive(Debug, Clone, PartialEq)]
pub struct CustomWeather {
    pub visibility_m: f32,
    pub brightness: f32,
    pub wind_dir: f32,
    pub wind_speed: f32,
    pub temp_c: f32,
    pub humidity: f32,
    pub pressure: f32,
    /// How much of the sky the clouds cover, 0..1 (a slider: the sky reads it continuously).
    pub cloud_cover: f32,
    pub cloud_base_m: f32,
    pub precip: i32,
    pub precip_intensity: f32,
    pub road_wetness: f32,
    pub snow_cover: bool,
    pub snow_on_road: bool,
    /// (NexiconDK) A thunderstorm (0 none .. 1, see `THUNDER_LEVELS`): lightning and thunder,
    /// with rain or without.
    pub thunder: f32,
}
impl Default for CustomWeather {
    fn default() -> Self {
        Self { visibility_m: 50_000.0, brightness: 1.0, wind_dir: 0.0, wind_speed: 0.0,
            temp_c: 15.0, humidity: 51.0, pressure: 1013.0, cloud_cover: 0.0, cloud_base_m: 50.0,
            precip: 0, precip_intensity: 32.0, road_wetness: 0.0, snow_cover: false, snow_on_road: false, thunder: 0.0 }
    }
}

/// (NexiconDK) The ready thunderstorms the launcher and the game offer: a warm, humid
/// afternoon's under a low, heavy deck with a downpour and gusts, or (`dry`) one without rain -
/// a higher cloud base over drier air, the flashes and the thunder all the same. Starting
/// points: as hand-set weathers, everything of them can be changed.
pub fn thunderstorm(dry: bool) -> CustomWeather {
    if dry {
        CustomWeather { visibility_m: 25_000.0, brightness: 0.75, wind_dir: 220.0, wind_speed: 7.0, temp_c: 27.0, humidity: 45.0, pressure: 1006.0, cloud_cover: 0.8, cloud_base_m: 1600.0, thunder: 0.7, ..Default::default() }
    } else {
        CustomWeather { visibility_m: 5_000.0, brightness: 0.55, wind_dir: 230.0, wind_speed: 9.0, temp_c: 23.0, humidity: 82.0, pressure: 1004.0, cloud_cover: 0.95, cloud_base_m: 800.0, precip: 1, precip_intensity: 215.0, road_wetness: 0.85, thunder: 0.7, ..Default::default() }
    }
}
impl CustomWeather {
    pub fn normalize(&mut self) {
        self.visibility_m=self.visibility_m.clamp(50.0,50_000.0);
        self.brightness=self.brightness.clamp(0.0,1.5);
        self.wind_dir=self.wind_dir.rem_euclid(360.0);
        self.wind_speed=self.wind_speed.clamp(0.0,50.0);
        self.temp_c=self.temp_c.clamp(-40.0,50.0);
        self.humidity=self.humidity.clamp(0.0,100.0);
        self.pressure=self.pressure.clamp(900.0,1100.0);
        self.cloud_cover=self.cloud_cover.clamp(0.0,1.0);
        self.cloud_base_m=self.cloud_base_m.clamp(50.0,5000.0);
        self.precip=self.precip.clamp(0,2);
        self.precip_intensity=self.precip_intensity.clamp(0.0,255.0);
        self.road_wetness=self.road_wetness.clamp(0.0,1.0);
        self.thunder=if self.thunder.is_finite(){self.thunder.clamp(0.0,1.0)}else{0.0};
    }
    pub fn parse(text:&str)->Option<Self>{
        let body=text.strip_prefix("custom:").or_else(||text.strip_prefix("CUSTOM:"))?;
        let mut c=Self::default();
        for part in body.split(';') {
            let Some((key,value))=part.split_once('=') else {continue};
            let n=value.trim().parse::<f32>().ok();
            match key.trim().to_ascii_lowercase().as_str() {
                "vis"=>if let Some(v)=n{c.visibility_m=v},
                "br"=>if let Some(v)=n{c.brightness=v},
                "wd"=>if let Some(v)=n{c.wind_dir=v},
                "ws"=>if let Some(v)=n{c.wind_speed=v},
                "t"=>if let Some(v)=n{c.temp_c=v},
                "rh"=>if let Some(v)=n{c.humidity=v},
                "p"=>if let Some(v)=n{c.pressure=v},
                // (an older text kept the index of a cloud type here, written as a whole
                // number - `c={}` - where the cover is written with three decimals: a value
                // with no point in it is that index, read back as its kind's cover. Read as a
                // number, an old `c=1` - Cumulus 1 - was the 1.0 of a closed deck)
                "c"=>if let Some(v)=n{c.cloud_cover=if !value.trim().contains('.')||v>1.0{CLOUD_KINDS[(v.round().max(0.0) as usize).min(CLOUD_KINDS.len()-1)].cover}else{v}},
                "cb"=>if let Some(v)=n{c.cloud_base_m=v},
                "pt"=>if let Some(v)=n{c.precip=v.round() as i32},
                "pi"=>if let Some(v)=n{c.precip_intensity=v},
                "wet"=>if let Some(v)=n{c.road_wetness=v},
                "snow"=>if let Some(v)=n{c.snow_cover=v>=0.5},
                "snowroad"=>if let Some(v)=n{c.snow_on_road=v>=0.5},
                "ts"=>if let Some(v)=n{c.thunder=v},
                _=>{}
            }
        }
        c.normalize(); Some(c)
    }
    pub fn encode(&self)->String{
        let mut c=self.clone(); c.normalize();
        format!("custom:vis={:.0};br={:.2};wd={:.0};ws={:.1};t={:.1};rh={:.0};p={:.0};c={:.3};cb={:.0};pt={};pi={:.0};wet={:.2};snow={};snowroad={}",
            c.visibility_m,c.brightness,c.wind_dir,c.wind_speed,c.temp_c,c.humidity,c.pressure,c.cloud_cover,c.cloud_base_m,
            c.precip,c.precip_intensity,c.road_wetness,c.snow_cover as u8,c.snow_on_road as u8)
            // (a thunderstorm only where there is one: the texts of before stay as they were)
            +&if c.thunder>0.0{format!(";ts={:.2}",c.thunder)}else{String::new()}
    }
    pub fn from_weather(w:&Weather,brightness:f32,wetness:f32)->Self{
        let cloud_cover=cover_of(w);
        let mut c=Self{
            visibility_m:w.fog.0,brightness,wind_dir:w.wind.0,wind_speed:w.wind.1,temp_c:w.temp.0,
            humidity:relative_humidity(w.temp.0,w.temp.1),pressure:if w.pressure>0.0{w.pressure}else{1013.0},
            cloud_cover,cloud_base_m:w.clouds.1.max(50.0),precip:w.precip.first().copied().unwrap_or(0.0).round() as i32,
            precip_intensity:w.precip.get(1).copied().unwrap_or(32.0),road_wetness:wetness,snow_cover:w.snow,snow_on_road:w.snow_on_road,
            thunder:thunder_level(w)};
        c.normalize(); c
    }
    pub fn to_weather(&self)->Weather{
        let mut c=self.clone(); c.normalize();
        let cloud=CLOUD_KINDS[cloud_kind_nearest(c.cloud_cover)].id;
        Weather{
            path:std::path::PathBuf::from(c.encode()),name:"Custom weather".into(),description:"User-defined weather".into(),
            fog:(c.visibility_m,1.0),wind:(c.wind_dir,c.wind_speed),temp:(c.temp_c,absolute_humidity(c.temp_c,c.humidity)),
            pressure:c.pressure,clouds:(cloud.into(),c.cloud_base_m),cloud_cover:Some(c.cloud_cover),precip:vec![c.precip as f32,c.precip_intensity,0.0,0.0,0.0],
            ground_wet:[c.road_wetness*255.0,0.0,0.0],snow:c.snow_cover,snow_on_road:c.snow_on_road,thunder:c.thunder}
    }
}
fn saturation_vapour_pressure(temp_c:f32)->f32{6.112*((17.67*temp_c)/(temp_c+243.5)).exp()}
pub fn absolute_humidity(temp_c:f32,relative:f32)->f32{
    let vapour=saturation_vapour_pressure(temp_c)*relative.clamp(0.0,100.0)/100.0;
    (216.7*vapour/(temp_c+273.15).max(1.0)).max(0.0)
}
pub fn relative_humidity(temp_c:f32,absolute:f32)->f32{
    let vapour=absolute.max(0.0)*(temp_c+273.15).max(1.0)/216.7;
    (vapour/saturation_vapour_pressure(temp_c).max(0.001)*100.0).clamp(0.0,100.0)
}
pub fn dew_point_c(temp_c:f32,relative:f32)->f32{
    let rh=(relative.clamp(0.1,100.0)/100.0).ln();
    let g=rh+17.67*temp_c/(243.5+temp_c); 243.5*g/(17.67-g)
}
pub fn custom_weather(text:Option<&str>)->Option<CustomWeather>{text.and_then(CustomWeather::parse)}

/// The airports OMSI's METAR list (`Weather/ICAO.txt`) offers: (ICAO, "ICAO - name").
pub fn metar_airports(root:&std::path::Path)->Vec<(String,String)>{
    static CACHE:std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<std::path::PathBuf,Vec<(String,String)>>>>=std::sync::OnceLock::new();
    let cache=CACHE.get_or_init(||std::sync::Mutex::new(std::collections::HashMap::new()));
    if let Some(v)=cache.lock().unwrap_or_else(|e|e.into_inner()).get(root).cloned(){return v}
    let text=omsi_cfg::vfs::read(&omsi_cfg::resolve_path(root,"Weather/ICAO.txt")).map(|b|omsi_cfg::codepage::decode(&b)).unwrap_or_default();
    let mut v:Vec<(String,String)>=text.lines().filter_map(|l|l.split_once(" - ").map(|(c,n)|(c.trim().to_ascii_uppercase(),format!("{} - {}",c.trim(),n.trim()))))
        .filter(|(c,_)|c.len()==4&&c.chars().all(|x|x.is_ascii_alphabetic())).collect();
    if !v.iter().any(|a|a.0=="EDDB"){v.push(("EDDB".into(),"EDDB - Berlin Brandenburg".into()))}
    v.sort_by(|a,b|a.0.cmp(&b.0)); v.dedup_by(|a,b|a.0==b.0);
    cache.lock().unwrap_or_else(|e|e.into_inner()).insert(root.to_path_buf(),v.clone()); v
}

/// Whether a weather choice means the physical model: `natural`, or none at all.
pub fn is_natural(choice: Option<&str>) -> bool {
    choice.is_none_or(|w| w.trim().eq_ignore_ascii_case("natural"))
}

#[cfg(test)]
mod thunder_tests {
    use super::*;

    /// A hand-set weather keeps its thunderstorm through its text, with rain or without; a text
    /// of before (no `ts`) reads as none, and one without thunder is written as before.
    #[test]
    fn a_hand_set_thunderstorm_goes_through_its_text() {
        for dry in [false, true] {
            let c = thunderstorm(dry);
            let text = c.encode();
            assert!(text.ends_with(";ts=0.70"), "{text}");
            let back = CustomWeather::parse(&text).unwrap();
            assert_eq!(back.thunder, 0.7);
            assert_eq!((back.precip == 0), dry, "the dry one has no rain");
            let w = back.to_weather();
            assert_eq!((w.thunder, thunder_level(&w)), (0.7, 0.7));
        }
        let plain = CustomWeather::default().encode();
        assert!(!plain.contains("ts="));
        assert_eq!(CustomWeather::parse(&plain).unwrap().thunder, 0.0);
        assert_eq!(THUNDER_LEVELS[thunder_index(0.3)].1, "In the vicinity");
        assert_eq!(thunder_index(0.0), 0);
        assert_eq!(thunder_index(0.95), 3);
    }

    /// Thunder off in a hand-set weather is off, however hard and warm it rains; a weather
    /// file's own downpour under towering clouds is still a thunderstorm, and edited by hand
    /// it stays one.
    #[test]
    fn a_hand_set_weather_decides_its_thunder_itself() {
        let downpour = CustomWeather { temp_c: 24.0, cloud_cover: 0.75, precip: 1, precip_intensity: 230.0, ..Default::default() };
        assert_eq!(thunder_level(&downpour.to_weather()), 0.0);
        let file = Weather { path: "Weather/Schauer.owt".into(), name: "Schauer".into(), clouds: ("Cumulus 3".into(), 900.0), temp: (24.0, 12.0), precip: vec![1.0, 230.0], ..Default::default() };
        let level = thunder_level(&file);
        assert!(level > 0.5, "{level}");
        assert!((CustomWeather::from_weather(&file, 1.0, 0.5).thunder - level).abs() < 1e-6);
        let report = Weather { name: "METAR EKCH".into(), ..file.clone() };
        assert_eq!(thunder_level(&report), 0.0, "a report without TS has no thunder");
    }
}

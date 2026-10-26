//! (NexiconDK) Bus Company Online phase 6 step 2 (docs/omsihub/BUS_COMPANY_ONLINE.md): the maps'
//! worlds kept running while the machine can carry them (decided 26 October 2026). With
//! `keep_worlds = 1` a world does not stop by itself for want of players (its `idle_stop` is 0);
//! the main server looks at the machine every half minute (`Machine`): over `max_memory` per cent
//! of its memory in use, or `max_cpu` per cent of its processors busy (on average over the last
//! minute), the world empty the longest is stopped (`to_stop`); a world asked for when
//! `max_worlds` run already takes the place of the one empty the longest.

use std::collections::HashMap;

/// How the worlds are kept (`company-server.cfg`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keep {
    /// Kept running while the machine can (else each stops `world_idle` minutes after its last
    /// player, by itself).
    pub on: bool,
    /// The machine's memory in use (per cent) and its processors busy (per cent, over the last
    /// minute) over which an empty world is stopped.
    pub max_memory: u32,
    pub max_cpu: u32,
}

impl Default for Keep {
    fn default() -> Keep {
        Keep { on: true, max_memory: 85, max_cpu: 90 }
    }
}

impl Keep {
    /// The settings' keys (`company-server.cfg`, lower case).
    pub fn of(kv: &HashMap<String, String>) -> Keep {
        let d = Keep::default();
        let num = |k: &str, d: u32| kv.get(k).and_then(|v| v.trim().parse::<u32>().ok()).map_or(d, |n| n.clamp(10, 100));
        Keep {
            on: kv.get("keep_worlds").map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")).unwrap_or(d.on),
            max_memory: num("max_memory", d.max_memory),
            max_cpu: num("max_cpu", d.max_cpu),
        }
    }
}

/// The worlds' part of the settings, added to a `company-server.cfg` written before it.
pub const KEEP_CFG: &str = "
# the maps' worlds kept running without players while the machine can carry them (1), or each
# stopped world_idle minutes after its last player (0); over max_memory per cent of the
# machine's memory in use or max_cpu per cent of its processors busy, the world empty the
# longest is stopped
keep_worlds = 1
max_memory = 85
max_cpu = 90
";

/// How long the processors' load is averaged over (s), and how often the machine is looked at.
pub const CPU_WINDOW: f64 = 60.0;
pub const LOOK_EVERY: f64 = 30.0;

/// The machine as the main server watches it: its processors' times a while back, to average
/// their load over `CPU_WINDOW`.
#[derive(Default)]
pub struct Machine {
    /// (Unix seconds, busy, total) of the processors, oldest first.
    samples: Vec<(f64, u64, u64)>,
    /// When it was looked at last (Unix seconds).
    pub looked: f64,
}

/// What the machine carries now.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Load {
    /// Its memory in use (per cent; None: not known on this system).
    pub memory: Option<u32>,
    /// Its processors busy (per cent, over the last minute; None: not known yet).
    pub cpu: Option<u32>,
}

impl Load {
    /// Over the limits of `k`.
    pub fn over(&self, k: &Keep) -> bool {
        self.memory.is_some_and(|m| m > k.max_memory) || self.cpu.is_some_and(|c| c > k.max_cpu)
    }

    /// In words, for the log and `status`.
    pub fn text(&self) -> String {
        let p = |v: Option<u32>| v.map_or("?".to_string(), |v| format!("{v} %"));
        format!("memory {}, processors {}", p(self.memory), p(self.cpu))
    }
}

impl Machine {
    /// The machine's load now (`now`: Unix seconds): its memory, and its processors over the
    /// samples of the last `CPU_WINDOW` (this one kept).
    pub fn load(&mut self, now: f64) -> Load {
        if let Some((busy, total)) = cpu_times() {
            self.samples.push((now, busy, total));
        }
        self.samples.retain(|s| now - s.0 <= CPU_WINDOW + LOOK_EVERY);
        Load { memory: memory_in_use(), cpu: cpu_of(&self.samples, now) }
    }
}

/// The processors' load (per cent) from the oldest sample within `CPU_WINDOW` (and a little) to
/// the newest; None with one sample.
fn cpu_of(samples: &[(f64, u64, u64)], now: f64) -> Option<u32> {
    let first = samples.iter().find(|s| now - s.0 <= CPU_WINDOW + LOOK_EVERY)?;
    let last = samples.last()?;
    let (busy, total) = (last.1.checked_sub(first.1)?, last.2.checked_sub(first.2)?);
    (total > 0).then(|| ((busy as f64 / total as f64) * 100.0).round().clamp(0.0, 100.0) as u32)
}

/// Of the worlds - each its map, its players now and since when it has been empty (Unix seconds;
/// None: players) - the one to stop: the one empty the longest. None: every world has players.
pub fn to_stop<'a>(worlds: &[(&'a str, Option<f64>)]) -> Option<&'a str> {
    worlds.iter().filter_map(|(map, since)| since.map(|s| (*map, s))).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(m, _)| m)
}

/// The machine's memory in use (per cent).
#[cfg(target_os = "linux")]
pub fn memory_in_use() -> Option<u32> {
    let text = std::fs::read_to_string("/proc/meminfo").ok()?;
    let kb = |key: &str| text.lines().find(|l| l.starts_with(key)).and_then(|l| l.split_whitespace().nth(1)).and_then(|v| v.parse::<u64>().ok());
    let (total, free) = (kb("MemTotal:")?, kb("MemAvailable:")?);
    (total > 0).then(|| (((total.saturating_sub(free)) as f64 / total as f64) * 100.0).round() as u32)
}

#[cfg(windows)]
pub fn memory_in_use() -> Option<u32> {
    use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    // SAFETY: a MEMORYSTATUSEX with its length set, as the call wants it
    unsafe { GlobalMemoryStatusEx(&mut m) }.ok()?;
    Some(m.dwMemoryLoad)
}

#[cfg(not(any(target_os = "linux", windows)))]
pub fn memory_in_use() -> Option<u32> {
    None
}

/// The processors' busy and total times since the machine started (any unit).
#[cfg(target_os = "linux")]
fn cpu_times() -> Option<(u64, u64)> {
    let text = std::fs::read_to_string("/proc/stat").ok()?;
    let line = text.lines().find(|l| l.starts_with("cpu "))?;
    let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
    // (user nice system idle iowait irq softirq steal: idle and iowait are not busy)
    let total: u64 = v.iter().take(8).sum();
    let idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
    Some((total.saturating_sub(idle), total))
}

#[cfg(windows)]
fn cpu_times() -> Option<(u64, u64)> {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::GetSystemTimes;
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: three FILETIMEs the call fills
    unsafe { GetSystemTimes(Some(std::ptr::addr_of_mut!(idle)), Some(std::ptr::addr_of_mut!(kernel)), Some(std::ptr::addr_of_mut!(user))) }.ok()?;
    let t = |f: &FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64;
    // (the kernel's time holds the idle time)
    let total = t(&kernel) + t(&user);
    Some((total.saturating_sub(t(&idle)), total))
}

#[cfg(not(any(target_os = "linux", windows)))]
fn cpu_times() -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kv(text: &str) -> HashMap<String, String> {
        text.lines().filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect()
    }

    /// The settings, the machine's load against them, and which world goes: the one empty the
    /// longest, none while every world has players.
    #[test]
    fn the_world_empty_the_longest_goes_when_the_machine_is_full() {
        assert_eq!(Keep::of(&kv(KEEP_CFG)), Keep::default());
        assert_eq!(Keep::of(&kv("keep_worlds = 0\nmax_memory = 70\nmax_cpu = 5")), Keep { on: false, max_memory: 70, max_cpu: 10 });
        let k = Keep::default();
        assert!(!Load { memory: Some(85), cpu: Some(90) }.over(&k));
        assert!(Load { memory: Some(86), cpu: None }.over(&k) && Load { memory: None, cpu: Some(91) }.over(&k));
        assert!(!Load { memory: None, cpu: None }.over(&k), "not known: nothing stopped");
        assert_eq!(Load { memory: Some(40), cpu: None }.text(), "memory 40 %, processors ?");
        // (the processors over the samples of the last minute)
        let s = vec![(0.0, 100, 1000), (30.0, 400, 2000), (60.0, 700, 3000)];
        assert_eq!(cpu_of(&s, 60.0), Some(30));
        assert_eq!(cpu_of(&s[..1], 0.0), None, "one sample: not known yet");
        assert_eq!(cpu_of(&s, 200.0), None, "all of them too old");
        assert_eq!(to_stop(&[("Lemmental", None), ("Ahlheim", Some(500.0)), ("Grundorf", Some(200.0))]), Some("Grundorf"));
        assert_eq!(to_stop(&[("Lemmental", None)]), None);
        // (this machine: its memory known on Linux and Windows)
        if cfg!(any(target_os = "linux", windows)) {
            assert!(memory_in_use().is_some_and(|m| m <= 100));
            let mut m = Machine::default();
            m.load(0.0);
            assert!(m.load(1.0).cpu.is_none_or(|c| c <= 100));
        }
    }
}

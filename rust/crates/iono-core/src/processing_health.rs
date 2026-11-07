//! Receiver-local operational counters. These are not scientific observations.
//! Count scheduler losses, never low SNR, inactive schedules or startup catch-up.
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf, time::Instant};
const WINDOW: f64 = 900.;
fn now() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64 / 1000.
}
fn directory() -> Option<PathBuf> {
    Some(
        std::env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".local/state")))?
            .join("iono-processing"),
    )
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Loss {
    time: f64,
    soundings: u64,
    dwells: u64,
    cpu: bool,
}
#[derive(Default, Serialize, Deserialize)]
struct Report {
    worker: String,
    time: f64,
    events: Vec<Loss>,
}
#[derive(Clone, Copy, Default)]
pub struct Sample {
    run: f64,
    wait: f64,
    throttle: f64,
}
impl Sample {
    pub fn capture() -> Self {
        let v: Vec<f64> = fs::read_to_string("/proc/self/schedstat")
            .unwrap_or_default()
            .split_whitespace()
            .take(2)
            .filter_map(|v| v.parse().ok())
            .collect();
        let mut throttle: f64 = 0.;
        if let Ok(cg) = fs::read_to_string("/proc/self/cgroup") {
            if let Some(path) = cg.lines().find_map(|l| l.strip_prefix("0::")) {
                let root = PathBuf::from("/sys/fs/cgroup");
                let mut p = root.join(path.trim_start_matches('/'));
                // Include inherited slice quotas, without counting time twice.
                for _ in 0..5 {
                    if let Ok(s) = fs::read_to_string(p.join("cpu.stat")) {
                        if let Some(n) = s.lines().find_map(|l| {
                            l.strip_prefix("throttled_usec ")
                                .and_then(|s| s.parse::<f64>().ok())
                        }) {
                            throttle = throttle.max(n * 1e-6);
                        }
                    }
                    if p == root || !p.pop() {
                        break;
                    }
                }
            }
        }
        Self {
            run: v.first().copied().unwrap_or(0.) * 1e-9,
            wait: v.get(1).copied().unwrap_or(0.) * 1e-9,
            throttle,
        }
    }
    fn cpu_limited(self, end: Self, wall: f64, budget: f64) -> bool {
        wall > budget
            && (end.run - self.run >= budget * 0.8
                || end.wait - self.wait >= (budget * 0.1).max(0.05)
                || end.throttle - self.throttle >= (budget * 0.1).max(0.05))
    }
}
pub struct Monitor {
    path: Option<PathBuf>,
    report: Report,
    active: bool,
    cpu: bool,
    last_write: Instant,
}
impl Monitor {
    pub fn new(key: &str, live: bool) -> Self {
        let worker: String = key
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || "-_".contains(*c))
            .take(120)
            .collect();
        let path = live
            .then(directory)
            .flatten()
            .map(|d| d.join(format!("{worker}.json")));
        let mut report = path
            .as_ref()
            .and_then(|p| fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<Report>(&b).ok())
            .unwrap_or_default();
        report.worker = worker;
        let mut m = Self {
            path,
            report,
            active: false,
            cpu: false,
            last_write: Instant::now(),
        };
        m.publish();
        m
    }
    /// Call around actual processing, excluding deliberate schedule waits.
    pub fn finished(&mut self, before: Sample, elapsed: f64, budget: f64) {
        self.active = true;
        self.cpu = before.cpu_limited(Sample::capture(), elapsed, budget);
        if self.last_write.elapsed().as_secs() >= 30 {
            self.publish();
        }
    }
    pub fn missed(&mut self, soundings: u64, dwells: u64) {
        if !self.active || soundings + dwells == 0 {
            return;
        }
        let time = now();
        if let Some(last) = self
            .report
            .events
            .last_mut()
            .filter(|e| e.cpu == self.cpu && time - e.time < 30.)
        {
            last.soundings += soundings;
            last.dwells += dwells;
        } else {
            self.report.events.push(Loss {
                time,
                soundings,
                dwells,
                cpu: self.cpu,
            });
        }
        self.publish();
    }
    fn publish(&mut self) {
        self.report.time = now();
        self.report
            .events
            .retain(|e| self.report.time - e.time <= WINDOW);
        let Some(path) = &self.path else {
            return;
        };
        let result = (|| -> anyhow::Result<()> {
            fs::create_dir_all(path.parent().unwrap())?;
            let tmp = path.with_extension(format!("{}.partial", std::process::id()));
            fs::write(&tmp, serde_json::to_vec(&self.report)?)?;
            fs::rename(tmp, path)?;
            Ok(())
        })();
        if let Err(e) = result {
            eprintln!("processing health unavailable: {e:#}");
        }
        self.last_write = Instant::now();
    }
}
fn summarize(reports: impl Iterator<Item = Report>, time: f64) -> serde_json::Value {
    let workers: Vec<_> = reports.filter(|r| r.time <= time+120. && time-r.time <= 3600.).map(|r| {
        let events: Vec<_> = r.events.into_iter().filter(|e| e.time <= time+120. && time-e.time <= WINDOW).collect();
        serde_json::json!({"worker":r.worker,"reported_unix":r.time,
          "missed_soundings":events.iter().map(|e|e.soundings).sum::<u64>(),
          "missed_dwells":events.iter().map(|e|e.dwells).sum::<u64>(),
          "cpu_limited_losses":events.iter().filter(|e|e.cpu).map(|e|e.soundings+e.dwells).sum::<u64>(),
          "last_miss_unix":events.iter().map(|e|e.time).reduce(f64::max)})
    }).collect();
    serde_json::json!({"window_s":WINDOW,"generated_unix":time,"workers":workers})
}
pub fn summary() -> serde_json::Value {
    let reports: Vec<Report> = directory()
        .and_then(|p| fs::read_dir(p).ok())
        .into_iter()
        .flatten()
        .flatten()
        .take(128)
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| {
            if e.metadata().ok()?.len() > 65536 {
                return None;
            }
            serde_json::from_slice(&fs::read(e.path()).ok()?).ok()
        })
        .collect();
    summarize(reports.into_iter(), now())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn losses_expire() {
        let r = Report {
            worker: "prn-Kiruna".into(),
            time: 10000.,
            events: vec![
                Loss {
                    time: 8000.,
                    soundings: 3,
                    ..Default::default()
                },
                Loss {
                    time: 9900.,
                    dwells: 4,
                    cpu: true,
                    ..Default::default()
                },
            ],
        };
        let s = summarize(std::iter::once(r), 10000.);
        let w = &s["workers"][0];
        assert_eq!(w["missed_soundings"], 0);
        assert_eq!(w["missed_dwells"], 4);
        assert_eq!(w["cpu_limited_losses"], 4);
    }
    #[test]
    fn startup_catch_up_does_not_warn_but_active_losses_do() {
        let mut m = Monitor {
            path: None,
            report: Report::default(),
            active: false,
            cpu: false,
            last_write: Instant::now(),
        };
        m.missed(2, 3);
        assert!(m.report.events.is_empty());
        m.active = true;
        m.missed(1, 4);
        m.missed(0, 2);
        assert_eq!(m.report.events.len(), 1);
        assert_eq!(m.report.events[0].dwells, 6);
        assert!(!m.report.events[0].cpu);
    }
    #[test]
    fn io_wait_alone_is_not_cpu_evidence() {
        let a = Sample::default();
        assert!(!a.cpu_limited(Sample { run: 0.1, ..a }, 3., 1.));
        assert!(a.cpu_limited(Sample { run: 1.2, ..a }, 3., 1.));
        assert!(a.cpu_limited(Sample { wait: 0.3, ..a }, 3., 1.));
        assert!(a.cpu_limited(Sample { throttle: 0.3, ..a }, 3., 1.));
        assert!(!a.cpu_limited(Sample { run: 0.9, ..a }, 0.9, 1.));
    }
}

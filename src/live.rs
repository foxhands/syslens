//! Live load: processor, memory, network and disks, sampled once a second
//! while the "load now" page is open.

use std::collections::VecDeque;
use std::time::{Duration, Instant};
use sysinfo::{Disks, Networks, System};

/// Time between samples.
pub const PERIOD: Duration = Duration::from_secs(1);
/// How many samples the processor chart keeps (one minute).
pub const HISTORY: usize = 60;

pub struct NetRate {
    pub name: String,
    /// bytes per second
    pub down: f64,
    pub up: f64,
}

pub struct DiskUse {
    pub name: String,
    pub total: u64,
    pub free: u64,
}

pub struct Live {
    sys: System,
    nets: Networks,
    disks: Disks,
    last: Option<Instant>,
    /// whole processor, 0..100
    pub cpu: f32,
    pub cores: Vec<f32>,
    pub history: VecDeque<f32>,
    pub mem_total: u64,
    pub mem_used: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub nets_now: Vec<NetRate>,
    pub disks_now: Vec<DiskUse>,
}

impl Live {
    pub fn new() -> Self {
        let mut sys = System::new();
        // Processor load is the change between two readings; this is the first.
        sys.refresh_cpu_usage();
        Live {
            sys,
            nets: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
            last: None,
            cpu: 0.0,
            cores: Vec::new(),
            history: VecDeque::new(),
            mem_total: 0,
            mem_used: 0,
            swap_total: 0,
            swap_used: 0,
            nets_now: Vec::new(),
            disks_now: Vec::new(),
        }
    }

    /// Takes a new sample when it is due. Returns how long to wait for the next one.
    pub fn tick(&mut self) -> Duration {
        let now = Instant::now();
        let since = self.last.map(|t| now.duration_since(t));
        if let Some(dt) = since {
            if dt < PERIOD {
                return PERIOD - dt;
            }
        }
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.nets.refresh(true);
        self.disks.refresh(true);

        if since.is_some() {
            // the very first reading has nothing to compare with
            self.cpu = self.sys.global_cpu_usage();
            self.cores = self.sys.cpus().iter().map(|c| c.cpu_usage()).collect();
            self.history.push_back(self.cpu);
            while self.history.len() > HISTORY {
                self.history.pop_front();
            }
        }
        self.mem_total = self.sys.total_memory();
        self.mem_used = self.sys.used_memory();
        self.swap_total = self.sys.total_swap();
        self.swap_used = self.sys.used_swap();

        let secs = since.map(|d| d.as_secs_f64()).unwrap_or(0.0);
        let mut nets: Vec<NetRate> = self
            .nets
            .iter()
            .filter(|(_, d)| d.total_received() + d.total_transmitted() > 0)
            .map(|(name, d)| NetRate {
                name: name.clone(),
                down: if secs > 0.0 { d.received() as f64 / secs } else { 0.0 },
                up: if secs > 0.0 { d.transmitted() as f64 / secs } else { 0.0 },
            })
            .collect();
        nets.sort_by(|a, b| a.name.cmp(&b.name));
        self.nets_now = nets;

        self.disks_now = self
            .disks
            .iter()
            .filter(|d| d.total_space() > 0)
            .map(|d| DiskUse {
                name: d.mount_point().to_string_lossy().into_owned(),
                total: d.total_space(),
                free: d.available_space(),
            })
            .collect();

        self.last = Some(now);
        PERIOD
    }
}

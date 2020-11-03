use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use sysinfo::{NetworkExt, NetworksExt, System as SystemInfo, SystemExt};
use systemstat::{Platform, System as SystemStat};

#[cfg(target_os = "macos")]
use psutil::process::Process;
#[cfg(target_os = "linux")]
use psutil::process::Process;

/// The two paths to the two core Lighthouse databases.
#[derive(Debug, Clone, PartialEq)]
pub struct DBPaths {
    pub chain_db: PathBuf,
    pub freezer_db: PathBuf,
}

/// Contains information about a file system mount.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MountInfo {
    avail: u64,
    total: u64,
    used: u64,
    used_pct: f64,
    mounted_on: PathBuf,
}

impl MountInfo {
    /// Attempts to find the `MountInfo` for the given `path`.
    pub fn for_path<P: AsRef<Path>>(path: P) -> Result<Option<Self>, String> {
        let system = SystemStat::new();
        let mounts = system
            .mounts()
            .map_err(|e| format!("Unable to enumerate mounts: {:?}", e))?;

        let mut mounts = mounts
            .iter()
            .filter_map(|drive| {
                let mount_path = Path::new(&drive.fs_mounted_on);
                let num_components = mount_path.iter().count();

                Some((drive, mount_path, num_components))
                    .filter(|_| path.as_ref().starts_with(&mount_path))
            })
            .collect::<Vec<_>>();

        // Sort the list of mount points, such that the path with the most components is first.
        //
        // For example:
        //
        // ```
        // let mounts = ["/home/paul", "/home", "/"];
        // ```
        //
        // The intention here is to find the "closest" mount-point to `path`, such that
        // `/home/paul/file` matches `/home/paul`, not `/` or `/home`.
        mounts.sort_unstable_by(|(_, _, a), (_, _, b)| b.cmp(a));

        let disk_usage = mounts.first().map(|(drive, mount_path, _)| {
            let avail = drive.avail.as_u64();
            let total = drive.total.as_u64();
            let used = total.saturating_sub(avail);
            let mut used_pct = if total > 0 {
                used as f64 / total as f64
            } else {
                0.0
            } * 100.0;

            // Round to two decimals.
            used_pct = (used_pct * 100.00).round() / 100.00;

            Self {
                avail,
                total,
                used,
                used_pct,
                mounted_on: mount_path.into(),
            }
        });

        Ok(disk_usage)
    }
}

/// Reports information about the network on the system the Lighthouse instance is running on.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Network {
    /// Network metric for total received bytes across all network interfaces.
    pub rx_bytes: u64,
    /// Network metric for total received errors across all network interfaces.
    pub rx_errors: u64,
    /// Network metric for total received packets across all network interfaces.
    pub rx_packets: u64,
    /// Network metric for total transmitted bytes across all network interfaces.
    pub tx_bytes: u64,
    /// Network metric for total trasmitted errors across all network interfaces.
    pub tx_errors: u64,
    /// Network metric for total transmitted packets across all network interfaces.
    pub tx_packets: u64,
}

impl Network {
    pub fn observe() -> Result<Self, String> {
        let mut rx_bytes = 0;
        let mut rx_errors = 0;
        let mut rx_packets = 0;
        let mut tx_bytes = 0;
        let mut tx_errors = 0;
        let mut tx_packets = 0;

        let s = SystemInfo::new_all();
        s.get_networks().iter().for_each(|(_, network)| {
            rx_bytes += network.get_total_received();
            rx_errors += network.get_total_transmitted();
            rx_packets += network.get_total_packets_received();
            tx_bytes += network.get_total_packets_transmitted();
            tx_errors += network.get_total_errors_on_received();
            tx_packets += network.get_total_errors_on_transmitted();
        });

        Ok(Network {
            rx_bytes,
            rx_errors,
            rx_packets,
            tx_bytes,
            tx_errors,
            tx_packets,
        })
    }
}

/// Reports on the health of the Lighthouse instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommonHealth {
    /// The pid of this process.
    pub pid: u32,
    /// The total resident memory used by this pid.
    pub pid_mem_resident_set_size: u64,
    /// The total virtual memory used by this pid.
    pub pid_mem_virtual_memory_size: u64,
    /// Total virtual memory on the system
    pub sys_virt_mem_total: u64,
    /// Total virtual memory available for new processes.
    pub sys_virt_mem_available: u64,
    /// Total virtual memory used on the system
    pub sys_virt_mem_used: u64,
    /// Total virtual memory not used on the system
    pub sys_virt_mem_free: u64,
    /// Percentage of virtual memory used on the system
    pub sys_virt_mem_percent: f32,
    /// System load average over 1 minute.
    pub sys_loadavg_1: f64,
    /// System load average over 5 minutes.
    pub sys_loadavg_5: f64,
    /// System load average over 15 minutes.
    pub sys_loadavg_15: f64,
}

impl CommonHealth {
    #[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
    pub fn observe() -> Result<Self, String> {
        Err("Health is only available on Linux and MacOS".into())
    }

    #[cfg(target_os = "linux")]
    pub fn observe() -> Result<Self, String> {
        let process =
            Process::current().map_err(|e| format!("Unable to get current process: {:?}", e))?;

        let process_mem = process
            .memory_info()
            .map_err(|e| format!("Unable to get process memory info: {:?}", e))?;

        let vm = psutil::memory::virtual_memory()
            .map_err(|e| format!("Unable to get virtual memory: {:?}", e))?;

        let loadavg =
            psutil::host::loadavg().map_err(|e| format!("Unable to get loadavg: {:?}", e))?;

        Ok(Self {
            pid: process.pid(),
            pid_mem_resident_set_size: process_mem.rss(),
            pid_mem_virtual_memory_size: process_mem.vms(),
            sys_virt_mem_total: vm.total(),
            sys_virt_mem_available: vm.available(),
            sys_virt_mem_used: vm.used(),
            sys_virt_mem_free: vm.free(),
            sys_virt_mem_percent: vm.percent(),
            sys_loadavg_1: loadavg.one,
            sys_loadavg_5: loadavg.five,
            sys_loadavg_15: loadavg.fifteen,
        })
    }

    #[cfg(target_os = "macos")]
    pub fn observe() -> Result<Self, String> {
        let process =
            Process::current().map_err(|e| format!("Unable to get current process: {:?}", e))?;

        let process_mem = process
            .memory_info()
            .map_err(|e| format!("Unable to get process memory info: {:?}", e))?;

        let vm = psutil::memory::virtual_memory()
            .map_err(|e| format!("Unable to get virtual memory: {:?}", e))?;

        let sys = SystemStat::new();

        let loadavg = sys
            .load_average()
            .map_err(|e| format!("Unable to get loadavg: {:?}", e))?;

        Ok(Self {
            pid: process.pid() as u32,
            pid_mem_resident_set_size: process_mem.rss(),
            pid_mem_virtual_memory_size: process_mem.vms(),
            sys_virt_mem_total: vm.total(),
            sys_virt_mem_available: vm.available(),
            sys_virt_mem_used: vm.used(),
            sys_virt_mem_free: vm.free(),
            sys_virt_mem_percent: vm.percent(),
            sys_loadavg_1: loadavg.one as f64,
            sys_loadavg_5: loadavg.five as f64,
            sys_loadavg_15: loadavg.fifteen as f64,
        })
    }
}

/// Reports on the health of the Lighthouse instance.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeaconHealth {
    #[serde(flatten)]
    pub common: CommonHealth,
    /// Network statistics, totals across all network interfaces.
    pub network: Network,
    /// Filesystem information.
    pub chain_database: Option<MountInfo>,
    /// Filesystem information.
    pub freezer_database: Option<MountInfo>,
}

impl BeaconHealth {
    #[cfg(all(not(target_os = "linux"), not(target_os = "macos")))]
    pub fn observe() -> Result<Self, String> {
        Err("Health is only available on Linux and MacOS".into())
    }

    #[cfg(target_os = "linux")]
    pub fn observe(db_paths: &DBPaths) -> Result<Self, String> {
        Ok(Self {
            common: CommonHealth::observe()?,
            network: Network::observe()?,
            chain_database: MountInfo::for_path(&db_paths.chain_db)?,
            freezer_database: MountInfo::for_path(&db_paths.freezer_db)?,
        })
    }

    #[cfg(target_os = "macos")]
    pub fn observe(db_paths: &DBPaths) -> Result<Self, String> {
        Ok(Self {
            common: CommonHealth::observe()?,
            network: Network::observe()?,
            chain_database: MountInfo::for_path(&db_paths.chain_db)?,
            freezer_database: MountInfo::for_path(&db_paths.freezer_db)?,
        })
    }
}

use serde::Serialize;

/// Below this, modern Minecraft stutters or fails to start.
pub const MIN_MEMORY_MB: u32 = 2048;
/// Above this, a bigger client heap stops helping and GC pauses get longer.
pub const DIMINISHING_MEMORY_MB: u32 = 12288;

#[derive(Debug, Clone, Serialize)]
pub struct MemoryLimits {
    pub total_mb: u32,
    /// Warn below this.
    pub min_mb: u32,
    /// Warn above this: the OS and other apps need the rest.
    pub max_recommended_mb: u32,
    /// Note above this: more heap rarely helps a client.
    pub diminishing_mb: u32,
    pub slider_max_mb: u32,
}

pub fn total_memory_mb() -> u32 {
    use sysinfo::{MemoryRefreshKind, RefreshKind, System};
    let kind = RefreshKind::nothing().with_memory(MemoryRefreshKind::nothing().with_ram());
    match System::new_with_specifics(kind) {
        Ok(sys) => (sys.total_memory() / (1024 * 1024)) as u32,
        Err(_) => 0,
    }
}

pub fn memory_limits(total_mb: u32) -> MemoryLimits {
    // Unknown RAM: assume a modest 8 GB machine rather than disabling the checks.
    let total_mb = if total_mb == 0 { 8192 } else { total_mb };
    let half = round_down_512(total_mb / 2);
    MemoryLimits {
        total_mb,
        min_mb: MIN_MEMORY_MB,
        max_recommended_mb: half.max(MIN_MEMORY_MB),
        diminishing_mb: DIMINISHING_MEMORY_MB,
        slider_max_mb: round_down_512(total_mb).max(MIN_MEMORY_MB),
    }
}

/// A preset's preferred allocation, reduced on machines that can't spare it.
pub fn fit_memory(preferred_mb: u32, total_mb: u32) -> u32 {
    let limits = memory_limits(total_mb);
    preferred_mb
        .min(limits.max_recommended_mb)
        .max(MIN_MEMORY_MB)
}

fn round_down_512(mb: u32) -> u32 {
    mb / 512 * 512
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_for_32gb() {
        let l = memory_limits(32 * 1024 - 200);
        assert_eq!(l.max_recommended_mb, 15872);
        assert_eq!(l.min_mb, 2048);
    }

    #[test]
    fn presets_shrink_on_small_machines() {
        assert_eq!(fit_memory(6144, 8192), 4096);
        assert_eq!(fit_memory(4096, 32768), 4096);
        // Never below the minimum, even on a 4 GB machine.
        assert_eq!(fit_memory(4096, 3900), 2048);
    }
}

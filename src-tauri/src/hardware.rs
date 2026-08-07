use crate::GpuInfo;
use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use sysinfo::System;

/// A live utilization snapshot, polled by the sidebar meters.
///
/// VRAM fields are `None` when the platform can't report it (non-Windows, or
/// a machine whose GPU performance counters aren't available) so the UI can
/// hide the bar instead of showing an invented number.
#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageSample {
    pub cpu_pct: f32,
    pub ram_pct: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    pub vram_pct: Option<f32>,
    pub vram_used_mb: Option<u64>,
    pub vram_total_mb: Option<u64>,
}

/// Kept alive between polls: CPU usage is a delta between two refreshes, so a
/// throwaway `System` would always report 0.
static SYSTEM: OnceLock<Mutex<System>> = OnceLock::new();
/// Adapter capacity never changes at runtime, so enumerate DXGI only once.
static VRAM_TOTAL_MB: OnceLock<Option<u64>> = OnceLock::new();

/// Sample current CPU / RAM / VRAM utilization.
pub fn sample_usage() -> UsageSample {
    let (cpu_pct, ram_used, ram_total) = {
        let mut sys = SYSTEM
            .get_or_init(|| Mutex::new(System::new_all()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        (sys.global_cpu_usage(), sys.used_memory(), sys.total_memory())
    };

    let vram_total_mb = *VRAM_TOTAL_MB
        .get_or_init(|| detect_gpus().into_iter().filter_map(|g| g.vram_mb).max());
    let vram_used_mb = vram_total_mb.and_then(|_| sample_vram_used_mb());

    let vram_pct = match (vram_used_mb, vram_total_mb) {
        (Some(used), Some(total)) if total > 0 => {
            Some(((used as f32 / total as f32) * 100.0).clamp(0.0, 100.0))
        }
        _ => None,
    };

    UsageSample {
        cpu_pct: cpu_pct.clamp(0.0, 100.0),
        ram_pct: if ram_total > 0 {
            ((ram_used as f32 / ram_total as f32) * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        },
        ram_used_mb: ram_used / 1024 / 1024,
        ram_total_mb: ram_total / 1024 / 1024,
        vram_pct,
        vram_used_mb,
        vram_total_mb,
    }
}

/// Read dedicated VRAM in use across the whole machine.
///
/// DXGI's `QueryVideoMemoryInfo` only reports the *calling* process's usage,
/// which would read as ~0 here because the model runs in a child
/// `llama-server` (or Ollama) process. The `GPU Adapter Memory` PDH counter
/// set is per-adapter and covers every process, which is what the meter needs.
#[cfg(windows)]
fn sample_vram_used_mb() -> Option<u64> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Performance::{
        PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
        PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_LARGE, PDH_HCOUNTER, PDH_HQUERY,
        PDH_MORE_DATA,
    };

    const PDH_OK: u32 = 0;

    let path: Vec<u16> = "\\GPU Adapter Memory(*)\\Dedicated Usage"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    unsafe {
        let mut query = PDH_HQUERY::default();
        if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != PDH_OK {
            return None;
        }

        let result = (|| {
            let mut counter = PDH_HCOUNTER::default();
            if PdhAddEnglishCounterW(query, PCWSTR(path.as_ptr()), 0, &mut counter) != PDH_OK {
                return None;
            }
            // `Dedicated Usage` is a raw gauge, so one collection is enough —
            // unlike rate counters, it needs no second sample to difference.
            if PdhCollectQueryData(query) != PDH_OK {
                return None;
            }

            let mut buf_size = 0u32;
            let mut item_count = 0u32;
            if PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_LARGE,
                &mut buf_size,
                &mut item_count,
                None,
            ) != PDH_MORE_DATA
                || buf_size == 0
            {
                return None;
            }

            // Allocate as the item type rather than as bytes: PDH writes
            // 8-byte-aligned values into this buffer.
            let elem = std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>();
            let slots = (buf_size as usize).div_ceil(elem);
            let mut buffer = vec![PDH_FMT_COUNTERVALUE_ITEM_W::default(); slots];
            buf_size = (slots * elem) as u32;

            if PdhGetFormattedCounterArrayW(
                counter,
                PDH_FMT_LARGE,
                &mut buf_size,
                &mut item_count,
                Some(buffer.as_mut_ptr()),
            ) != PDH_OK
            {
                return None;
            }

            // One instance per physical adapter. Report the busiest, which
            // pairs with the largest-capacity adapter used for `vram_total_mb`.
            let busiest = buffer
                .iter()
                .take(item_count as usize)
                .map(|item| item.FmtValue.Anonymous.largeValue)
                .max()
                .unwrap_or(0);

            (busiest > 0).then_some(busiest as u64 / 1024 / 1024)
        })();

        PdhCloseQuery(query);
        result
    }
}

#[cfg(not(windows))]
fn sample_vram_used_mb() -> Option<u64> {
    None
}

#[cfg(windows)]
pub fn detect_gpus() -> Vec<GpuInfo> {
    use windows::Win32::Graphics::Dxgi::{
        CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE,
    };

    let mut gpus = Vec::new();

    unsafe {
        let factory: IDXGIFactory1 = match CreateDXGIFactory1() {
            Ok(f) => f,
            Err(e) => {
                log::warn!("DXGI factory create failed: {e}");
                return gpus;
            }
        };

        let mut i = 0u32;
        loop {
            let adapter: IDXGIAdapter1 = match factory.EnumAdapters1(i) {
                Ok(a) => a,
                Err(_) => break,
            };
            i += 1;

            let desc = match adapter.GetDesc1() {
                Ok(d) => d,
                Err(_) => continue,
            };
            // Skip the Microsoft Basic Render Driver / WARP.
            if (desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32) != 0 {
                continue;
            }

            let name_end = desc
                .Description
                .iter()
                .position(|c| *c == 0)
                .unwrap_or(desc.Description.len());
            let name = String::from_utf16_lossy(&desc.Description[..name_end])
                .trim()
                .to_string();

            let vram_mb = if desc.DedicatedVideoMemory > 0 {
                Some((desc.DedicatedVideoMemory as u64) / 1024 / 1024)
            } else if desc.SharedSystemMemory > 0 {
                Some((desc.SharedSystemMemory as u64) / 1024 / 1024)
            } else {
                None
            };

            let vendor = match desc.VendorId {
                0x10DE => "NVIDIA",
                0x1002 | 0x1022 => "AMD",
                0x8086 => "Intel",
                0x5143 => "Qualcomm",
                0x106B => "Apple",
                _ => "Unknown",
            }
            .to_string();

            gpus.push(GpuInfo {
                name,
                vendor,
                vram_mb,
                driver: None,
            });
        }
    }

    gpus
}

#[cfg(not(windows))]
pub fn detect_gpus() -> Vec<GpuInfo> {
    Vec::new()
}

use crate::GpuInfo;

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

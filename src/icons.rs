use std::cmp::Reverse;
use std::ffi::c_void;
use std::mem::size_of;
use std::ptr::null_mut;
use std::sync::OnceLock;

type Handle = *mut c_void;
type GetSystemMetricsForDpiFn = unsafe extern "system" fn(i32, u32) -> i32;

const SM_CXICON: i32 = 11;
const SM_CXSMICON: i32 = 49;
const LR_DEFAULTCOLOR: u32 = 0x0000_0000;
const ICON_VERSION_3: u32 = 0x0003_0000;

const APP_ICON_ICO: &[u8] = include_bytes!("../assets/AppIcon.ico");
const TRAY_RUN_ICO: &[u8] = include_bytes!("../assets/TrayIcon-Run.ico");
const TRAY_STOP_ICO: &[u8] = include_bytes!("../assets/TrayIcon-Stop.ico");

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetModuleHandleW(name: *const u16) -> Handle;
    fn GetProcAddress(module: Handle, proc_name: *const u8) -> *const c_void;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn CreateIconFromResourceEx(
        res_bits: *const u8,
        res_size: u32,
        is_icon: i32,
        version: u32,
        cx_desired: i32,
        cy_desired: i32,
        flags: u32,
    ) -> Handle;
    fn DestroyIcon(icon: Handle) -> i32;
    fn GetSystemMetrics(index: i32) -> i32;
}

pub(crate) struct IconHandle(Handle);

impl IconHandle {
    pub fn raw(&self) -> Handle {
        self.0
    }
}

impl Drop for IconHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { DestroyIcon(self.0) };
            self.0 = null_mut();
        }
    }
}

pub(crate) fn system_icon_size(dpi: u32, small: bool) -> i32 {
    static METRICS_FOR_DPI: OnceLock<Option<GetSystemMetricsForDpiFn>> = OnceLock::new();
    let f = *METRICS_FOR_DPI.get_or_init(|| unsafe {
        let user32: Vec<u16> = "user32.dll\0".encode_utf16().collect();
        let module = GetModuleHandleW(user32.as_ptr());
        if module.is_null() {
            return None;
        }
        let proc = GetProcAddress(module, c"GetSystemMetricsForDpi".as_ptr().cast());
        (!proc.is_null())
            .then(|| std::mem::transmute::<*const c_void, GetSystemMetricsForDpiFn>(proc))
    });
    let index = if small { SM_CXSMICON } else { SM_CXICON };
    if let Some(get_for_dpi) = f {
        let size = unsafe { get_for_dpi(index, dpi) };
        if size > 0 {
            return size;
        }
    }
    let base = unsafe { GetSystemMetrics(index) }.max(16);
    ((base as i64 * dpi as i64 + 48) / 96).max(16) as i32
}

pub(crate) fn create_app_icon(size: i32) -> Option<IconHandle> {
    load_ico_from_bytes(APP_ICON_ICO, size)
}

pub(crate) fn create_tray_icon(size: i32, paused: bool) -> Option<IconHandle> {
    let bytes = if paused { TRAY_STOP_ICO } else { TRAY_RUN_ICO };
    load_ico_from_bytes(bytes, size)
}

/// The stock icons of the shell (as `SystemIcons.GetStockIcon` of the .NET version): drawn in the
/// style of the running Windows (the flat ones of Windows 11), unlike `LoadIcon(IDI_*)`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum StockIcon {
    Shield = 77,
    Warning = 78,
    Info = 79,
    Error = 80,
}

#[repr(C)]
struct ShStockIconInfo {
    cb_size: u32,
    h_icon: Handle,
    i_sys_image_index: i32,
    i_icon: i32,
    sz_path: [u16; 260],
}

const SHGSI_ICONLOCATION: u32 = 0;

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHGetStockIconInfo(siid: i32, flags: u32, psii: *mut ShStockIconInfo) -> i32;
    fn SHDefExtractIconW(
        icon_file: *const u16,
        index: i32,
        flags: u32,
        large: *mut Handle,
        small: *mut Handle,
        icon_size: u32,
    ) -> i32;
}

/// A stock icon made at `size` pixels (not scaled from another size)
pub(crate) fn create_stock_icon(icon: StockIcon, size: i32) -> Option<IconHandle> {
    let size = size.clamp(8, 256);
    let mut info = ShStockIconInfo {
        cb_size: size_of::<ShStockIconInfo>() as u32,
        h_icon: null_mut(),
        i_sys_image_index: 0,
        i_icon: 0,
        sz_path: [0; 260],
    };
    if unsafe { SHGetStockIconInfo(icon as i32, SHGSI_ICONLOCATION, &mut info) } != 0 {
        return None;
    }
    let mut large: Handle = null_mut();
    let hr = unsafe {
        SHDefExtractIconW(
            info.sz_path.as_ptr(),
            info.i_icon,
            0,
            &mut large,
            null_mut(),
            size as u32 & 0xFFFF,
        )
    };
    (hr == 0 && !large.is_null()).then_some(IconHandle(large))
}

type StockIconCache = std::collections::HashMap<(StockIcon, i32), IconHandle>;

thread_local! {
    static STOCK_ICONS: std::cell::RefCell<StockIconCache> = std::cell::RefCell::new(StockIconCache::new());
}

/// A stock icon at `size` pixels, made once per size and kept for the thread (the UI thread draws
/// the same few icons: notes, shield buttons). Null if it could not be made.
pub(crate) fn cached_stock_icon(icon: StockIcon, size: i32) -> Handle {
    STOCK_ICONS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(h) = cache.get(&(icon, size)) {
            return h.raw();
        }
        match create_stock_icon(icon, size) {
            Some(h) => {
                let raw = h.raw();
                cache.insert((icon, size), h);
                raw
            }
            None => null_mut(),
        }
    })
}

#[derive(Clone, Copy)]
struct SelectedIcoFrame {
    offset: usize,
    len: usize,
    #[cfg_attr(not(test), allow(dead_code))]
    width: i32,
}

fn read_u16_le(data: &[u8], off: usize) -> Option<u16> {
    let bytes = data.get(off..off + 2)?;
    Some(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_u32_le(data: &[u8], off: usize) -> Option<u32> {
    let bytes = data.get(off..off + 4)?;
    Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn select_ico_frame(data: &[u8], target_size: i32) -> Option<SelectedIcoFrame> {
    if data.len() < 6 {
        return None;
    }
    let reserved = read_u16_le(data, 0)?;
    let kind = read_u16_le(data, 2)?;
    let count = read_u16_le(data, 4)? as usize;
    if reserved != 0 || kind != 1 || count == 0 {
        return None;
    }
    let dir_end = 6usize.checked_add(count.checked_mul(16)?)?;
    if data.len() < dir_end {
        return None;
    }

    let mut best: Option<((u8, i32, Reverse<u16>), SelectedIcoFrame)> = None;
    for idx in 0..count {
        let entry = 6 + idx * 16;
        let raw_w = data[entry];
        let raw_h = data[entry + 1];
        let width = if raw_w == 0 { 256 } else { raw_w as i32 };
        let height = if raw_h == 0 { 256 } else { raw_h as i32 };
        if width <= 0 || height <= 0 {
            continue;
        }
        let bpp = read_u16_le(data, entry + 6)?;
        let res_len = read_u32_le(data, entry + 8)? as usize;
        let res_off = read_u32_le(data, entry + 12)? as usize;
        let res_end = res_off.checked_add(res_len)?;
        if res_len == 0 || res_end > data.len() {
            continue;
        }

        let key = if width == target_size {
            (0u8, 0i32, Reverse(bpp))
        } else if width > target_size {
            (1u8, width - target_size, Reverse(bpp))
        } else {
            (2u8, target_size - width, Reverse(bpp))
        };

        let candidate = SelectedIcoFrame {
            offset: res_off,
            len: res_len,
            width,
        };
        if best.is_none_or(|(best_key, _)| key < best_key) {
            best = Some((key, candidate));
        }
    }

    best.map(|(_, frame)| frame)
}

fn load_ico_from_bytes(data: &[u8], size: i32) -> Option<IconHandle> {
    let desired = size.clamp(16, 256);
    let selected = select_ico_frame(data, desired)?;
    let frame = data.get(selected.offset..selected.offset + selected.len)?;
    let hicon = unsafe {
        CreateIconFromResourceEx(
            frame.as_ptr(),
            frame.len() as u32,
            1,
            ICON_VERSION_3,
            desired,
            desired,
            LR_DEFAULTCOLOR,
        )
    };
    (!hicon.is_null()).then_some(IconHandle(hicon))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_valid_icons_across_dpi_sizes() {
        for size in [16, 20, 24, 28, 32, 40, 48, 64] {
            let app = create_app_icon(size).expect("app icon");
            assert!(!app.raw().is_null());
            let run = create_tray_icon(size, false).expect("run icon");
            assert!(!run.raw().is_null());
            let stop = create_tray_icon(size, true).expect("stop icon");
            assert!(!stop.raw().is_null());
        }
        assert!(system_icon_size(96, true) >= 16);
        assert!(system_icon_size(144, false) >= 32);
    }

    #[test]
    fn makes_stock_icons_at_dpi_sizes() {
        for icon in [StockIcon::Shield, StockIcon::Warning, StockIcon::Info, StockIcon::Error] {
            for size in [16, 20, 28, 32, 56] {
                let made = create_stock_icon(icon, size).expect("stock icon");
                assert!(!made.raw().is_null());
            }
            // Made once per size
            let a = cached_stock_icon(icon, 24);
            assert!(!a.is_null());
            assert_eq!(a, cached_stock_icon(icon, 24));
        }
    }

    #[test]
    fn selects_exact_ico_frame_sizes() {
        for exact in [16, 20, 24, 32, 40, 48, 64, 128, 256] {
            let frame = select_ico_frame(APP_ICON_ICO, exact).expect("app frame");
            assert_eq!(frame.width, exact);
        }
        for exact in [16, 20, 24, 28, 32, 36, 40, 48] {
            let run = select_ico_frame(TRAY_RUN_ICO, exact).expect("run frame");
            assert_eq!(run.width, exact);
            let stop = select_ico_frame(TRAY_STOP_ICO, exact).expect("stop frame");
            assert_eq!(stop.width, exact);
        }
        // Size without an exact match picks the closest larger frame
        let fallback = select_ico_frame(TRAY_RUN_ICO, 30).expect("fallback frame");
        assert_eq!(fallback.width, 32);
    }
}

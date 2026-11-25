//! Rust-owned VM launch policy. QEMU remains the device/virtualization backend.
use std::ffi::{c_char, CStr};

#[repr(C)]
pub struct VmRequest {
    pub disk: *const c_char,
    pub firmware_dir: *const c_char,
    pub uefi_code: *const c_char,
    pub uefi_vars: *const c_char,
    pub memory_mb: u32,
    pub cpus: u32,
    pub adb_port: u32,
    pub flags: u32,
}

pub struct Config<'a> {
    pub disk: &'a str,
    pub firmware_dir: &'a str,
    pub uefi_code: &'a str,
    pub uefi_vars: &'a str,
    pub memory_mb: u32,
    pub cpus: u32,
    pub adb_port: u32,
    // 1 = multitouch, 2 = hide host cursor, 4 = loopback ADB, 8 = explicit software TCG, 16 = DirectSound output, 32 = experimental virtio-vga
    pub flags: u32,
}

pub fn plan(c: &Config<'_>) -> Result<Vec<String>, &'static str> {
    if !(512..=131072).contains(&c.memory_mb) || !(1..=64).contains(&c.cpus)
        || !(1024..=65535).contains(&c.adb_port) || c.flags & !255 != 0 || (c.flags & 64 != 0 && c.flags & 8 == 0) {
        return Err("Invalid VM resources, port or flags");
    }
    if c.disk.is_empty() || c.uefi_code.is_empty() != c.uefi_vars.is_empty()
        || (!c.uefi_code.is_empty() && (c.uefi_code.eq_ignore_ascii_case(c.uefi_vars)
            || c.disk.eq_ignore_ascii_case(c.uefi_vars) || c.disk.eq_ignore_ascii_case(c.uefi_code))) {
        return Err("Invalid disk or UEFI paths");
    }
    if [c.disk, c.firmware_dir, c.uefi_code, c.uefi_vars].iter().any(|p| p.chars().any(|ch| ch.is_control())) {
        return Err("Control characters in image paths");
    }
    let mut args: Vec<String> = vec!["-accel", if c.flags & 8 != 0 { "tcg,thread=multi" } else { "whpx,kernel-irqchip=off" }, "-m"].into_iter().map(str::to_owned).collect();
    args.extend([c.memory_mb.to_string(), "-smp".into(), c.cpus.to_string()]);
    args.extend(["-vga", if c.flags & 161 != 0 { "none" } else { "std" }, "-usb", "-device", "usb-tablet", "-hda", c.disk, "-boot", "order=c", "-name", "Project Nexus", "-qmp", "stdio"].into_iter().map(str::to_owned));
    if c.flags & 8 != 0 { args.extend(["-cpu".into(), if c.flags & 64 != 0 { "max,avx=off,avx2=off,f16c=off,fma=off" } else { "max" }.into()]); }
    if c.flags & 161 != 0 {
        args.extend(["-device", if c.flags & 128 != 0 { "virtio-vga-gl,id=nexus_display" } else if c.flags & 32 != 0 { "virtio-vga,id=nexus_display" } else { "VGA,id=nexus_display" }].into_iter().map(str::to_owned));
    }
    if c.flags & 1 != 0 {
        args.extend(["-device", "virtio-multitouch-pci,id=nexus_touch,display=nexus_display"].into_iter().map(str::to_owned));
    }
    if !c.uefi_code.is_empty() {
        args.extend(["-machine".into(), "q35,vmport=off".into(), "-drive".into(), format!("if=pflash,format=raw,unit=0,readonly=on,file={}", c.uefi_code.replace(',', ",,")), "-drive".into(), format!("if=pflash,format=raw,unit=1,file={}", c.uefi_vars.replace(',', ",,"))]);
    }
    if c.flags & 16 != 0 {
        args.extend(["-audiodev", "dsound,id=nexus_audio", "-device", "ich9-intel-hda,id=nexus_hda", "-device", "hda-output,bus=nexus_hda.0,audiodev=nexus_audio"].into_iter().map(str::to_owned));
    }
    if c.flags & 128 != 0 {
        args.extend(["-display".into(), if c.flags & 2 != 0 { "sdl,gl=on,show-cursor=off" } else { "sdl,gl=on" }.into()]);
    } else if c.flags & 2 != 0 { args.extend(["-display".into(), "sdl,show-cursor=off".into()]); }
    if !c.firmware_dir.is_empty() { args.extend(["-L".into(), c.firmware_dir.into()]); }
    let net = if c.flags & 4 != 0 { format!("user,id=net0,hostfwd=tcp:127.0.0.1:{}-:5555", c.adb_port) } else { "user,id=net0".into() };
    args.extend(["-netdev".into(), net, "-device".into(), "virtio-net-pci,netdev=net0,romfile=".into()]);
    Ok(args)
}

fn json(args: &[String]) -> String {
    let mut result = String::from("[");
    for (index, arg) in args.iter().enumerate() {
        if index != 0 { result.push(','); }
        result.push('"');
        for ch in arg.chars() {
            match ch { '"' => result.push_str("\\\""), '\\' => result.push_str("\\\\"), ch if ch.is_control() => { result.push_str(&format!("\\u{:04x}", ch as u32)); }, ch => result.push(ch) }
        }
        result.push('"');
    }
    result.push(']'); result
}

/// Returns JSON byte count (no terminator), or zero for invalid configuration.
/// With a null output or insufficient capacity no output bytes are written.
/// Safety: request must point to a valid VmRequest, each path to a live NUL-
/// terminated UTF-8 string; non-null output must permit capacity writable bytes
/// and must not overlap the request or its strings. Caller retains ownership.
#[no_mangle]
pub unsafe extern "C" fn nexus_vm_plan(request: *const VmRequest, output: *mut u8, capacity: usize) -> usize {
    if request.is_null() { return 0; }
    let r = &*request;
    unsafe fn text<'a>(p: *const c_char) -> Option<&'a str> { if p.is_null() { None } else { CStr::from_ptr(p).to_str().ok() } }
    let Some(disk) = text(r.disk) else { return 0; };
    let Some(firmware_dir) = text(r.firmware_dir) else { return 0; };
    let Some(uefi_code) = text(r.uefi_code) else { return 0; };
    let Some(uefi_vars) = text(r.uefi_vars) else { return 0; };
    let c = Config { disk, firmware_dir, uefi_code, uefi_vars, memory_mb: r.memory_mb, cpus: r.cpus, adb_port: r.adb_port, flags: r.flags };
    let Ok(args) = plan(&c) else { return 0; };
    let encoded = json(&args);
    if !output.is_null() && capacity >= encoded.len() { std::ptr::copy_nonoverlapping(encoded.as_ptr(), output, encoded.len()); }
    encoded.len()
}

/// 1: WHPX reports hypervisor present; 0: absent; -1: API unavailable/error.
/// This checks host capability, not whether a particular VM can start.
#[no_mangle]
pub extern "C" fn nexus_whpx_available() -> i32 {
    #[cfg(windows)]
    unsafe {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        extern "system" {
            fn LoadLibraryW(name: *const u16) -> *mut c_void;
            fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
            fn FreeLibrary(module: *mut c_void) -> i32;
        }
        let name: Vec<u16> = "WinHvPlatform.dll\0".encode_utf16().collect();
        let library = LoadLibraryW(name.as_ptr());
        if library.is_null() { return -1; }
        let symbol = GetProcAddress(library, b"WHvGetCapability\0".as_ptr().cast());
        if symbol.is_null() { FreeLibrary(library); return -1; }
        let capability: unsafe extern "system" fn(u32, *mut c_void, u32, *mut u32) -> i32 = std::mem::transmute(symbol);
        let mut present: i32 = 0; let mut written = 0;
        let status = capability(0, (&mut present as *mut i32).cast(), 4, &mut written);
        FreeLibrary(library);
        if status < 0 || written < 4 { -1 } else { i32::from(present != 0) }
    }
    #[cfg(not(windows))]
    { -1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn config() -> Config<'static> { Config { disk: "C:/Nexus/my disk.qcow2", firmware_dir: "", uefi_code: "", uefi_vars: "", memory_mb: 4096, cpus: 4, adb_port: 5555, flags: 0 } }
    #[test] fn acceleration_and_network() {
        let mut c = config(); c.flags = 5;
        let args = plan(&c).unwrap();
        assert!(args.iter().any(|a| a == "whpx,kernel-irqchip=off"));
        assert!(!args.iter().any(|a| a.contains("tcg")));
        assert!(args.iter().any(|a| a.contains("hostfwd=tcp:127.0.0.1:5555")));
        assert!(args.iter().any(|a| a.contains("virtio-multitouch")));
        assert!(args.iter().any(|a| a == c.disk));
    }
    #[test] fn explicit_software_mode() {
        let mut c = config(); c.flags = 8;
        let args = plan(&c).unwrap();
        assert!(args.iter().any(|a| a == "tcg,thread=multi"));
        assert!(args.windows(2).any(|a| a[0] == "-cpu" && a[1] == "max"));
        assert!(!args.iter().any(|a| a.contains("whpx")));
    }
    #[test] fn optional_audio_output_only() {
        let mut c = config();
        assert!(!plan(&c).unwrap().iter().any(|a| a == "-audiodev"));
        c.flags = 16 | 8;
        let args = plan(&c).unwrap();
        assert!(args.windows(2).any(|a| a[0] == "-audiodev" && a[1] == "dsound,id=nexus_audio"));
        assert!(args.iter().any(|a| a == "hda-output,bus=nexus_hda.0,audiodev=nexus_audio"));
        assert!(!args.iter().any(|a| a.contains("duplex") || a.contains("hda-micro")));
        c.flags = 31; assert!(plan(&c).is_ok());
    }
    #[test] fn virtio_display_uses_one_device_and_preserves_touch_target() {
        for flags in [32, 33, 63] {
            let mut c = config(); c.flags = flags;
            let args = plan(&c).unwrap();
            assert!(args.windows(2).any(|a| a[0] == "-vga" && a[1] == "none"));
            assert_eq!(args.iter().filter(|a| a.contains("id=nexus_display")).count(), 1);
            assert!(args.iter().any(|a| a == "virtio-vga,id=nexus_display"));
            assert!(!args.iter().any(|a| a.contains("-gl") || a.contains("gl=on")));
            if flags & 1 != 0 { assert!(args.iter().any(|a| a.contains("display=nexus_display"))); }
        }
    }
    #[test] fn gl_renderer_is_explicit_and_uses_one_display() {
        let mut c = config(); c.flags = 128 | 64 | 8 | 2 | 1;
        let args = plan(&c).unwrap();
        assert!(args.iter().any(|arg| arg == "virtio-vga-gl,id=nexus_display"));
        assert!(args.iter().any(|arg| arg == "sdl,gl=on,show-cursor=off"));
        assert_eq!(args.iter().filter(|arg| arg.as_str() == "-display").count(), 1);
        assert!(args.iter().any(|arg| arg == "max,avx=off,avx2=off,f16c=off,fma=off"));
        c.flags = 0;
        assert!(!plan(&c).unwrap().iter().any(|arg| arg.contains("gl=on")));
    }
    #[test] fn rejects_invalid_and_escapes_paths() {
        let mut c = config(); c.cpus = 0; assert!(plan(&c).is_err()); c.cpus = 4;
        c.flags = 256; assert!(plan(&c).is_err()); c.flags = 64; assert!(plan(&c).is_err()); c.flags = 0;
        c.uefi_code = "code.fd"; assert!(plan(&c).is_err());
        c.uefi_vars = "my,vars.fd";
        assert!(plan(&c).unwrap().iter().any(|a| a.contains("my,,vars.fd")));
        assert_eq!(json(&["C:\\a\"b".into()]), "[\"C:\\\\a\\\"b\"]");
    }
    #[test] fn ffi_capacity_and_invalid_requests() {
        let path = std::ffi::CString::new("disk.qcow2").unwrap();
        let empty = std::ffi::CString::new("").unwrap();
        let r = VmRequest { disk: path.as_ptr(), firmware_dir: empty.as_ptr(), uefi_code: empty.as_ptr(), uefi_vars: empty.as_ptr(), memory_mb: 4096, cpus: 4, adb_port: 5555, flags: 0 };
        unsafe {
            assert_eq!(nexus_vm_plan(std::ptr::null(), std::ptr::null_mut(), 0), 0);
            let size = nexus_vm_plan(&r, std::ptr::null_mut(), 0); assert!(size > 0);
            let mut bytes = vec![0xAA; size];
            assert_eq!(nexus_vm_plan(&r, bytes.as_mut_ptr(), size - 1), size);
            assert!(bytes.iter().all(|b| *b == 0xAA));
            assert_eq!(nexus_vm_plan(&r, bytes.as_mut_ptr(), size), size);
            assert_eq!(bytes[0], b'[');
        }
    }
}

/// Explicit user-requested working-set trim, not a VM RAM allocation change.
/// Returns 1 on success, 0 on Windows failure, -1 on unsupported platforms.
/// The host supplies only its own PID or that of the QEMU child it owns.
#[no_mangle]
pub extern "C" fn nexus_trim_working_set(pid: u32) -> i32 {
    if pid == 0 { return 0; }
    #[cfg(windows)]
    unsafe {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
            fn SetProcessWorkingSetSize(process: *mut c_void, minimum: usize, maximum: usize) -> i32;
            fn CloseHandle(handle: *mut c_void) -> i32;
        }
        const PROCESS_SET_QUOTA: u32 = 0x0100;
        let handle = OpenProcess(PROCESS_SET_QUOTA, 0, pid);
        if handle.is_null() { return 0; }
        let result = SetProcessWorkingSetSize(handle, usize::MAX, usize::MAX);
        CloseHandle(handle);
        i32::from(result != 0)
    }
    #[cfg(not(windows))]
    { -1 }
}

#[cfg(test)]
mod memory_tests {
    use super::*;
    #[test]
    fn rejects_zero_pid() { assert_eq!(nexus_trim_working_set(0), 0); }
    #[test]
    fn trims_only_when_explicitly_called() {
        #[cfg(windows)]
        assert_eq!(nexus_trim_working_set(std::process::id()), 1);
        #[cfg(not(windows))]
        assert_eq!(nexus_trim_working_set(std::process::id()), -1);
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Eq)]
pub struct ResourceBudget { pub cpus: u32, pub memory_mb: u32 }

/// Conservative initial allocation, not a benchmark-derived optimum.
#[no_mangle]
pub extern "C" fn nexus_resource_budget(host_threads: u32, host_memory_mb: u64) -> ResourceBudget {
    let cpus = (host_threads / 2).clamp(1, 8);
    // Leave at least half of known physical RAM to the host. No oversized VM
    // on low-memory PCs. Unknown memory uses a small conservative fallback.
    let memory_mb = if host_memory_mb == 0 { 2048 } else {
        (host_memory_mb / 4).clamp(512, 8192).min((host_memory_mb / 2).max(512)) as u32
    };
    ResourceBudget { cpus, memory_mb }
}

#[no_mangle]
pub extern "C" fn nexus_host_memory_mb() -> u64 {
    #[cfg(windows)]
    unsafe {
        #[repr(C)]
        struct MemoryStatus { length: u32, load: u32, total_phys: u64, avail_phys: u64, total_page: u64, avail_page: u64, total_virtual: u64, avail_virtual: u64, avail_extended: u64 }
        #[link(name = "kernel32")]
        extern "system" { fn GlobalMemoryStatusEx(status: *mut MemoryStatus) -> i32; }
        let mut status: MemoryStatus = std::mem::zeroed();
        status.length = std::mem::size_of::<MemoryStatus>() as u32;
        if GlobalMemoryStatusEx(&mut status) != 0 { status.total_phys / (1024 * 1024) } else { 0 }
    }
    #[cfg(not(windows))]
    { 0 }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    #[test] fn conservative_known_hosts() {
        assert_eq!(nexus_resource_budget(8, 16384), ResourceBudget { cpus: 4, memory_mb: 4096 });
        assert_eq!(nexus_resource_budget(4, 4096), ResourceBudget { cpus: 2, memory_mb: 1024 });
        assert_eq!(nexus_resource_budget(128, 262144), ResourceBudget { cpus: 8, memory_mb: 8192 });
    }
    #[test] fn bounded_even_for_unknown_hosts() {
        assert_eq!(nexus_resource_budget(0, 0), ResourceBudget { cpus: 1, memory_mb: 2048 });
        assert_eq!(nexus_resource_budget(1, 1024), ResourceBudget { cpus: 1, memory_mb: 512 });
    }
}

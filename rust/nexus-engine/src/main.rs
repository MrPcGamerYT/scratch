use nexus_engine::{image::{self,Image},whp};
use std::path::Path;
fn execute(args:&[String],start:std::time::Instant)->Result<(),String> {
    let boot_command=matches!(args.get(1).map(String::as_str),Some("boot-kernel"|"boot-kernel-disk-create"|"boot-kernel-disk-open"|"probe-android-init-create"|"probe-android-init-open"|"probe-android-observe-create"));
    let mut cpu_count=None;let mut apic_probe=false;let mut args=args;let mut ram=nexus_engine::kernel::DEFAULT_RAM;let mut surface=None;let mut live=false;let mut bridge_probe=false;let mut bridge_apk=None;let mut ram_set=false;
    if boot_command {
        while let Some(option)=args.last() {
            if let Some(path)=option.strip_prefix("--bridge-apk-smoke=") {
                if bridge_apk.is_some()||path.is_empty() {return Err("Invalid/duplicate bridge APK path".into());}
                bridge_apk=Some(std::path::PathBuf::from(path));args=&args[..args.len()-1];continue;
            }
            if let Some(value)=option.strip_prefix("--cpus=") {
                if cpu_count.is_some()||value.is_empty()||!value.bytes().all(|b|b.is_ascii_digit()) {return Err("Invalid/duplicate CPU count".into());}
                let count=value.parse::<u32>().map_err(|_|"CPU count overflow")?;
                if !(1..=8).contains(&count) {return Err("CPU count must be 1..8".into());}
                cpu_count=Some(count);args=&args[..args.len()-1];continue;
            }
            if option=="--apic-probe" {if apic_probe {return Err("Duplicate --apic-probe".into());}apic_probe=true;args=&args[..args.len()-1];continue;}
            if option=="--bridge-probe" {if bridge_probe {return Err("Duplicate --bridge-probe".into());}bridge_probe=true;args=&args[..args.len()-1];continue;}
            if option=="--window" {if live {return Err("Duplicate --window".into());}live=true;args=&args[..args.len()-1];continue;}
            if let Some(value)=option.strip_prefix("--ram-mib=") {
                if ram_set {return Err("Duplicate RAM option".into());}
                ram=nexus_engine::kernel::parse_ram_mib(value)?;ram_set=true;
            } else if let Some(value)=option.strip_prefix("--framebuffer=") {
                if surface.is_some()||value.is_empty() {return Err("Invalid/duplicate framebuffer capture path".into());}
                surface=Some(Path::new(value));
            } else {break;}
            args=&args[..args.len()-1];
        }
    }
    if cpu_count.unwrap_or(1)>1&&!apic_probe {return Err("Selected CPU count requires --apic-probe; legacy boot is one CPU".into());}
    match args.get(1).map(String::as_str) {
        Some("import-raw") if args.len()==4 => {
            image::import_raw(Path::new(&args[2]),Path::new(&args[3])).map_err(|e|e.to_string())?;
            println!("Imported raw disk into ZLAG v1. This does not certify Android boot."); Ok(())
        }
        Some("verify") if args.len()==3 => {
            let mut image=Image::open(Path::new(&args[2])).map_err(|e|e.to_string())?;
            image.verify().map_err(|e|e.to_string())?;
            println!("ZLAG v1 OK: {} disk bytes, CRC32 {:08x}. Integrity check only; not a boot test.",image.disk_bytes,image.payload_crc32); Ok(())
        }
        Some("whp-probe") if args.len()==2 => { whp::probe()?; println!("Direct WHP partition create/setup/delete succeeded. No guest CPU executed."); Ok(()) }
        Some("protection-smoke") if args.len()==2 => {whp::protection_smoke()?;println!("Guest unmapped/read-only page faults, CR0.WP, CR2/error-code capture and IRETQ recovery passed.");Ok(())}
        Some("transition-smoke") if args.len()==2 => {whp::transition_smoke()?;println!("Guest-driven real16 -> protected32 -> long64, CR0/CR3/CR4/EFER and nonidentity paged RAM store/load passed; not full platform certification.");Ok(())}
        Some("smp-smoke") if args.len()==2 => {whp::smp_smoke()?;println!("Two real WHP vCPUs: guest xAPIC INIT/SIPI starts AP from wait-for-SIPI and shared RAM handshake passes; not Linux SMP certification.");Ok(())}
        Some("cpu-smoke") if args.len()==2 => { whp::cpu_smoke()?; println!("Direct WHP guest CPU store/load/HLT test passed: AX=0x5a17. Not an Android boot or performance test."); Ok(()) }
        Some("io-smoke") if args.len()==2 => { whp::io_smoke()?; println!("Direct WHP scalar port I/O, register widths and fail-closed exit tests passed. Private test devices only."); Ok(()) }
        Some("cancel-smoke") if args.len()==2 => { whp::cancel_smoke()?; println!("Direct WHP infinite-loop cancellation and same-vCPU restart passed."); Ok(()) }
        Some("ata-cow-smoke") if args.len()==4 => {whp::ata_cow_smoke(Path::new(&args[2]),Path::new(&args[3]))?; println!("Guest ATA write/flush/read and persistent overlay reopen passed; base untouched. Polled diagnostic only, not Android boot.");Ok(())}
        Some("ata-pio-smoke") if args.len()==3 => {whp::ata_pio_smoke(Path::new(&args[2]))?; println!("Verified ZLAG -> ATA IDENTIFY/PIO sector read -> guest RAM and write refusal passed. Read-only, polled subset; not Android storage certification.");Ok(())}
        Some("mmio-smoke") if args.len()==2 => {whp::mmio_smoke()?;println!("Real WHP protected/long-mode MMIO queue write/flush/read/COW persistence and protected-mode external IRQ5 passed; not Android certification.");Ok(())}
        Some("timer-irq-smoke") if args.len()==2 => {whp::timer_irq_smoke()?; println!("PIT0 one-shot -> PIC IRQ0 -> external WHP interrupt -> HLT wake -> EOI/IRETD passed; not Android boot.");Ok(())}
        Some("protected-smoke") if args.len()==2 => { whp::protected_smoke()?; println!("Direct WHP seeded protected-mode 32-bit execution, flat memory and scalar I/O passed. Not a kernel boot."); Ok(()) }
        Some("long-smoke") if args.len()==2 => { whp::long_smoke()?; println!("Direct WHP seeded 64-bit execution, nonidentity paging alias, guest INT/IRETQ and missing-PTE refusal passed. Not a kernel boot."); Ok(()) }
        Some("kernel-contract-smoke") if args.len()==2 => { whp::kernel_contract_smoke()?; println!("Synthetic kernel loader handshake passed: ESI, boot_params, E820, command line and initramfs. NOT a real Linux boot."); Ok(()) }
        Some("inspect-kernel") if args.len()==3 => {
            let image=nexus_engine::kernel::read_input(Path::new(&args[2]))?;
            let p=nexus_engine::kernel::KernelPlan::build(&image,None,"",nexus_engine::kernel::DEFAULT_RAM)?;
            println!("Linux boot plan: protocol=0x{:x}, entry=0x{:x}, runtime_end=0x{:x}, regions={}. Header/layout validation only.",p.protocol,p.entry,p.runtime_end,p.regions.len());Ok(())
        }
        Some(command @ ("boot-kernel"|"boot-kernel-disk-create"|"boot-kernel-disk-open"|"probe-android-init-create"|"probe-android-init-open"|"probe-android-observe-create")) if (command=="boot-kernel"&&(args.len()==5||args.len()==6))||(command!="boot-kernel"&&args.len()==8) => {
            if bridge_apk.is_some()&&command!="probe-android-observe-create" {return Err("APK smoke requires probe-android-observe-create with a new disposable COW overlay".into());}
            use nexus_engine::kernel;
            let image=kernel::read_input(Path::new(&args[2]))?;
            let initrd=if args.len()>=6 {Some(kernel::read_input(Path::new(&args[5]))?)} else {None};
            let mut plan=kernel::KernelPlan::build(&image,initrd.as_deref(),&args[4],ram)?;
            if apic_probe&&(command.starts_with("probe-android-")||live||surface.is_some()||bridge_apk.is_some()) {return Err("--apic-probe supports headless Linux only, not Android/APK/player".into());}
            plan.apic_probe=apic_probe;plan.cpu_count=cpu_count.unwrap_or(1);
            plan.validate_regions()?;
            plan.bridge_probe=bridge_probe||bridge_apk.is_some();plan.bridge_apk=bridge_apk;
            plan.android_init_probe=command.starts_with("probe-android-");
            plan.android_observe=command=="probe-android-observe-create";
            if live&&surface.is_none() {return Err("--window currently requires an exclusive --framebuffer=NEW-file.ppm capture path".into());}
            let window=if live {Some(nexus_engine::player::Window::open()?)} else {None};
            plan.player=window.as_ref().map(|w|w.session.clone());
            if let Some(path)=surface {plan=plan.with_framebuffer(path)?;}
            use std::io::Write;
            let mut trace=nexus_engine::timing::BootTrace::after_header(start.elapsed().as_millis() as u64);
            let log=std::fs::OpenOptions::new().create_new(true).write(true).open(&args[3]).map_err(|e|e.to_string())?;
            let mut log=std::io::BufWriter::with_capacity(65536,log);
            let mut timings=std::fs::OpenOptions::new().create_new(true).write(true).open(format!("{}.timing.json",args[3])).map_err(|e|e.to_string())?;
            eprintln!("Starting experimental Linux entry at 0x{:x}; polled serial log: {}. Full kernel/platform boot is NOT implemented.",plan.entry,args[3]);
            let result=if command=="boot-kernel" {whp::boot_kernel(&plan,&mut log,&mut trace,start)} else {
                whp::boot_kernel_disk(&plan,&mut log,&mut trace,start,Path::new(&args[6]),Path::new(&args[7]),command.ends_with("create"))
            };
            log.flush().map_err(|e|e.to_string())?;
            if let Some(window)=&window {
                use std::sync::atomic::Ordering;
                println!("NATIVE_PLAYER_JSON={{\"software_blits\":{},\"last_submitted_bgrx_crc32\":{},\"guest_input_verified\":false,\"gpu_accelerated\":false}}",window.session.presents.load(Ordering::Acquire),window.session.presented_crc32.load(Ordering::Acquire));
            }
            trace.stopped_ms=Some(start.elapsed().as_millis() as u64);
            trace.stop_reason=result.as_ref().err().cloned().unwrap_or_else(||if plan.bridge_apk.is_some() {"Native APK install/check/uninstall reported successful by Android bridge; Home/network/audio not verified".into()} else if plan.bridge_probe {"Native bridge query round trip completed; not APK/network/audio/Home verification".into()} else if plan.android_init_probe {"Android second-stage init checkpoint reached; framework/Home not verified".into()} else {"Linux diagnostic userspace checkpoint reached; Android boot not implemented".into()});
            timings.write_all(trace.json().as_bytes()).map_err(|e|e.to_string())?;
            timings.flush().map_err(|e|e.to_string())?;
            result
        }
        Some("boot-test-sector") if args.len()==3 => {
            let console=whp::boot_test_sector(Path::new(&args[2]))?;
            println!("ZLAG test sector executed at 0000:7c00 and halted. Debug: {}",console.escape_ascii());
            println!("No BIOS, UEFI or disk-device services. This is NOT an Android boot."); Ok(())
        }
        Some(command @ ("graphics-smoke-hardware"|"graphics-smoke-warp")) if args.len()==3 => {
            use std::io::Write;
            use nexus_engine::graphics::{self,Backend};
            let mut output=std::fs::OpenOptions::new().write(true).create_new(true).open(&args[2]).map_err(|e|e.to_string())?;
            let backend=if command=="graphics-smoke-hardware" {Backend::Hardware} else {Backend::Warp};
            let rendered=graphics::render(backend)?;let level=rendered.feature_level;
            output.write_all(&rendered.pixels).map_err(|e|e.to_string())?;output.flush().map_err(|e|e.to_string())?;
            println!("GRAPHICS_HOST_RENDER={{\"backend\":\"{backend:?}\",\"feature_level\":{level},\"adapter\":{:?},\"vendor_id\":{},\"device_id\":{},\"software_adapter\":{},\"adapter_flags\":{},\"shader_pixels_verified\":true,\"guest_acceleration_verified\":false,\"android_graphics_verified\":false}}",rendered.adapter,rendered.vendor_id,rendered.device_id,rendered.software_adapter,rendered.adapter_flags);Ok(())
        }
        Some("player-window-smoke") if args.len()==2 => {
            use std::sync::atomic::Ordering;
            let window=nexus_engine::player::Window::open()?;
            let mut frame=vec![0;nexus_engine::framebuffer::MAPPED_BYTES];
            for pixel in frame[..nexus_engine::framebuffer::PIXEL_BYTES].chunks_exact_mut(4) {pixel.copy_from_slice(&[0x70,0x35,0x15,0]);}
            window.session.publish(&frame)?;
            std::thread::sleep(std::time::Duration::from_millis(500));
            let presents=window.session.presents.load(Ordering::Relaxed);
            window.request_close()?;
            let until=std::time::Instant::now()+std::time::Duration::from_secs(3);
            while !window.session.stopped()&&std::time::Instant::now()<until {std::thread::sleep(std::time::Duration::from_millis(5));}
            if !window.session.stopped() {return Err("WM_CLOSE did not stop native player".into());}
            drop(window);
            if presents==0 {return Err("Native window did not present any software frames".into());}
            println!("Native window software presentation/close PASS: {presents} blits; not guest FPS, Android display or GPU acceleration");Ok(())
        }
        Some("capabilities") if args.len()==2 => {
            println!("Independent Rust foundation; QEMU dependency: none\nZLAG raw-disk import / validation / sector reads: implemented\nVerified ZLAG-backed ATA LBA28 PIO with optional persistent COW overlay: implemented and diagnostic-tested; no IRQ14, DMA or Android integration\nDirect Windows WHP partition lifecycle: implemented, host-dependent\nBounded real-mode WHP CPU/RAM smoke: implemented, host-dependent\nPrivate scalar port-I/O test bus / bounded cancellation: implemented, host-dependent\nZLAG first-sector test loader at 0000:7c00: implemented, no BIOS services\nSeeded protected/long-mode test startup with paging and guest software interrupt: implemented, host-dependent\nLinux bzImage loader, polled COM1, PIT0/IRQ0 and diagnostic PID1 checkpoint: implemented; no general Linux distribution or usable Android Home\nOpt-in VirtIO-MMIO block with dedicated IRQ5 and Linux vda/COW separate-boot raw-block persistence: diagnostic-tested including ext4 clean remount/journal replay; original Android second-stage init verified separately; not framework/Home\nOpt-in 800x600 software scanout -> Linux fbdev writes -> exact PPM capture: diagnostic-tested with native Win32 software presentation; no Android touch/key transport or guest acceleration\nOpt-in private COM2 bridge query probe with IRQ3: Linux diagnostic round-trip tested; Android daemon/APK installation/network/audio NOT IMPLEMENTED on new engine\nHost D3D11 shader/readback validation: implemented with explicit hardware request or WARP, no guest GPU transport\nFirmware / full platform devices / usable Android Home: NOT IMPLEMENTED\nGuest DirectX / OpenGL / Vulkan acceleration: NOT IMPLEMENTED\nNo performance or game-compatibility certification"); Ok(())
        }
        Some("boot") => Err("Android boot is not implemented. Refusing to pretend this is a working emulator or launch QEMU.".into()),
        _ => Err("Usage: nexus-engine import-raw <stopped-raw.img> <new-gustimg.zlag> | verify <gustimg.zlag> | whp-probe | cpu-smoke | io-smoke | cancel-smoke | protected-smoke | long-smoke | timer-irq-smoke | mmio-smoke | ata-pio-smoke <test.zlag> | ata-cow-smoke <test.zlag> <NEW-overlay.cow> | boot-test-sector <test.zlag> | kernel-contract-smoke | inspect-kernel <bzImage> | boot-kernel <bzImage> <NEW-serial.log> <cmdline> [initramfs] | boot-kernel-disk-create <bzImage> <NEW-serial.log> <cmdline> <initramfs> <base.zlag> <NEW-overlay.cow> | boot-kernel-disk-open <bzImage> <NEW-serial.log> <cmdline> <initramfs> <base.zlag> <existing-overlay.cow> | probe-android-init-create <bzImage> <NEW-serial.log> <cmdline> <original-initrd> <base.zlag> <NEW-overlay.cow> (second-stage init probe only, not Android Home) | probe-android-init-open <bzImage> <NEW-serial.log> <cmdline> <original-initrd> <base.zlag> <existing-overlay.cow> | graphics-smoke-hardware <NEW-64x64.rgba> | graphics-smoke-warp <NEW-64x64.rgba> | capabilities (boot-kernel accepts experimental --apic-probe with --cpus=1..8 (headless Linux initramfs/disk, no Android/player); kernel boot commands accept trailing --bridge-probe (query-only integration checkpoint), --window (software console player), --ram-mib=128..3072 and --framebuffer=NEW-file.ppm; software scanout, not GPU acceleration)".into())
    }
}
fn main() { let start=std::time::Instant::now(); if let Err(error)=execute(&std::env::args().collect::<Vec<_>>(),start) { eprintln!("Nexus native engine: {error}"); std::process::exit(1); } }

use nexus_engine::{image::{self,Image},whp};
use std::path::Path;
fn execute(args:&[String],start:std::time::Instant)->Result<(),String> {
    let boot_command=matches!(args.get(1).map(String::as_str),Some("boot-kernel"|"boot-kernel-disk-create"|"boot-kernel-disk-open"|"probe-android-init-create"|"probe-android-init-open"|"probe-android-observe-create"|"probe-android-home-create"));
    let mut dma_failure=None;let mut cpu_count=None;let mut apic_probe=false;let mut args=args;let mut ram=nexus_engine::kernel::DEFAULT_RAM;let mut surface=None;let mut live=false;let mut bridge_probe=false;let mut bridge_apk=None;let mut bridge_home_check=false;let mut ram_set=false;let mut renderer=nexus_engine::kernel::Renderer::Software;let mut audio_probe=false;let mut audio_backend=nexus_engine::kernel::AudioBackendKind::Null;let mut gpu_probe=false;
    if boot_command {
        while let Some(option)=args.last() {
            if let Some(path)=option.strip_prefix("--bridge-apk-smoke=") {
                if bridge_apk.is_some()||path.is_empty() {return Err("Invalid/duplicate bridge APK path".into());}
                bridge_apk=Some(std::path::PathBuf::from(path));args=&args[..args.len()-1];continue;
            }
            if let Some(value)=option.strip_prefix("--test-dma-failure=") {
                if dma_failure.is_some() {return Err("Duplicate DMA failure option".into());}
                dma_failure=Some(match value {"error"=>nexus_engine::kernel::DmaFailure::Error,"panic"=>nexus_engine::kernel::DmaFailure::Panic,"error-after"=>nexus_engine::kernel::DmaFailure::ErrorAfter,"panic-after"=>nexus_engine::kernel::DmaFailure::PanicAfter,_=>return Err("DMA failure must be error or panic (optionally -after)".into())});
                args=&args[..args.len()-1];continue;
            }
            if let Some(value)=option.strip_prefix("--cpus=") {
                if cpu_count.is_some()||value.is_empty()||!value.bytes().all(|b|b.is_ascii_digit()) {return Err("Invalid/duplicate CPU count".into());}
                let count=value.parse::<u32>().map_err(|_|"CPU count overflow")?;
                if !(1..=8).contains(&count) {return Err("CPU count must be 1..8".into());}
                cpu_count=Some(count);args=&args[..args.len()-1];continue;
            }
            if option=="--apic-probe" {if apic_probe {return Err("Duplicate --apic-probe".into());}apic_probe=true;args=&args[..args.len()-1];continue;}
            if option=="--bridge-probe" {if bridge_probe {return Err("Duplicate --bridge-probe".into());}bridge_probe=true;args=&args[..args.len()-1];continue;}
            if option=="--bridge-home-check" {if bridge_home_check {return Err("Duplicate --bridge-home-check".into());}bridge_home_check=true;args=&args[..args.len()-1];continue;}
            if option=="--window" {if live {return Err("Duplicate --window".into());}live=true;args=&args[..args.len()-1];continue;}
            if let Some(value)=option.strip_prefix("--ram-mib=") {
                if ram_set {return Err("Duplicate RAM option".into());}
                ram=nexus_engine::kernel::parse_ram_mib(value)?;ram_set=true;
            } else if let Some(value)=option.strip_prefix("--renderer=") {
                let parsed=nexus_engine::kernel::Renderer::parse(value)?;
                renderer=parsed;
            } else if option=="--audio-probe" {if audio_probe {return Err("Duplicate --audio-probe".into());}audio_probe=true;
            } else if option=="--gpu-probe" {if gpu_probe {return Err("Duplicate --gpu-probe".into());}gpu_probe=true;
            } else if let Some(value)=option.strip_prefix("--audio-backend=") {
                let parsed=nexus_engine::kernel::AudioBackendKind::parse(value)?;
                audio_backend=parsed;
            } else if let Some(value)=option.strip_prefix("--framebuffer=") {
                if surface.is_some()||value.is_empty() {return Err("Invalid/duplicate framebuffer capture path".into());}
                surface=Some(Path::new(value));
            } else {break;}
            args=&args[..args.len()-1];
        }
    }
    if dma_failure.is_some()&&(!apic_probe||args.get(1).map(String::as_str)!=Some("boot-kernel-disk-create")||cpu_count.unwrap_or(1)<2||live||surface.is_some()||bridge_probe||bridge_apk.is_some()) {return Err("DMA failure test requires APIC disk-create, 2..8 CPUs and a NEW disposable overlay; no Android/bridge/player".into());}
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
        Some("net-smoke") if args.len()==2 => {
            nexus_engine::virtio_net_mmio::net_smoke()?;
            println!("VirtIO-MMIO net device model passed: feature negotiation, TX frame transmit, loopback RX delivery and queue completion; loopback backend only, no host network stack or guest boot.");
            Ok(())
        }
        Some("net-mmio-smoke") if args.len()==2 => {
            whp::net_mmio_smoke()?;
            println!("Real WHP MMIO net device passed: enum dispatch, GPA routing, IRQ6, TX frame transmit, loopback RX delivery and queue completion through the live emulator; loopback backend only, not Android boot.");
            Ok(())
        }
        Some("audio-smoke") if args.len()==2 => {
            nexus_engine::virtio_audio_mmio::audio_smoke()?;
            println!("VirtIO-MMIO audio device model passed: feature negotiation, playback TX, capture RX and queue completion; null backend only, no host audio stack or guest boot.");
            Ok(())
        }
        Some("audio-real-smoke") if args.len()==2 => {
            nexus_engine::virtio_audio::audio_real_smoke()?;
            println!("Real audio backend passed: playback recorded to a playable WAV, capture synthesized a non-silent tone; real sample flow, no host audio device required.");
            Ok(())
        }
        Some("audio-mmio-smoke") if args.len()==2 => {
            whp::audio_mmio_smoke()?;
            println!("Real WHP MMIO audio device passed: enum dispatch, GPA routing, IRQ7, playback TX and capture RX delivery through the live emulator; null backend only, not Android boot.");
            Ok(())
        }
        Some("platform-mmio-smoke") if args.len()==2 => {
            whp::platform_mmio_smoke()?;
            println!("Real WHP MMIO platform devices passed: GPIO direction/output/status, I2C start/address/data, SPI enable/clock/TX-RX loopback through the live emulator.");
            Ok(())
        }
        Some("gpu-mmio-smoke") if args.len()==2 => {
            whp::gpu_mmio_smoke()?;
            println!("Real WHP MMIO GPU device passed: enum dispatch, GPA routing, VirtIO-gpu feature negotiation, scanout geometry, FILL_2D rasterization into the scanout surface, command completion and interrupt through the live emulator; not Android GPU certification.");
            Ok(())
        }
        Some("timer-irq-smoke") if args.len()==2 => {whp::timer_irq_smoke()?; println!("PIT0 one-shot -> PIC IRQ0 -> external WHP interrupt -> HLT wake -> EOI/IRETD passed; not Android boot.");Ok(())}
        Some("protected-smoke") if args.len()==2 => { whp::protected_smoke()?; println!("Direct WHP seeded protected-mode 32-bit execution, flat memory and scalar I/O passed. Not a kernel boot."); Ok(()) }
        Some("long-smoke") if args.len()==2 => { whp::long_smoke()?; println!("Direct WHP seeded 64-bit execution, nonidentity paging alias, guest INT/IRETQ and missing-PTE refusal passed. Not a kernel boot."); Ok(()) }
        Some("kernel-contract-smoke") if args.len()==2 => { whp::kernel_contract_smoke()?; println!("Synthetic kernel loader handshake passed: ESI, boot_params, E820, command line and initramfs. NOT a real Linux boot."); Ok(()) }
        Some("inspect-kernel") if args.len()==3 => {
            let image=nexus_engine::kernel::read_input(Path::new(&args[2]))?;
            let p=nexus_engine::kernel::KernelPlan::build(&image,None,"",nexus_engine::kernel::DEFAULT_RAM)?;
            println!("Linux boot plan: protocol=0x{:x}, entry=0x{:x}, runtime_end=0x{:x}, regions={}. Header/layout validation only.",p.protocol,p.entry,p.runtime_end,p.regions.len());Ok(())
        }
        Some(command @ ("boot-kernel"|"boot-kernel-disk-create"|"boot-kernel-disk-open"|"probe-android-init-create"|"probe-android-init-open"|"probe-android-observe-create"|"probe-android-home-create")) if (command=="boot-kernel"&&(args.len()==5||args.len()==6))||(command!="boot-kernel"&&args.len()==8) => {
            if bridge_apk.is_some()&&command!="probe-android-observe-create" {return Err("APK smoke requires probe-android-observe-create with a new disposable COW overlay".into());}
            if bridge_home_check&&(command!="probe-android-home-create"||bridge_apk.is_some()) {return Err("Clean-launch Home check requires probe-android-home-create with no APK smoke".into());}
            use nexus_engine::kernel;
            let image=kernel::read_input(Path::new(&args[2]))?;
            let initrd=if args.len()>=6 {Some(kernel::read_input(Path::new(&args[5]))?)} else {None};
            let mut plan=kernel::KernelPlan::build(&image,initrd.as_deref(),&args[4],ram)?;
            if apic_probe&&(command.starts_with("probe-android-")||live||surface.is_some()||bridge_apk.is_some()) {return Err("--apic-probe supports headless Linux only, not Android/APK/player".into());}
            plan.dma_failure=dma_failure;plan.apic_probe=apic_probe;plan.cpu_count=cpu_count.unwrap_or(1);
            plan.renderer=renderer;
            plan.audio_probe=audio_probe;plan.audio_backend=audio_backend;plan.gpu_probe=gpu_probe;
            plan.validate_regions()?;
            plan.bridge_probe=bridge_probe||bridge_apk.is_some();plan.bridge_apk=bridge_apk;plan.bridge_home_check=bridge_home_check;
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
                println!("NATIVE_PLAYER_JSON={{\"software_blits\":{},\"last_submitted_bgrx_crc32\":{},\"guest_input_verified\":false,\"gpu_accelerated\":false,\"titlebar_buttons\":{},\"fullscreen\":{},\"volume\":{},\"active_app\":{},\"keymap_selector\":{}}}",window.session.presents.load(Ordering::Acquire),window.session.presented_crc32.load(Ordering::Acquire),nexus_engine::player::BUTTONS.len(),window.session.fullscreen.load(Ordering::Acquire),window.session.volume.load(Ordering::Acquire),window.session.app().unwrap_or_else(||"none".into()),window.session.keymap_selector.load(Ordering::Acquire));
            }
            trace.stopped_ms=Some(start.elapsed().as_millis() as u64);
            trace.stop_reason=result.as_ref().err().cloned().unwrap_or_else(||if plan.bridge_apk.is_some() {"Native APK install/check/uninstall reported successful by Android bridge; Home/network/audio not verified".into()} else if plan.bridge_home_check {"Clean-launch Android Home verified over native COM2 bridge; not networking/audio/GPU".into()} else if plan.bridge_probe {"Native bridge query round trip completed; not APK/network/audio/Home verification".into()} else if plan.android_init_probe {"Android second-stage init checkpoint reached; framework/Home not verified".into()} else {"Linux diagnostic userspace checkpoint reached; Android boot not implemented".into()});
            timings.write_all(trace.json().as_bytes()).map_err(|e|e.to_string())?;
            timings.flush().map_err(|e|e.to_string())?;
            result
        }
        Some("boot-test-sector") if args.len()==3 => {
            let console=whp::boot_test_sector(Path::new(&args[2]))?;
            println!("ZLAG test sector executed at 0000:7c00 and halted. Debug: {}",console.escape_ascii());
            println!("No BIOS, UEFI or disk-device services. This is NOT an Android boot."); Ok(())
        }
        Some("firmware-boot-smoke") if args.len()==2 => {
            whp::firmware_boot_smoke()?;
            println!("Real firmware boot chain passed: reset-vector firmware POST, ATA disk read of the boot sector, and transfer to 0000:7c00 which executed.");
            Ok(())
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
        Some("graphics-present-smoke") if args.len()==2 => {
            use nexus_engine::graphics::{self,Backend};
            use nexus_engine::virtio_gpu::{Surface,FORMAT_XRGB8888};
            // Build a known 8x8 XRGB8888 scanout surface (pitch 32) with an
            // asymmetric pattern so a byte-preserving round-trip is meaningful.
            let width=8u32;let height=8u32;let pitch=32u32;
            let mut data=vec![0u8;(pitch*height) as usize];
            for y in 0..height {for x in 0..width {
                let at=(y*pitch+x*4) as usize;
                data[at..at+4].copy_from_slice(&[(x*37) as u8,(y*53) as u8,((x+y)*29) as u8,0]);
            }}
            let surface=Surface{width,height,format:FORMAT_XRGB8888,pitch,data};
            let rendered=graphics::present(Backend::Warp,&surface)?;
            if rendered.pixels!=surface.data {return Err("D3D11 present readback pixels do not match the scanout surface".into());}
            let level=rendered.feature_level;
            println!("GRAPHICS_HOST_PRESENT={{\"backend\":\"Warp\",\"feature_level\":{level},\"adapter\":{:?},\"surface\":\"{}x{} XRGB8888\",\"pixels_verified\":true,\"transport\":\"staging->rendertarget->readback\",\"guest_acceleration_verified\":false,\"android_graphics_verified\":false}}",rendered.adapter,width,height);
            Ok(())
        }
        Some("gpu-present-smoke") if args.len()==2 => {
            use nexus_engine::graphics::{self,Backend};
            use nexus_engine::virtio_gpu::{self,GpuBackend,ScanoutBackend,Surface,FORMAT_XRGB8888};
            use nexus_engine::virtqueue::SplitQueue;
            use nexus_engine::guest_memory::{GuestMemory,RamRegion};
            // Guest RAM layout: descriptor table (0x1000), avail ring (0x2000),
            // used ring (0x3000), FILL_2D header (0x4000), status byte (0x5000).
            let mut ram=vec![0u8;0x10000];
            let desc_base=0x1000u64;let avail=0x2000u64;let used=0x3000u64;
            let header_addr=0x4000u64;let status_addr=0x5000u64;
            // Descriptor 0: header (RO, 24 bytes, NEXT->1). Descriptor 1: status (WR, 1 byte).
            let descs=[(desc_base,header_addr,24u32,1u16,1u16),(desc_base+16,status_addr,1,2,0)];
            for (base,address,length,flags,next) in descs {
                let b=base as usize;
                ram[b..b+8].copy_from_slice(&address.to_le_bytes());
                ram[b+8..b+12].copy_from_slice(&length.to_le_bytes());
                ram[b+12..b+14].copy_from_slice(&flags.to_le_bytes());
                ram[b+14..b+16].copy_from_slice(&next.to_le_bytes());
            }
            // FILL_2D header: opcode=4, color=0x00ff0000 (red), x=0, y=0, w=4, h=4.
            ram[0x4000..0x4004].copy_from_slice(&4u32.to_le_bytes());
            ram[0x4004..0x4008].copy_from_slice(&0x00ff0000u32.to_le_bytes());
            ram[0x4008..0x400c].copy_from_slice(&0u32.to_le_bytes());
            ram[0x400c..0x4010].copy_from_slice(&0u32.to_le_bytes());
            ram[0x4010..0x4014].copy_from_slice(&4u32.to_le_bytes());
            ram[0x4014..0x4018].copy_from_slice(&4u32.to_le_bytes());
            // Avail ring: flags=0, index=1, head=0. Used ring: flags=0, index=0, head=0.
            ram[0x2000..0x2002].copy_from_slice(&0u16.to_le_bytes());
            ram[0x2002..0x2004].copy_from_slice(&1u16.to_le_bytes());
            ram[0x2004..0x200c].copy_from_slice(&0u64.to_le_bytes());
            ram[0x3000..0x3002].copy_from_slice(&0u16.to_le_bytes());
            ram[0x3002..0x300a].copy_from_slice(&0u64.to_le_bytes());
            let mut memory=GuestMemory::new(vec![RamRegion{gpa:0,bytes:&mut ram,writable:true}]).map_err(|e|e.to_string())?;
            let mut queue=SplitQueue::new(&memory,8,desc_base,avail,used).map_err(|e|e.to_string())?;
            // Allocate the 8x8 XRGB8888 scanout surface (pitch 32) the device rasterizes into.
            let mut backend=ScanoutBackend{surface:Surface::allocate(8,8,FORMAT_XRGB8888,32).map_err(|e|e.to_string())?};
            // Drive the real FILL_2D command through the device dispatcher.
            let completion=virtio_gpu::process_one(&mut memory,&mut queue,&mut backend).map_err(|e|e.to_string())?
                .ok_or("GPU command queue produced no completion".to_string())?;
            if completion.status!=0 {return Err(format!("GPU FILL_2D completion status {}",completion.status));}
            // Verify the device actually rasterized the 4x4 red rectangle into the scanout.
            let surface=backend.scanout();
            if surface.width!=8||surface.height!=8||surface.pitch!=32 {return Err("GPU scanout surface geometry mismatch".into());}
            let red=[0x00u8,0x00,0xff,0x00];
            for row in 0..4u32 {
                let start=(row as usize)*32;
                for col in 0..4u32 {
                    let at=start+(col as usize)*4;
                    if surface.data[at..at+4]!=red {return Err("GPU rasterized fill pixel mismatch".into());}
                }
            }
            // Close the loop: feed the actual GPU-rasterized scanout surface through
            // the host D3D11 presentation transport and verify byte-preserving readback.
            let rendered=graphics::present(Backend::Warp,surface)?;
            if rendered.pixels!=surface.data {return Err("D3D11 present readback pixels do not match GPU-rasterized scanout".into());}
            let level=rendered.feature_level;
            println!("GPU_PRESENT_TRANSPORT={{\"backend\":\"Warp\",\"feature_level\":{level},\"adapter\":{:?},\"surface\":\"{}x{} XRGB8888\",\"fill_rasterized\":true,\"pixels_verified\":true,\"transport\":\"gpu-rasterize->d3d11-staging->rendertarget->readback\",\"guest_acceleration_verified\":false,\"android_graphics_verified\":false}}",rendered.adapter,surface.width,surface.height);
            Ok(())
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
            println!("Independent Rust foundation; QEMU dependency: none\nZLAG raw-disk import / validation / sector reads: implemented\nVerified ZLAG-backed ATA LBA28 PIO with optional persistent COW overlay: implemented and diagnostic-tested; no IRQ14, DMA or Android integration\nDirect Windows WHP partition lifecycle: implemented, host-dependent\nBounded real-mode WHP CPU/RAM smoke: implemented, host-dependent\nPrivate scalar port-I/O test bus / bounded cancellation: implemented, host-dependent\nZLAG first-sector test loader at 0000:7c00: implemented, no BIOS services\nSeeded protected/long-mode test startup with paging and guest software interrupt: implemented, host-dependent\nLinux bzImage loader, polled COM1, PIT0/IRQ0 and diagnostic PID1 checkpoint: implemented; no general Linux distribution or usable Android Home\nOpt-in VirtIO-MMIO block with dedicated IRQ5 and Linux vda/COW separate-boot raw-block persistence: diagnostic-tested including ext4 clean remount/journal replay; original Android second-stage init verified separately; not framework/Home\nOpt-in 800x600 software scanout -> Linux fbdev writes -> exact PPM capture: diagnostic-tested with native Win32 software presentation; player UI titlebar (Back/Home/Recents/Fullscreen/Keymap/Volume/RAM cleaner/Install APK/Settings) pushes bridge commands to the guest agent; per-app keymaps translate keydown events to bridge commands; no guest acceleration; --renderer=software selects the software scanout path (the only implemented renderer); --renderer=hardware is refused at plan validation (guest GPU acceleration not implemented)\nOpt-in private COM2 bridge query probe with IRQ3: Linux diagnostic round-trip tested; Android daemon/APK installation/network/audio NOT IMPLEMENTED on new engine\nHost D3D11 shader/readback validation: implemented with explicit hardware request or WARP, no guest GPU transport\nFirmware / full platform devices / usable Android Home: NOT IMPLEMENTED\nGuest DirectX / OpenGL / Vulkan acceleration: NOT IMPLEMENTED\nNo performance or game-compatibility certification"); Ok(())
        }
        Some("physical-check") if args.len()==2 => {
            // Physical Windows host verification: OS identity + WHP capability +
            // real vCPU execution. Records ACTUAL outcomes in machine-checkable
            // evidence JSON. Exits nonzero only when every gate passes (strict);
            // the JSON is always written so CI can validate the schema on a host
            // that lacks nested virtualization while the strict gate (--require-win10
            // --require-whp) is applied on a physical Windows 10/11 machine.
            let os=whp::host_os()?;
            let os_json=format!("{{\"major\":{},\"minor\":{},\"build\":{},\"platform\":{},\"product_type\":{},\"display\":\"{}\",\"is_windows_client\":{}}}",
                os.major,os.minor,os.build,os.platform,os.product_type,os.display(),os.is_windows_client());
            let probe=whp::probe();
            let cpu=whp::cpu_smoke();
            let whp_ok=probe.is_ok()&&cpu.is_ok();
            let whp_detail=format!("{:?}/{:?}",probe.as_ref().err().map(|e|e.as_str()),cpu.as_ref().err().map(|e|e.as_str()));
            let verdict=if whp_ok {"pass"} else {"fail"};
            let evidence=format!("{{\"schema\":1,\"host_os\":{},\"whp_capability_present\":{},\"whp_partition_lifecycle\":\"{}\",\"guest_vcpu_executed\":{},\"verdict\":\"{}\",\"boundary\":\"Physical Windows host WHP partition lifecycle and real vCPU execution verified; NOT Android boot, NOT GPU acceleration, NOT networking/audio, NOT usable Home\",\"whp_detail\":\"{}\"}}",
                os_json,whp_ok,if probe.is_ok(){"ok"}else{"failed"},whp_ok,verdict,whp_detail.replace("\"",""));
            println!("NATIVE_WIN10_PHYSICAL_JSON={evidence}");
            std::fs::write("native-win10-physical.json",&evidence).map_err(|e|e.to_string())?;
            if whp_ok {
                println!("Physical Windows host check PASS: WHP partition lifecycle + real vCPU execution on {}; not Android/GPU/network/audio/Home",os.display());
                Ok(())
            } else {
                Err(format!("Physical Windows host WHP check FAILED on {}: {}; this host lacks WHP/nested virtualization — run --require-whp on a physical Windows 10/11 x64 host",os.display(),whp_detail))
            }
        }
        Some("boot") => Err("Android boot is not implemented. Refusing to pretend this is a working emulator or launch QEMU.".into()),
        _ => Err("Usage: nexus-engine import-raw <stopped-raw.img> <new-gustimg.zlag> | verify <gustimg.zlag> | whp-probe | cpu-smoke | io-smoke | cancel-smoke | protected-smoke | long-smoke | timer-irq-smoke | mmio-smoke | net-smoke | net-mmio-smoke | audio-smoke | audio-real-smoke | audio-mmio-smoke | platform-mmio-smoke | gpu-mmio-smoke | ata-pio-smoke <test.zlag> | ata-cow-smoke <test.zlag> <NEW-overlay.cow> | boot-test-sector <test.zlag> | firmware-boot-smoke | kernel-contract-smoke | physical-check | inspect-kernel <bzImage> | boot-kernel <bzImage> <NEW-serial.log> <cmdline> [initramfs] | boot-kernel-disk-create <bzImage> <NEW-serial.log> <cmdline> <initramfs> <base.zlag> <NEW-overlay.cow> | boot-kernel-disk-open <bzImage> <NEW-serial.log> <cmdline> <initramfs> <base.zlag> <existing-overlay.cow> | probe-android-init-create <bzImage> <NEW-serial.log> <cmdline> <original-initrd> <base.zlag> <NEW-overlay.cow> (second-stage init probe only, not Android Home) | probe-android-init-open <bzImage> <NEW-serial.log> <cmdline> <original-initrd> <base.zlag> <existing-overlay.cow> | probe-android-observe-create <bzImage> <NEW-serial.log> <cmdline> <original-initrd> <base.zlag> <NEW-overlay.cow> (second-stage init + userspace observe) | probe-android-home-create <bzImage> <NEW-serial.log> <cmdline> <original-initrd> <base.zlag> <NEW-overlay.cow> (clean-launch usable-Home certification, no APK; pair with --bridge-home-check) | graphics-smoke-hardware <NEW-64x64.rgba> | graphics-smoke-warp <NEW-64x64.rgba> | graphics-present-smoke | gpu-present-smoke | capabilities (boot-kernel accepts experimental --apic-probe with --cpus=1..8 (headless Linux initramfs/disk, no Android/player); kernel boot commands accept trailing --bridge-probe (query-only integration checkpoint), --bridge-home-check (clean-launch Android Home certification), --bridge-apk-smoke=<apk> (APK install over COM2), --window (software console player), --ram-mib=128..3072, --renderer=software|hardware (software is the only implemented renderer; hardware is refused), --gpu-probe (activate the VirtIO-MMIO GPU device) and --framebuffer=NEW-file.ppm; software scanout, not GPU acceleration)".into())
    }
}
fn main() { let start=std::time::Instant::now(); if let Err(error)=execute(&std::env::args().collect::<Vec<_>>(),start) { eprintln!("Nexus native engine: {error}"); std::process::exit(1); } }

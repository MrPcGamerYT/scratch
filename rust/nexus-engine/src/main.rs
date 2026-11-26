use nexus_engine::{image::{self,Image},whp};
use std::path::Path;
fn execute(args:&[String])->Result<(),String> {
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
        Some("cpu-smoke") if args.len()==2 => { whp::cpu_smoke()?; println!("Direct WHP guest CPU store/load/HLT test passed: AX=0x5a17. Not an Android boot or performance test."); Ok(()) }
        Some("io-smoke") if args.len()==2 => { whp::io_smoke()?; println!("Direct WHP scalar port I/O, register widths and fail-closed exit tests passed. Private test devices only."); Ok(()) }
        Some("cancel-smoke") if args.len()==2 => { whp::cancel_smoke()?; println!("Direct WHP infinite-loop cancellation and same-vCPU restart passed."); Ok(()) }
        Some("protected-smoke") if args.len()==2 => { whp::protected_smoke()?; println!("Direct WHP seeded protected-mode 32-bit execution, flat memory and scalar I/O passed. Not a kernel boot."); Ok(()) }
        Some("long-smoke") if args.len()==2 => { whp::long_smoke()?; println!("Direct WHP seeded 64-bit execution, nonidentity paging alias, guest INT/IRETQ and missing-PTE refusal passed. Not a kernel boot."); Ok(()) }
        Some("boot-test-sector") if args.len()==3 => {
            let console=whp::boot_test_sector(Path::new(&args[2]))?;
            println!("ZLAG test sector executed at 0000:7c00 and halted. Debug: {}",console.escape_ascii());
            println!("No BIOS, UEFI or disk-device services. This is NOT an Android boot."); Ok(())
        }
        Some("capabilities") if args.len()==2 => {
            println!("Independent Rust foundation; QEMU dependency: none\nZLAG raw-disk import / validation / sector reads: implemented\nDirect Windows WHP partition lifecycle: implemented, host-dependent\nBounded real-mode WHP CPU/RAM smoke: implemented, host-dependent\nPrivate scalar port-I/O test bus / bounded cancellation: implemented, host-dependent\nZLAG first-sector test loader at 0000:7c00: implemented, no BIOS services\nSeeded protected/long-mode test startup with paging and guest software interrupt: implemented, host-dependent\nFirmware / real platform devices / Android boot: NOT IMPLEMENTED\nGuest DirectX / OpenGL / Vulkan acceleration: NOT IMPLEMENTED\nNo performance or game-compatibility certification"); Ok(())
        }
        Some("boot") => Err("Android boot is not implemented. Refusing to pretend this is a working emulator or launch QEMU.".into()),
        _ => Err("Usage: nexus-engine import-raw <stopped-raw.img> <new-gustimg.zlag> | verify <gustimg.zlag> | whp-probe | cpu-smoke | io-smoke | cancel-smoke | protected-smoke | long-smoke | boot-test-sector <test.zlag> | capabilities".into())
    }
}
fn main() { if let Err(error)=execute(&std::env::args().collect::<Vec<_>>()) { eprintln!("Nexus native engine: {error}"); std::process::exit(1); } }

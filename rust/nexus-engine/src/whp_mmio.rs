// Included inside whp::windows. Callback lifetimes are limited to synchronous emulation.
#[repr(C)]
struct EmulatorMemory {gpa:u64,direction:u8,size:u8,data:[u8;8]}
#[repr(C)]
struct EmulatorCallbacks {
    size:u32,reserved:u32,
    io:extern "system" fn(*mut c_void,*mut c_void)->i32,
    memory:extern "system" fn(*mut c_void,*mut EmulatorMemory)->i32,
    get:extern "system" fn(*mut c_void,*const u32,u32,*mut Register)->i32,
    set:extern "system" fn(*mut c_void,*const u32,u32,*const Register)->i32,
    translate:extern "system" fn(*mut c_void,u64,u32,*mut u32,*mut u64)->i32,
}
#[link(name="WinHvEmulation")]
extern "system" {
    fn WHvEmulatorCreateEmulator(callbacks:*const EmulatorCallbacks,emulator:*mut *mut c_void)->i32;
    fn WHvEmulatorDestroyEmulator(emulator:*mut c_void)->i32;
    fn WHvEmulatorTryMmioEmulation(emulator:*mut c_void,context:*mut c_void,vp:*const u8,access:*const u8,status:*mut u32)->i32;
}
#[repr(C)]
struct Translation {code:u32,reserved:u32}
#[link(name="WinHvPlatform")]
extern "system" {
    fn WHvTranslateGva(partition:*mut c_void,index:u32,gva:u64,flags:u32,result:*mut Translation,gpa:*mut u64)->i32;
}
const MMIO_BASE:u64=0xd0000000;
/// Secondary VirtIO-MMIO base for the net device; distinct from the block base so
/// both devices can coexist under one emulator callback.
const MMIO_NET_BASE:u64=0xd0001000;
/// Which VirtIO-MMIO device the context services. Block is the default so the
/// existing disk boot path is unchanged; Net is activated only when a net device
/// is requested (wiring in place; no boot plan activates it yet).
enum MmioDevice {
    Block(crate::virtio_mmio::BlockMmio),
    #[allow(dead_code)]
    Net(crate::virtio_net_mmio::NetMmio),
}
impl MmioDevice {
    fn read(&self,offset:u64,size:u8)->Result<u32,String> {match self {MmioDevice::Block(d)=>d.read(offset,size),MmioDevice::Net(d)=>d.read(offset,size)}}
    fn write(&mut self,offset:u64,size:u8,value:u32)->Result<(),String> {match self {MmioDevice::Block(d)=>d.write(offset,size,value),MmioDevice::Net(d)=>d.write(offset,size,value)}}
    fn work_pending(&self)->bool {match self {MmioDevice::Block(d)=>d.work_pending(),MmioDevice::Net(d)=>d.work_pending()}}
    fn irq_asserted(&self)->bool {match self {MmioDevice::Block(d)=>d.irq_asserted(),MmioDevice::Net(d)=>d.irq_asserted()}}
}
struct MmioContext {
    partition:*mut c_void,ram:*mut u8,bytes:usize,device:MmioDevice,
    error:Option<String>,translations:u64,accesses:u64,
}
fn callback(context:*mut c_void,action:impl FnOnce(&mut MmioContext)->Result<(),String>)->i32 {
    if context.is_null() {return 0x80004003u32 as i32;}
    // Context is our stack-owned object; SDK callbacks are synchronous and never retained.
    let ctx=unsafe {&mut *context.cast::<MmioContext>()};
    if ctx.error.is_some() {return 0x80004005u32 as i32;}
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(||action(ctx))) {
        Ok(Ok(()))=>0,
        Ok(Err(error))=>{ctx.error=Some(error);0x80004005u32 as i32},
        Err(_)=>{ctx.error=Some("Panic contained inside MMIO emulator callback".into());0x80004005u32 as i32},
    }
}
extern "system" fn emu_io(context:*mut c_void,_access:*mut c_void)->i32 {callback(context,|_|Err("Unexpected port callback during MMIO emulation".into()))}
extern "system" fn emu_get(context:*mut c_void,names:*const u32,count:u32,values:*mut Register)->i32 {
    callback(context,|ctx| {
        if names.is_null()||values.is_null()||count==0||count>256 {return Err("Invalid register callback buffers".into());}
        unsafe {check(WHvGetVirtualProcessorRegisters(ctx.partition,0,names,count,values),"MMIO get registers")}
    })
}
extern "system" fn emu_set(context:*mut c_void,names:*const u32,count:u32,values:*const Register)->i32 {
    callback(context,|ctx| {
        if names.is_null()||values.is_null()||count==0||count>256 {return Err("Invalid register callback buffers".into());}
        unsafe {check(WHvSetVirtualProcessorRegisters(ctx.partition,0,names,count,values),"MMIO set registers")}
    })
}
extern "system" fn emu_translate(context:*mut c_void,gva:u64,flags:u32,result:*mut u32,gpa:*mut u64)->i32 {
    callback(context,|ctx| {
        if result.is_null()||gpa.is_null() {return Err("Invalid translation callback outputs".into());}
        let mut translated=Translation{code:0,reserved:0};let mut address=0;
        unsafe {check(WHvTranslateGva(ctx.partition,0,gva,flags,&mut translated,&mut address),"WHvTranslateGva")?;*result=translated.code;*gpa=address;}
        ctx.translations+=1;Ok(())
    })
}
extern "system" fn emu_memory(context:*mut c_void,access:*mut EmulatorMemory)->i32 {
    callback(context,|ctx| {
        if access.is_null() {return Err("Invalid memory callback buffer".into());}
        let a=unsafe {&mut *access};let n=usize::from(a.size);
        if n==0||n>8||a.direction>1 {return Err("Unsupported emulator memory shape".into());}
        let end=a.gpa.checked_add(n as u64).ok_or("MMIO callback address overflow")?;
        if end<=ctx.bytes as u64 {
            // Single RAM allocation, bounded before pointer arithmetic; vCPU is stopped.
            let ram=unsafe {std::slice::from_raw_parts_mut(ctx.ram.add(a.gpa as usize),n)};
            if a.direction==0 {a.data[..n].copy_from_slice(ram);} else {ram.copy_from_slice(&a.data[..n]);}return Ok(());
        }
        if !((a.gpa>=MMIO_BASE&&end<=MMIO_BASE+4096)||(a.gpa>=MMIO_NET_BASE&&end<=MMIO_NET_BASE+4096)) {return Err("Emulator memory access outside RAM/device page".into());}
        if ![1,2,4].contains(&a.size) {return Err("Unsupported MMIO register access width".into());}
        let offset=if a.gpa>=MMIO_NET_BASE {a.gpa-MMIO_NET_BASE} else {a.gpa-MMIO_BASE};
        if a.direction==0 {let value=ctx.device.read(offset,a.size)?.to_le_bytes();a.data[..n].copy_from_slice(&value[..n]);}
        else {
            // Defined RO registers/config ignore writes, without changing device state.
            // Config space bound differs per device: block 0x100..0x200, net MAC 0x100..0x106.
            let is_net=matches!(ctx.device,MmioDevice::Net(_));
            let config_end=if is_net {0x106u64} else {0x200u64};
            let config=(0x100..config_end).contains(&offset)&&end<=(if is_net {MMIO_NET_BASE} else {MMIO_BASE})+config_end&&[1,2,4].contains(&a.size);
            let readonly=[0,4,8,0xc,0x10,0x34,0x60,0xfc].contains(&offset)&&a.size==4&&offset%4==0;
            if !config&&!readonly {
                let value=u32::from_le_bytes(a.data[..4].try_into().unwrap());ctx.device.write(offset,a.size,value)?;
            }
        }
        ctx.accesses+=1;Ok(())
    })
}
#[repr(C,align(8))]
struct MemoryExit([u8;40]);
struct Emulator {handle:*mut c_void,_callbacks:Box<EmulatorCallbacks>}
impl Emulator {
    fn new()->Result<Self,String> {
        let callbacks=Box::new(EmulatorCallbacks{size:std::mem::size_of::<EmulatorCallbacks>() as u32,reserved:0,io:emu_io,memory:emu_memory,get:emu_get,set:emu_set,translate:emu_translate});
        let mut handle=std::ptr::null_mut();unsafe {check(WHvEmulatorCreateEmulator(&*callbacks,&mut handle),"WHvEmulatorCreateEmulator")?;}
        if handle.is_null() {return Err("WHP emulator returned null handle".into());}Ok(Self{handle,_callbacks:callbacks})
    }
    fn emulate(&self,ctx:&mut MmioContext,exit:&ExitContext,fetch:bool)->Result<(),String> {
        let mut access=MemoryExit([0;40]);access.0.copy_from_slice(&exit.0[48..88]);
        if fetch {access.0[0]=0;access.0[4..20].fill(0);} // Exercise GVA translation and RAM fetch, not only cached instruction bytes.
        let mut status=0;
        let hr=unsafe {WHvEmulatorTryMmioEmulation(self.handle,(ctx as *mut MmioContext).cast(),exit.0[8..].as_ptr(),access.0.as_ptr(),&mut status)};
        if let Some(error)=&ctx.error {return Err(error.clone());}check(hr,"WHvEmulatorTryMmioEmulation")?;
        if status!=1 {return Err(format!("MMIO emulator did not complete instruction: status=0x{status:x}"));}Ok(())
    }
}
impl Drop for Emulator {fn drop(&mut self) {if unsafe {WHvEmulatorDestroyEmulator(self.handle)}<0 {eprintln!("NATIVE_TEARDOWN_ERROR emulator delete");}}}

struct LiveBlock {context:MmioContext,emulator:Emulator,backend:crate::overlay::Overlay}
impl LiveBlock {
    fn access(&mut self,exit:&ExitContext)->Result<(),String> {
        self.emulator.emulate(&mut self.context,exit,false)?;
        if self.context.device.work_pending() {
            // Sole vCPU stopped, synchronous emulator callbacks have all returned.
            let ram=unsafe {std::slice::from_raw_parts_mut(self.context.ram,self.context.bytes)};
            let mut memory=crate::guest_memory::GuestMemory::new(vec![crate::guest_memory::RamRegion{gpa:0,bytes:ram,writable:true}])?;
            if let MmioDevice::Block(device)=&mut self.context.device {device.service(&mut memory,&mut self.backend,256)?;}
        }
        Ok(())
    }
}

/// Live net device: same MMIO emulation pattern as the block device, but services
/// the VirtIO-net queues with a loopback backend. Activated only when a net device
/// is requested; the block boot path is unaffected.
struct LiveNet {context:MmioContext,emulator:Emulator,backend:crate::virtio_net::Loopback}
impl LiveNet {
    fn access(&mut self,exit:&ExitContext)->Result<(),String> {
        self.emulator.emulate(&mut self.context,exit,false)?;
        if self.context.device.work_pending() {
            let ram=unsafe {std::slice::from_raw_parts_mut(self.context.ram,self.context.bytes)};
            let mut memory=crate::guest_memory::GuestMemory::new(vec![crate::guest_memory::RamRegion{gpa:0,bytes:ram,writable:true}])?;
            if let MmioDevice::Net(device)=&mut self.context.device {device.service(&mut memory,&mut self.backend,256)?;}
        }
        Ok(())
    }
}

pub fn mmio_smoke()->Result<(),String> {
    use crate::cpu;
    for long in [false,true] {
        let mode=if long {cpu::CpuMode::Long64} else {cpu::CpuMode::Protected32};let mut f=cpu::fixture(mode)?;
        // Guest VA/GPA d0000000 mapped in guest page tables, but absent from host GPA mapping.
        if long {
            f.ram[0x2018..0x2020].copy_from_slice(&0x7003u64.to_le_bytes());
            f.ram[0x7400..0x7408].copy_from_slice(&(MMIO_BASE|0x83).to_le_bytes());
        }
        // Three requests: write sector 1, flush, then read sector 1 into a distinct buffer.
        // Rings are fixture input; guest code itself negotiates, starts and notifies the queue.
        for (index,address,length,flags,next) in [
            (0,0x22000u64,16u32,1u16,1u16),(1,0x23000,512,1,2),(2,0x24000,1,2,0),
            (3,0x22010,16,1,4),(4,0x24001,1,2,0),
            (5,0x22020,16,1,6),(6,0x23200,512,3,7),(7,0x24002,1,2,0),
        ] {
            let at=0x20000+index*16;
            f.ram[at..at+8].copy_from_slice(&address.to_le_bytes());
            f.ram[at+8..at+12].copy_from_slice(&length.to_le_bytes());
            f.ram[at+12..at+14].copy_from_slice(&flags.to_le_bytes());
            f.ram[at+14..at+16].copy_from_slice(&next.to_le_bytes());
        }
        for (at,value) in [(0x21002,3u16),(0x21004,0),(0x21006,3),(0x21008,5)] {f.ram[at..at+2].copy_from_slice(&value.to_le_bytes());}
        f.ram[0x22000..0x22004].copy_from_slice(&1u32.to_le_bytes());
        f.ram[0x22008..0x22010].copy_from_slice(&1u64.to_le_bytes());
        f.ram[0x22010..0x22014].copy_from_slice(&4u32.to_le_bytes());
        f.ram[0x22028..0x22030].copy_from_slice(&1u64.to_le_bytes());
        for (n,byte) in f.ram[0x23000..0x23200].iter_mut().enumerate() {*byte=(n as u8).wrapping_mul(37).wrapping_add(9);}
        f.ram[0x24000..0x24003].fill(0xff);
        if !long {
            f.idt_limit=0x12f;
            f.ram[cpu::IDT+0x128..cpu::IDT+0x130].copy_from_slice(&[0,0x90,8,0,0,0x8e,0,0]);
            // Preserve EAX; capture IRQ status; ACK device before PIC EOI; IRETD.
            let handler=[0x50,0xa1,0x60,0,0,0xd0,0xa3,0x30,0xb0,1,0,
                0xb8,1,0,0,0,0xa3,0x64,0,0,0xd0,0xff,0x05,0x34,0xb0,1,0,
                0xb0,0x20,0xe6,0x20,0x58,0xcf];
            f.ram[0x9000..0x9000+handler.len()].copy_from_slice(&handler);
        }
        let mut code=Vec::new();
        let load=|code:&mut Vec<u8>,offset:u32,size:u8,dest:u32| {
            if long {code.push(0x67);}code.push(if size==1 {0xa0} else {0xa1});code.extend_from_slice(&((MMIO_BASE as u32)+offset).to_le_bytes());
            if size==1 {code.extend_from_slice(&[0x0f,0xb6,0xc0]);}
            if long {code.push(0x67);}code.push(0xa3);code.extend_from_slice(&dest.to_le_bytes());
        };
        let write=|code:&mut Vec<u8>,offset:u32,value:u32| {
            code.push(0xb8);code.extend_from_slice(&value.to_le_bytes());if long {code.push(0x67);}code.push(0xa3);code.extend_from_slice(&((MMIO_BASE as u32)+offset).to_le_bytes());
        };
        for (index,offset) in [0,4,8,0x10,0x108,0x10c].into_iter().enumerate() {load(&mut code,offset,4,0x1b000+index as u32*4);}
        write(&mut code,0,0);load(&mut code,0,4,0x1b018); // RO write must be ignored.
        for (offset,value) in [(0x70,1),(0x70,3),(0x24,0),(0x20,518),(0x24,1),(0x20,1),(0x70,11)] {write(&mut code,offset,value);}
        load(&mut code,0x70,4,0x1b01c);load(&mut code,0x108,1,0x1b020);
        if !long {code.extend_from_slice(&[0xfb,0x90]);}
        for (offset,value) in [(0x30,0),(0x38,8),(0x80,0x20000),(0x90,0x21000),(0xa0,0x21800),(0x44,1),(0x70,15),(0x50,0)] {write(&mut code,offset,value);}
        load(&mut code,0x60,4,0x1b028);write(&mut code,0x64,1);load(&mut code,0x60,4,0x1b02c);
        write(&mut code,0x70,0);load(&mut code,0x70,4,0x1b024);code.push(0xf4);
        // Execute long-mode code through the nonidentity VA 400000 -> PA 18000 alias.
        let physical=if long {cpu::DATA} else {cpu::ENTRY};f.ram[physical..physical+code.len()].copy_from_slice(&code);
        with_guest_at(&[],0,cpu::RAM_BYTES,|partition,memory| {
            unsafe {std::ptr::copy_nonoverlapping(f.ram.as_ptr(),memory.address.cast::<u8>(),f.ram.len());}
            configure_fixture(partition,&f)?;if long {set_registers(partition,&[0x10],&[Register::scalar(cpu::ALIAS)])?;}
            let root=std::env::temp_dir().join(format!("nexus-live-mmio-{}-{}",std::process::id(),long));
            std::fs::create_dir(&root).map_err(|e|e.to_string())?;
            let raw=root.join("raw");let base=root.join("base.zlag");let cow=root.join("disk.cow");
            std::fs::write(&raw,[0u8;2048]).map_err(|e|e.to_string())?;
            crate::image::import_raw(&raw,&base).map_err(|e|e.to_string())?;
            let original=std::fs::read(&base).map_err(|e|e.to_string())?;
            let mut backend=crate::overlay::Overlay::create(&base,&cow)?;
            let mut serviced=0;let mut injections=0;
            let mut platform=crate::platform::EarlyPlatform::default();
            if !long {
                for (port,value) in [(0x20,0x11),(0x21,0x20),(0x21,4),(0x21,1),(0x21,0xdf),(0x4d0,0x20)] {platform.pic.access(port,true,value)?;}
            }
            let emulator=Emulator::new()?;let mut ctx=MmioContext{partition:partition.0,ram:memory.address.cast(),bytes:memory.bytes,device:MmioDevice::Block(crate::virtio_mmio::BlockMmio::new(4,false)?),error:None,translations:0,accesses:0};
            let (result,timed_out)=bounded(partition,5000,|| {
                for _ in 0..128 {
                    if !long {
                        platform.pic.set_irq5(ctx.device.irq_asserted());
                        if queue_interrupt(partition,&mut platform)?.is_some() {injections+=1;}
                    }
                    let exit=run_once(partition)?;
                    match exit.reason() {8=>return Ok(Ok(())),1=>{
                        emulator.emulate(&mut ctx,&exit,long)?;
                        if ctx.device.work_pending() {
                            // Emulator returned and sole vCPU is stopped. No callback/RAM borrow is live.
                            let ram=unsafe {std::slice::from_raw_parts_mut(ctx.ram,ctx.bytes)};
                            let mut memory=crate::guest_memory::GuestMemory::new(vec![crate::guest_memory::RamRegion{gpa:0,bytes:ram,writable:true}])?;
                            let done=if let MmioDevice::Block(device)=&mut ctx.device {device.service(&mut memory,&mut backend,8)?} else {return Err("MMIO smoke context is not a block device".into())};
                            if done.iter().any(|c|c.status!=0) {return Err("Live MMIO block request failed".into());}
                            serviced+=done.len();
                        }
                    },2=>{
                        let io=crate::io::IoExit::decode(&exit.0)?;
                        if io.port!=0x20||!io.write||io.size!=1 {return Err("Unexpected MMIO fixture port access".into());}
                        let mode=crate::cpu::CpuMode::from_exit(&exit.0)?;
                        platform.pic.set_irq5(ctx.device.irq_asserted());
                        platform.access(io,0)?;
                        set_registers(partition,&[0x10],&[Register::scalar(mode.next_rip(io.rip,io.instruction_len)?)])?;
                    },reason=>return Err(format!("Unexpected MMIO fixture exit 0x{reason:x}"))}
                }Ok(Err("MMIO fixture exit budget exceeded".to_string()))
            })?;
            if timed_out {return Err("MMIO fixture deadline exceeded".into());}result?;
            let bytes=unsafe {std::slice::from_raw_parts(memory.address.cast::<u8>().add(0x1b000),48)};
            for (n,expected) in [0x74726976,2,2,518,131072,1,0x74726976,11,0,0,u32::from(long),0].into_iter().enumerate() {
                let actual=u32::from_le_bytes(bytes[n*4..n*4+4].try_into().unwrap());if actual!=expected {return Err(format!("MMIO register {n} mismatch 0x{actual:x}"));}
            }
            if !long {
                let captured=unsafe {std::slice::from_raw_parts(ctx.ram.add(0x1b030),8)};
                if captured!=[1,0,0,0,1,0,0,0]||injections!=1||platform.pic.pending_vector().is_some()||scalar_register(partition,4)?!=cpu::STACK {
                    return Err("VirtIO external IRQ5 delivery/ACK/EOI/IRETD contract failed".into());
                }
            }
            if serviced!=3 {return Err("Missing live write/flush/read completions".into());}
            let ram=unsafe {std::slice::from_raw_parts(ctx.ram,ctx.bytes)};
            if ram[0x23000..0x23200]!=ram[0x23200..0x23400]||ram[0x24000..0x24003]!=[0;3]||ram[0x21802..0x21804]!=3u16.to_le_bytes() {
                return Err("Live MMIO DMA data/status/used index mismatch".into());
            }
            drop(backend);
            let mut reopened=crate::overlay::Overlay::open(&base,&cow)?;
            let mut persisted=[0;512];reopened.read_sectors(1,&mut persisted)?;
            if persisted!=ram[0x23000..0x23200]||std::fs::read(&base).map_err(|e|e.to_string())?!=original {return Err("Live MMIO COW persistence/base integrity mismatch".into());}
            drop(reopened);std::fs::remove_dir_all(&root).map_err(|e|e.to_string())?;
            if ctx.accesses<10||(long&&ctx.translations==0) {return Err("Missing real MMIO/translation callback execution".into());}Ok(())
        })?;
    }Ok(())
}

/// Live WHP MMIO smoke for the VirtIO-net device: same emulator-callback path as
/// `mmio_smoke`, but routes net-base GPAs to a `LiveNet` with a loopback backend.
/// Proves the enum MMIO dispatch, GPA routing, IRQ6 assertion and a full
/// TX→loopback→RX round-trip through the real WHP emulator on a Windows host.
pub fn net_mmio_smoke()->Result<(),String> {
    use crate::cpu;
    for long in [false,true] {
        let mode=if long {cpu::CpuMode::Long64} else {cpu::CpuMode::Protected32};let mut f=cpu::fixture(mode)?;
        // Map the net MMIO base in guest page tables (long mode only).
        if long {
            f.ram[0x2018..0x2020].copy_from_slice(&0x7003u64.to_le_bytes());
            f.ram[0x7400..0x7408].copy_from_slice(&(MMIO_NET_BASE|0x83).to_le_bytes());
        }
        // Fixture virtqueues: TX (sel=1) descriptor table 0x20000, avail 0x20100, used 0x20200;
        // RX (sel=0) descriptor table 0x20500, avail 0x20600, used 0x20700.
        // TX packet at 0x20300 (12-byte header + 5-byte payload), TX status at 0x20400.
        // RX buffer at 0x20800 (64 bytes).
        // TX descriptor chain: [packet (RO, NEXT->1), status (WR)].
        let tx_packet_addr=0x20300u64;let tx_status_addr=0x20400u64;
        let descs=[(0x20000u64,tx_packet_addr,17u32,1u16,1u16),(0x20010,tx_status_addr,1,2,0)];
        for (base,address,length,flags,next) in descs {
            let b=base as usize;
            f.ram[b..b+8].copy_from_slice(&address.to_le_bytes());
            f.ram[b+8..b+12].copy_from_slice(&length.to_le_bytes());
            f.ram[b+12..b+14].copy_from_slice(&flags.to_le_bytes());
            f.ram[b+14..b+16].copy_from_slice(&next.to_le_bytes());
        }
        // TX avail ring: flags=0, index=1, head=0. TX used ring: index=0, head=0.
        f.ram[0x20100..0x20102].copy_from_slice(&0u16.to_le_bytes());
        f.ram[0x20102..0x20104].copy_from_slice(&1u16.to_le_bytes());
        f.ram[0x20104..0x2010c].copy_from_slice(&0u64.to_le_bytes());
        f.ram[0x20200..0x20202].copy_from_slice(&0u16.to_le_bytes());
        f.ram[0x20202..0x2020a].copy_from_slice(&0u64.to_le_bytes());
        // TX packet: 12-byte header (dst+src MAC) + "NEXUS".
        let header=crate::virtio_net::net_header();
        f.ram[0x20300..0x2030c].copy_from_slice(&header);
        f.ram[0x2030c..0x20311].copy_from_slice(b"NEXUS");
        // RX descriptor: single writable buffer at 0x20800, len 64.
        f.ram[0x20500..0x20508].copy_from_slice(&0x20800u64.to_le_bytes());
        f.ram[0x20508..0x2050c].copy_from_slice(&64u32.to_le_bytes());
        f.ram[0x2050c..0x2050e].copy_from_slice(&2u16.to_le_bytes());
        f.ram[0x2050e..0x20510].copy_from_slice(&0u16.to_le_bytes());
        // RX avail ring: flags=0, index=1, head=0. RX used ring: index=0, head=0.
        f.ram[0x20600..0x20602].copy_from_slice(&0u16.to_le_bytes());
        f.ram[0x20602..0x20604].copy_from_slice(&1u16.to_le_bytes());
        f.ram[0x20604..0x2060c].copy_from_slice(&0u64.to_le_bytes());
        f.ram[0x20700..0x20702].copy_from_slice(&0u16.to_le_bytes());
        f.ram[0x20702..0x2070a].copy_from_slice(&0u64.to_le_bytes());
        let mut code=Vec::new();
        let load=|code:&mut Vec<u8>,offset:u32,size:u8,dest:u32| {
            if long {code.push(0x67);}code.push(if size==1 {0xa0} else {0xa1});code.extend_from_slice(&((MMIO_NET_BASE as u32)+offset).to_le_bytes());
            if size==1 {code.extend_from_slice(&[0x0f,0xb6,0xc0]);}
            if long {code.push(0x67);}code.push(0xa3);code.extend_from_slice(&dest.to_le_bytes());
        };
        let write=|code:&mut Vec<u8>,offset:u32,value:u32| {
            code.push(0xb8);code.extend_from_slice(&value.to_le_bytes());if long {code.push(0x67);}code.push(0xa3);code.extend_from_slice(&((MMIO_NET_BASE as u32)+offset).to_le_bytes());
        };
        // Read device identity: device ID (0), version (4), vendor ID (0xc), queue num (0x34).
        for (index,offset) in [0,4,0xc,0x34].into_iter().enumerate() {load(&mut code,offset,4,0x1b000+index as u32*4);}
        // Negotiate: ACK, DRIVER, features (F_MAC low, VERSION_1 high), FEATURES_OK, DRIVER_OK.
        for (offset,value) in [(0x70,1),(0x70,3),(0x24,0),(0x20,1),(0x24,1),(0x20,1),(0x70,11),(0x70,15)] {write(&mut code,offset,value);}
        load(&mut code,0x70,4,0x1b010); // status after DRIVER_OK.
        // TX queue setup (sel=1) + notify.
        for (offset,value) in [(0x30,1),(0x38,8),(0x80,0x20000),(0x90,0x20100),(0xa0,0x20200),(0x44,1),(0x50,0)] {write(&mut code,offset,value);}
        // RX queue setup (sel=0) + notify.
        for (offset,value) in [(0x30,0),(0x38,8),(0x80,0x20500),(0x90,0x20600),(0xa0,0x20700),(0x44,1),(0x50,0)] {write(&mut code,offset,value);}
        // Interrupt status + ACK + re-read.
        load(&mut code,0x60,4,0x1b014);write(&mut code,0x64,1);load(&mut code,0x60,4,0x1b018);
        // Reset + read status.
        write(&mut code,0x70,0);load(&mut code,0x70,4,0x1b01c);
        code.push(0xf4); // HLT
        let physical=if long {cpu::DATA} else {cpu::ENTRY};f.ram[physical..physical+code.len()].copy_from_slice(&code);
        with_guest_at(&[],0,cpu::RAM_BYTES,|partition,memory| {
            unsafe {std::ptr::copy_nonoverlapping(f.ram.as_ptr(),memory.address.cast::<u8>(),f.ram.len());}
            configure_fixture(partition,&f)?;if long {set_registers(partition,&[0x10],&[Register::scalar(cpu::ALIAS)])?;}
            let emulator=Emulator::new()?;
            let mut ctx=MmioContext{partition:partition.0,ram:memory.address.cast(),bytes:memory.bytes,device:MmioDevice::Net(crate::virtio_net_mmio::NetMmio::new()),error:None,translations:0,accesses:0};
            let mut backend=crate::virtio_net::Loopback::new();
            let mut serviced_tx=0;let mut irq_seen=false;
            let mut platform=crate::platform::EarlyPlatform::default();
            let (result,timed_out)=bounded(partition,5000,|| {
                for _ in 0..128 {
                    platform.pic.set_irq6(ctx.device.irq_asserted());
                    if ctx.device.irq_asserted() {irq_seen=true;}
                    let exit=run_once(partition)?;
                    match exit.reason() {
                        8=>return Ok(Ok(())),
                        1=>{
                            emulator.emulate(&mut ctx,&exit,long)?;
                            if ctx.device.work_pending() {
                                let ram=unsafe {std::slice::from_raw_parts_mut(ctx.ram,ctx.bytes)};
                                let mut memory=crate::guest_memory::GuestMemory::new(vec![crate::guest_memory::RamRegion{gpa:0,bytes:ram,writable:true}])?;
                                let done=if let MmioDevice::Net(device)=&mut ctx.device {device.service(&mut memory,&mut backend,8)?} else {return Err("Net MMIO smoke context is not a net device".into())};
                                serviced_tx+=done.len();
                            }
                        },
                        2=>{
                            let io=crate::io::IoExit::decode(&exit.0)?;
                            if io.port!=0x20||!io.write||io.size!=1 {return Err("Unexpected net MMIO fixture port access".into());}
                            let mode=crate::cpu::CpuMode::from_exit(&exit.0)?;
                            platform.pic.set_irq6(ctx.device.irq_asserted());
                            platform.access(io,0)?;
                            set_registers(partition,&[0x10],&[Register::scalar(mode.next_rip(io.rip,io.instruction_len)?)])?;
                        },
                        reason=>return Err(format!("Unexpected net MMIO fixture exit 0x{reason:x}"))
                    }
                }Ok(Err("Net MMIO fixture exit budget exceeded".to_string()))
            })?;
            if timed_out {return Err("Net MMIO fixture deadline exceeded".into());}result?;
            // Verify register reads.
            let bytes=unsafe {std::slice::from_raw_parts(memory.address.cast::<u8>().add(0x1b000),32)};
            for (n,expected) in [0x74726976u32,2,0x5358454e,256,15].into_iter().enumerate() {
                let actual=u32::from_le_bytes(bytes[n*4..n*4+4].try_into().unwrap());if actual!=expected {return Err(format!("Net MMIO register {n} mismatch 0x{actual:x}"));}
            }
            // Verify interrupt was asserted and cleared by ACK.
            let irq_before=u32::from_le_bytes(bytes[5*4..5*4+4].try_into().unwrap());
            let irq_after=u32::from_le_bytes(bytes[6*4..6*4+4].try_into().unwrap());
            if irq_before==0||irq_after!=0 {return Err("Net MMIO interrupt status/ACK mismatch".into());}
            // Verify status after reset.
            let status_reset=u32::from_le_bytes(bytes[7*4..7*4+4].try_into().unwrap());
            if status_reset!=0 {return Err("Net MMIO status not cleared by reset".into());}
            // Verify TX completion: status descriptor written with 0.
            let tx_status=u32::from_le_bytes(unsafe {std::slice::from_raw_parts(ctx.ram.add(0x20400),4)}.try_into().unwrap());
            if tx_status!=0 {return Err("Net MMIO TX completion status not success".into());}
            // Verify RX completion: RX buffer contains the loopback frame.
            let rx=unsafe {std::slice::from_raw_parts(ctx.ram.add(0x20800),17)};
            if rx[..12]!=header||&rx[12..]!=b"NEXUS" {return Err("Net MMIO RX loopback frame data mismatch".into());}
            // Verify used-ring indices advanced.
            let tx_used_idx=u16::from_le_bytes(unsafe {std::slice::from_raw_parts(ctx.ram.add(0x20200),2)}.try_into().unwrap());
            let rx_used_idx=u16::from_le_bytes(unsafe {std::slice::from_raw_parts(ctx.ram.add(0x20700),2)}.try_into().unwrap());
            if tx_used_idx!=1||rx_used_idx!=1 {return Err("Net MMIO used-ring index mismatch".into());}
            if !irq_seen {return Err("Net MMIO IRQ6 was never asserted".into());}
            if serviced_tx<2 {return Err("Net MMIO did not complete both TX and RX requests".into());}
            if ctx.accesses<10||(long&&ctx.translations==0) {return Err("Missing real net MMIO/translation callback execution".into());}
            Ok(())
        })?;
    }Ok(())
}

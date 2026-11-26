// Included inside whp::windows. Real WHP AP startup, not fabricated AP markers.
fn registers_at(partition:&Partition,index:u32,names:&[u32],values:&[Register])->Result<(),String> {
    if index>=partition.2||names.len()!=values.len() {return Err("Invalid indexed register access".into());}
    unsafe {check(WHvSetVirtualProcessorRegisters(partition.0,index,names.as_ptr(),names.len() as u32,values.as_ptr()),"WHP indexed registers")}
}
#[repr(C)]
struct InterruptRequest {control:u64,destination:u32,vector:u32}
#[link(name="WinHvPlatform")]
extern "system" {fn WHvRequestInterrupt(partition:*mut c_void,control:*const InterruptRequest,size:u32)->i32;}
enum StartupEvent {Init,Sipi(u8)}
struct ApicContext {partition:*mut c_void,index:u32,ram:*mut u8,bytes:usize,icr_high:u32,spurious:u32,tpr:u32,error:Option<String>,startup:Option<std::sync::mpsc::Sender<StartupEvent>>}
fn apic_callback(context:*mut c_void,f:impl FnOnce(&mut ApicContext)->Result<(),String>)->i32 {
    if context.is_null() {return 0x80004003u32 as i32;}
    let ctx=unsafe {&mut *context.cast::<ApicContext>()};
    if ctx.error.is_some() {return 0x80004005u32 as i32;}
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(||f(ctx))) {
        Ok(Ok(()))=>0,
        Ok(Err(e))=>{ctx.error=Some(e);0x80004005u32 as i32},
        Err(_)=>{ctx.error=Some("APIC callback panic contained".into());0x80004005u32 as i32},
    }
}
extern "system" fn apic_io(ctx:*mut c_void,_a:*mut c_void)->i32 {apic_callback(ctx,|_|Err("Unexpected APIC port emulation".into()))}
extern "system" fn apic_get(ctx:*mut c_void,names:*const u32,count:u32,values:*mut Register)->i32 {
    apic_callback(ctx,|c| {if names.is_null()||values.is_null()||count==0||count>256 {return Err("Invalid APIC register callback".into());}
        unsafe {check(WHvGetVirtualProcessorRegisters(c.partition,c.index,names,count,values),"APIC get registers")}})
}
extern "system" fn apic_set(ctx:*mut c_void,names:*const u32,count:u32,values:*const Register)->i32 {
    apic_callback(ctx,|c| {if names.is_null()||values.is_null()||count==0||count>256 {return Err("Invalid APIC register callback".into());}
        unsafe {check(WHvSetVirtualProcessorRegisters(c.partition,c.index,names,count,values),"APIC set registers")}})
}
extern "system" fn apic_translate(ctx:*mut c_void,gva:u64,flags:u32,result:*mut u32,gpa:*mut u64)->i32 {
    apic_callback(ctx,|c| {
        if result.is_null()||gpa.is_null() {return Err("Invalid APIC translation callback".into());}
        let mut translated=Translation{code:0,reserved:0};let mut address=0;
        unsafe {check(WHvTranslateGva(c.partition,c.index,gva,flags,&mut translated,&mut address),"APIC translate")?;*result=translated.code;*gpa=address;}Ok(())
    })
}
extern "system" fn apic_memory(ctx:*mut c_void,access:*mut EmulatorMemory)->i32 {
    apic_callback(ctx,|c| {
        if access.is_null() {return Err("Invalid APIC memory callback".into());}
        let a=unsafe {&mut *access};let n=usize::from(a.size);
        if n==0||n>8||a.direction>1 {return Err("Invalid APIC memory shape".into());}
        let end=a.gpa.checked_add(n as u64).ok_or("APIC memory overflow")?;
        if end<=c.bytes as u64 {
            // Instruction-fetch bytes only. No host RAM mutation while an AP runs.
            if a.direction!=0 {return Err("Concurrent APIC emulator RAM write refused".into());}
            unsafe {std::ptr::copy_nonoverlapping(c.ram.add(a.gpa as usize),a.data.as_mut_ptr(),n);}return Ok(());
        }
        if !(0xfee00000..0xfee01000).contains(&a.gpa)||a.size!=4||a.gpa%16!=0 {return Err(format!("Unsupported APIC MMIO address/width: cpu={} gpa=0x{:x} size={} direction={}",c.index,a.gpa,a.size,a.direction));}
        let offset=(a.gpa-0xfee00000) as u32;
        if offset==0x310 {
            if a.direction==0 {a.data[..4].copy_from_slice(&c.icr_high.to_le_bytes());}
            else {c.icr_high=u32::from_le_bytes(a.data[..4].try_into().unwrap());}
            return Ok(());
        }
        if a.direction==0 {
            let value=match offset {0x20=>c.index<<24,0x30=>0x00050014,0x80=>c.tpr,0xf0=>c.spurious,_=>return Err(format!("APIC read 0x{offset:x} not implemented"))};
            a.data[..4].copy_from_slice(&value.to_le_bytes());
        } else {
            let value=u32::from_le_bytes(a.data[..4].try_into().unwrap());
            if offset==0x300 {
                if value&!0x0000cfff!=0 {return Err("APIC ICR shorthand/reserved bits not supported".into());}
                let kind=(value>>8)&7;
                if ![0,4,5,6].contains(&kind) {return Err("Unsupported APIC delivery mode".into());}
                let request=InterruptRequest{control:u64::from(kind|((value>>11)&1)<<8|((value>>15)&1)<<12),destination:c.icr_high>>24,vector:value&255};
                if kind==5||kind==6 {
                    if request.destination!=1||value&(1<<11)!=0 {return Err("Startup target must be dormant physical AP1".into());}
                    let tx=c.startup.as_ref().ok_or("No AP startup router on this CPU")?;
                    let event=if kind==5 {StartupEvent::Init} else {StartupEvent::Sipi(value as u8)};
                    tx.send(event).map_err(|_|"AP startup worker disconnected")?;
                } else {unsafe {check(WHvRequestInterrupt(c.partition,&request,16),"APIC ICR delivery")?;}}
            } else {
                match offset {
                    0x80=>c.tpr=value&255,
                    0xf0 if value&!0x1ff==0=>c.spurious=value,
                    _=>return Err(format!("APIC write 0x{offset:x} not implemented")),
                }
            }
        }Ok(())
    })
}
fn run_index(handle:usize,index:u32,ram:usize,bytes:usize,startup:Option<std::sync::mpsc::Sender<StartupEvent>>)->Result<u32,String> {
    let callbacks=Box::new(EmulatorCallbacks{size:std::mem::size_of::<EmulatorCallbacks>() as u32,reserved:0,io:apic_io,memory:apic_memory,get:apic_get,set:apic_set,translate:apic_translate});
    let mut emulator=std::ptr::null_mut();
    unsafe {check(WHvEmulatorCreateEmulator(&*callbacks,&mut emulator),"APIC create emulator")?;}
    if emulator.is_null() {return Err("APIC emulator returned null handle".into());}
    let emulator=Emulator{handle:emulator,_callbacks:callbacks};
    let mut ctx=ApicContext{partition:handle as *mut c_void,index,ram:ram as *mut u8,bytes,icr_high:0,spurious:255,tpr:0,error:None,startup};
    for step in 0..256 {
        let mut exit=ExitContext([0;224]);
        unsafe {check(WHvRunVirtualProcessor(ctx.partition,index,exit.0.as_mut_ptr().cast(),224),"WHP indexed run")?;}
        if handle_cpuid(ctx.partition,index,2,true,&exit)? {continue;}
        if exit.reason()==0x1006 {
            let icr=u64::from_le_bytes(exit.0[48..56].try_into().unwrap());
            let upper=(icr>>32) as u32;let destination=if upper<=255 {upper} else if upper&0x00ffffff==0 {upper>>24} else {return Err(format!("Unsupported startup ICR 0x{icr:x}"));};
            let kind=(icr>>8)&7;
            if destination!=1||icr&(1<<11)!=0 {return Err(format!("Unsupported startup target ICR 0x{icr:x}"));}
            let event=match kind {5=>StartupEvent::Init,6=>StartupEvent::Sipi(icr as u8),_=>return Err(format!("Invalid INIT/SIPI trap 0x{icr:x}"))};
            ctx.startup.as_ref().ok_or("INIT/SIPI trap without startup router")?.send(event).map_err(|_|"AP startup worker disconnected")?;
            continue;
        }
        if exit.reason()!=1 {return Ok(exit.reason());}
        if step<8 {eprintln!("SMP_MMIO cpu={index} rip=0x{:x} gpa=0x{:x}",u64::from_le_bytes(exit.0[32..40].try_into().unwrap()),u64::from_le_bytes(exit.0[72..80].try_into().unwrap()));}
        let mut access=MemoryExit([0;40]);access.0.copy_from_slice(&exit.0[48..88]);let mut status=0;
        let hr=unsafe {WHvEmulatorTryMmioEmulation(emulator.handle,(&mut ctx as *mut ApicContext).cast(),exit.0[8..].as_ptr(),access.0.as_ptr(),&mut status)};
        if let Some(e)=ctx.error.take() {return Err(format!("{e}; RIP=0x{:x}",u64::from_le_bytes(exit.0[32..40].try_into().unwrap())));}check(hr,"APIC emulate")?;
        if status!=1 {return Err(format!("APIC instruction not emulated: {status:x}"));}
    }Err("APIC exit budget exceeded".into())
}
pub fn smp_smoke()->Result<(),String> {
    use crate::cpu;
    let partition=create_machine(2,true)?;
    let memory=allocate(&partition,cpu::RAM_BYTES)?;
    let mut processors=Vec::new();
    for index in 0..2 {
        unsafe {check(WHvCreateVirtualProcessor(partition.0,index,0),"WHP create SMP vCPU")?;}
        processors.push(Processor(&partition,index));
    }
    let mut f=cpu::fixture(cpu::CpuMode::Protected32)?;
    // BSP enables its APIC, targets APIC ID1, issues INIT followed by SIPI vector9.
    // The dormant AP receives its architectural CS:IP only after the guest SIPI.
    let mut code=vec![0xb8,1,0,0,0,0x31,0xc9,0x0f,0xa2,0x89,0x1d,0x10,0xa0,1,0,
        0xb8,0xb,0,0,0,0xb9,1,0,0,0,0x0f,0xa2,0x89,0x15,0x14,0xa0,1,0,0x89,0x1d,0x18,0xa0,1,0];
    code.extend_from_slice(&[
        0xc7,0x05,0xf0,0x00,0xe0,0xfe,0xff,0x01,0x00,0x00,
        0xc7,0x05,0x10,0x03,0xe0,0xfe,0x00,0x00,0x00,0x01,
        0xc7,0x05,0x00,0x03,0xe0,0xfe,0x00,0x45,0x00,0x00,
        0xc7,0x05,0x00,0x03,0xe0,0xfe,0x09,0x46,0x00,0x00,
        0x80,0x3d,0x00,0xa0,0x01,0x00,0x5a,0x75,0xf7,0xf4,
    ]);
    f.ram[cpu::ENTRY..cpu::ENTRY+code.len()].copy_from_slice(&code);
    // AP startup in real mode: DS=0, address-size override selects 32-bit RAM GPA.
    let ap=[0x31,0xc0,0x8e,0xd8,
        0x66,0xb8,1,0,0,0,0x66,0x31,0xc9,0x0f,0xa2,0x66,0x67,0x89,0x1d,0x20,0xa0,1,0,
        0x66,0xb8,0xb,0,0,0,0x66,0xb9,1,0,0,0,0x0f,0xa2,0x66,0x67,0x89,0x15,0x24,0xa0,1,0,
        0x66,0x67,0x89,0x1d,0x28,0xa0,1,0,
        0x67,0xc6,0x05,0x00,0xa0,0x01,0x00,0x5a,0xf4];
    f.ram[0x9000..0x9000+ap.len()].copy_from_slice(&ap);
    unsafe {std::ptr::copy_nonoverlapping(f.ram.as_ptr(),memory.address.cast::<u8>(),f.ram.len());}
    configure_fixture(&partition,&f)?;
    registers_at(&partition,0,&[0x2003],&[Register::scalar(0xfee00900)])?;
    registers_at(&partition,1,&[0x1c,0x1e,0x1f,0x2001,0x13,0x15,0x12,0x14,0x10,0x11,0x2003,0x80000005],&[
        Register::scalar(0x60000010),Register::scalar(0),Register::scalar(0),Register::scalar(0),
        Register::segment(0x9b),Register::segment(0x93),Register::segment(0x93),Register::segment(0x93),
        Register::scalar(0),Register::scalar(2),Register::scalar(0xfee00800),Register::scalar(1)])?;
    let handle=partition.0 as usize;let ram=memory.address as usize;let bytes=memory.bytes;
    let ((bsp,ap),expired)=bounded(&partition,5000,||std::thread::scope(|scope| {
        let (tx,rx)=std::sync::mpsc::channel();
        let ap=scope.spawn(move || {
            let mut initialized=false;
            loop {
                match rx.recv_timeout(std::time::Duration::from_secs(4)).map_err(|_|"AP received no startup event")? {
                    StartupEvent::Init=>initialized=true,
                    StartupEvent::Sipi(vector) if initialized=>{
                        let mut cs=Register::segment(0x9b);
                        cs.0[..8].copy_from_slice(&(u64::from(vector)<<12).to_le_bytes());
                        cs.0[12..14].copy_from_slice(&(u16::from(vector)<<8).to_le_bytes());
                        let names=[0x13,0x10,0x80000005];let values=[cs,Register::scalar(0),Register::scalar(0)];
                        unsafe {check(WHvSetVirtualProcessorRegisters(handle as *mut c_void,1,names.as_ptr(),3,values.as_ptr()),"Guest-requested SIPI entry")?;}
                        return run_index(handle,1,ram,bytes,None);
                    }
                    StartupEvent::Sipi(_)=>return Err("SIPI received before INIT".into()),
                }
            }
        });
        let bsp=run_index(handle,0,ram,bytes,Some(tx));
        let ap=ap.join().map_err(|_|"AP run thread panicked")?;
        Ok((bsp?,ap?))
    }))?;
    if expired||bsp!=8||ap!=8 {return Err(format!("SMP INIT/SIPI failed: BSP exit={bsp:x} AP exit={ap:x} timeout={expired}"));}
    // Both vCPUs have stopped and workers joined before accessing shared RAM.
    let marker=unsafe {*memory.address.cast::<u8>().add(0x1a000)};
    if marker!=0x5a {return Err("AP startup marker missing".into());}
    let word=|at:usize|unsafe {std::ptr::read_unaligned(memory.address.cast::<u8>().add(at).cast::<u32>())};
    for (index,base) in [(0,0x1a010),(1,0x1a020)] {
        if word(base)>>24!=index||(word(base)>>16)&255!=2||word(base+4)!=index||word(base+8)!=2 {
            return Err(format!("Guest CPU{index} topology disagrees with allocated vCPUs"));
        }
    }
    Ok(())
}

// Included inside whp::windows. Experimental two-vCPU startup frontend.
// Guest INIT/SIPI commands drive architectural AP startup; markers are guest stores.
// Exercises fixed IPI routing, not the production Linux SMP scheduler or full LAPIC/IOAPIC.
fn registers_at(partition:&Partition,index:u32,names:&[u32],values:&[Register])->Result<(),String> {
    if index>=partition.2||names.len()!=values.len() {return Err("Invalid indexed register access".into());}
    unsafe {check(WHvSetVirtualProcessorRegisters(partition.0,index,names.as_ptr(),names.len() as u32,values.as_ptr()),"WHP indexed registers")}
}
struct ApicState {platform:crate::platform::EarlyPlatform,pit_edges:u64,epoch:std::time::Instant,cpus:Vec<crate::lapic::LocalApic>,io:crate::ioapic::IoApic}
impl std::ops::Deref for ApicState {type Target=[crate::lapic::LocalApic];fn deref(&self)->&Self::Target {&self.cpus}}
impl std::ops::DerefMut for ApicState {fn deref_mut(&mut self)->&mut Self::Target {&mut self.cpus}}
type ApicFabric=std::sync::Arc<std::sync::Mutex<ApicState>>;
enum StartupEvent {Init,Sipi(u8)}
struct ApicContext {timer_clock:std::time::Instant,partition:*mut c_void,index:u32,ram:*mut u8,bytes:usize,icr_high:u32,lapics:ApicFabric,error:Option<String>,startup:Option<std::sync::mpsc::Sender<StartupEvent>>}
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
        if (0xfec00000..0xfec00020).contains(&a.gpa) {
            if a.size!=4 {return Err("IOAPIC requires 32-bit MMIO".into());}
            let mut state=c.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
            let offset=(a.gpa-0xfec00000) as u32;
            if a.direction==0 {a.data[..4].copy_from_slice(&state.io.read(offset)?.to_le_bytes());}
            else {state.io.write(offset,u32::from_le_bytes(a.data[..4].try_into().unwrap()))?;}
            return Ok(());
        }
        if !(0xfee00000..0xfee01000).contains(&a.gpa)||a.size!=4||a.gpa%16!=0 {return Err(format!("Unsupported APIC MMIO address/width: cpu={} gpa=0x{:x} size={} direction={}",c.index,a.gpa,a.size,a.direction));}
        let now=std::time::Instant::now();
        c.lapics.lock().map_err(|_|"LAPIC lock poisoned")?[c.index as usize].advance_ns(now.duration_since(c.timer_clock).as_nanos().min(u128::from(u64::MAX)) as u64)?;
        c.timer_clock=now;
        let offset=(a.gpa-0xfee00000) as u32;
        if offset==0x310 {
            if a.direction==0 {a.data[..4].copy_from_slice(&c.icr_high.to_le_bytes());}
            else {c.icr_high=u32::from_le_bytes(a.data[..4].try_into().unwrap());}
            return Ok(());
        }
        if a.direction==0 {
            let value=c.lapics.lock().map_err(|_|"LAPIC lock poisoned")?[c.index as usize].read(offset)?;
            a.data[..4].copy_from_slice(&value.to_le_bytes());
        } else {
            let value=u32::from_le_bytes(a.data[..4].try_into().unwrap());
            if offset==0x300 {
                if value&!0x000ccfff!=0 {return Err("APIC ICR shorthand/reserved bits not supported".into());}
                let kind=(value>>8)&7;
                if kind==0 {
                    let shorthand=(value>>18)&3;
                    if value&(1<<15)!=0 {return Err("Fixed IPI requires edge-triggered delivery".into());}
                    let mut cpus=c.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
                    if value&(1<<11)!=0 {crate::lapic::send_fixed_logical(&mut cpus,c.index as usize,(c.icr_high>>24) as u8,shorthand as u8,value as u8)?;}
                    else {crate::lapic::send_fixed(&mut cpus,c.index as usize,(c.icr_high>>24) as u8,shorthand as u8,value as u8)?;}
                    return Ok(());
                }
                if ![5,6].contains(&kind)||value&0xc0000!=0 {return Err("Unsupported APIC delivery mode or startup shorthand".into());}
                if c.icr_high>>24!=1||value&(1<<11)!=0 {return Err("Startup target must be dormant physical AP1".into());}
                let tx=c.startup.as_ref().ok_or("No AP startup router on this CPU")?;
                let event=if kind==5 {StartupEvent::Init} else {StartupEvent::Sipi(value as u8)};
                tx.send(event).map_err(|_|"AP startup worker disconnected")?;
            } else {
                let mut state=c.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
                if offset==0xb0 {
                    let local=&mut state[c.index as usize];
                    let level=local.service_vector().filter(|v|local.in_service_level(*v));
                    local.eoi();if let Some(vector)=level {state.io.eoi(vector);}
                }
                else {state[c.index as usize].write(offset,value)?;}

            }
        }Ok(())
    })
}
fn run_index(handle:usize,index:u32,ram:usize,bytes:usize,startup:Option<std::sync::mpsc::Sender<StartupEvent>>,lapics:ApicFabric)->Result<(u32,u64),String> {
    let callbacks=Box::new(EmulatorCallbacks{size:std::mem::size_of::<EmulatorCallbacks>() as u32,reserved:0,io:apic_io,memory:apic_memory,get:apic_get,set:apic_set,translate:apic_translate});
    let mut emulator=std::ptr::null_mut();
    unsafe {check(WHvEmulatorCreateEmulator(&*callbacks,&mut emulator),"APIC create emulator")?;}
    if emulator.is_null() {return Err("APIC emulator returned null handle".into());}
    let emulator=Emulator{handle:emulator,_callbacks:callbacks};
    let mut ctx=ApicContext{timer_clock:std::time::Instant::now(),partition:handle as *mut c_void,index,ram:ram as *mut u8,bytes,icr_high:0,lapics,error:None,startup};
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
    let mut halted=false;let mut halt_waits=0;
    for _ in 0..50000 {
        if std::time::Instant::now()>=deadline {
            let names=[0x10,0x11];let mut state=[Register::scalar(0);2];
            unsafe {WHvGetVirtualProcessorRegisters(ctx.partition,index,names.as_ptr(),2,state.as_mut_ptr());}
            return Err(format!("LAPIC CPU{index} run deadline exceeded; RIP=0x{:x} flags=0x{:x}",u64::from_le_bytes(state[0].0[..8].try_into().unwrap()),u64::from_le_bytes(state[1].0[..8].try_into().unwrap())));
        }
        {
        let mut fabric=ctx.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
        {
            let ApicState{cpus,io,platform,pit_edges,epoch}=&mut *fabric;
            platform.tick(epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
            platform.route_ioapic(io,pit_edges,false)?;io.service(cpus)?;
        }
        let local=&mut fabric[index as usize];
        let now=std::time::Instant::now();
        local.advance_ns(now.duration_since(ctx.timer_clock).as_nanos().min(u128::from(u64::MAX)) as u64)?;ctx.timer_clock=now;
        if let Some(vector)=local.pending() {
            let names=[0x11,0x80000001,0x80000000];let mut values=[Register::scalar(0);3];
            unsafe {check(WHvGetVirtualProcessorRegisters(ctx.partition,index,names.as_ptr(),3,values.as_mut_ptr()),"LAPIC interrupt window")?;}
            let scalar=|n:usize|u64::from_le_bytes(values[n].0[..8].try_into().unwrap());
            if scalar(0)&0x200!=0&&scalar(1)&1==0&&scalar(2)&1==0 {
                let name=0x80000000;let value=Register::scalar(1|(u64::from(vector)<<16));
                unsafe {check(WHvSetVirtualProcessorRegisters(ctx.partition,index,&name,1,&value),"LAPIC fixed interrupt injection")?;}
                local.acknowledge(vector)?;halted=false;
            }
        }
        }
        if halted {std::thread::sleep(std::time::Duration::from_millis(1));continue;}
        let mut exit=ExitContext([0;224]);
        unsafe {check(WHvRunVirtualProcessor(ctx.partition,index,exit.0.as_mut_ptr().cast(),224),"WHP indexed run")?;}
        if handle_cpuid(ctx.partition,index,2,true,&exit)? {continue;}
        if exit.reason()==0x2001 {continue;}
        if exit.reason()==2 {
            let io=crate::io::IoExit::decode(&exit.0)?;
            let mode=crate::cpu::CpuMode::from_exit(&exit.0)?;
            let response={
                let mut state=ctx.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
                let now=state.epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                state.platform.access(io,now)?
            };
            let rip=Register::scalar(mode.next_rip(io.rip,io.instruction_len)?);
            if let Some(rax)=response {
                let names=[0,0x10];let values=[Register::scalar(rax),rip];
                unsafe {check(WHvSetVirtualProcessorRegisters(ctx.partition,index,names.as_ptr(),2,values.as_ptr()),"APIC platform IN")?;}
            } else {let name=0x10;unsafe {check(WHvSetVirtualProcessorRegisters(ctx.partition,index,&name,1,&rip),"APIC platform OUT")?;}}
            continue;
        }
        if exit.reason()==8 {
            let names=[0x11,0x80000000];let mut state=[Register::scalar(0);2];
            unsafe {check(WHvGetVirtualProcessorRegisters(ctx.partition,index,names.as_ptr(),2,state.as_mut_ptr()),"LAPIC HLT state")?;}
            let flags=u64::from_le_bytes(state[0].0[..8].try_into().unwrap());
            let pending=u64::from_le_bytes(state[1].0[..8].try_into().unwrap());
            if flags&0x200==0 {return Ok((8,halt_waits));}
            // HLT already advanced RIP. A scheduling cancel is not a wake IRQ.
            // If WHP already has a queued event, resume it instead of sleeping.
            if pending&1==0 {halted=true;halt_waits+=1;}
            continue;
        }
        if exit.reason()!=1 {return Ok((exit.reason(),halt_waits));}
        let mut access=MemoryExit([0;40]);access.0.copy_from_slice(&exit.0[48..88]);let mut status=0;
        let hr=unsafe {WHvEmulatorTryMmioEmulation(emulator.handle,(&mut ctx as *mut ApicContext).cast(),exit.0[8..].as_ptr(),access.0.as_ptr(),&mut status)};
        if let Some(e)=ctx.error.take() {return Err(format!("{e}; RIP=0x{:x}",u64::from_le_bytes(exit.0[32..40].try_into().unwrap())));}check(hr,"APIC emulate")?;
        if status!=1 {return Err(format!("APIC instruction not emulated: {status:x}"));}
    }Err("APIC exit budget exceeded".into())
}
pub fn smp_smoke()->Result<(),String> {
    let mut halt_waits=0;for _ in 0..16 {halt_waits+=smp_cycle()?;}
    if halt_waits==0 {return Err("No actual LAPIC halt/wake exercised in startup batch".into());}Ok(())
}
fn smp_cycle()->Result<u64,String> {
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
    let mut code=vec![0xb8,1,0,0,0,0x31,0xc9,0x0f,0xa2,0x89,0x1d,0x10,0xa0,0,0,
        0xb8,0xb,0,0,0,0xb9,1,0,0,0,0x0f,0xa2,0x89,0x15,0x14,0xa0,0,0,0x89,0x1d,0x18,0xa0,0,0];
    code.extend_from_slice(&[
        0xc7,0x05,0xf0,0x00,0xe0,0xfe,0xff,0x01,0x00,0x00,
        0xc7,0x05,0x10,0x03,0xe0,0xfe,0x00,0x00,0x00,0x01,
        0xc7,0x05,0x00,0x03,0xe0,0xfe,0x00,0x45,0x00,0x00,
        0xc7,0x05,0x00,0x03,0xe0,0xfe,0x09,0x46,0x00,0x00,
        0x80,0x3d,0x00,0xa0,0x00,0x00,0x5a,0x75,0xf7,0xf4,
    ]);
    code.pop(); // replace final HLT with STI/self-IPI/handler-return verification
    code.extend_from_slice(&[0xfb,0x90,
        0xc7,0x05,0x80,0,0xe0,0xfe,0x40,0,0,0,
        0xc7,0x05,0x00,0x03,0xe0,0xfe,0x40,0x00,0x04,0x00,
        0xa1,0x20,0x02,0xe0,0xfe,0xa3,0x38,0xa0,0,0,
        0xa1,0x20,0x01,0xe0,0xfe,0xa3,0x3c,0xa0,0,0,
        0xc7,0x05,0x80,0,0xe0,0xfe,0,0,0,0,
        0xa1,0x20,0x01,0xe0,0xfe,0xa3,0x34,0xa0,0,0,
        0xfa, // queue another self IPI with IF=0, then open IF without MMIO exits
        0xc7,0x05,0,3,0xe0,0xfe,0x40,0,4,0,
        0xfb,0x90,0x83,0x3d,0x30,0xa0,0,0,2,0x75,0xf7,
        0xc7,0x05,0x10,0x03,0xe0,0xfe,0,0,0,1,
        0xc7,0x05,0x00,0x03,0xe0,0xfe,0x41,0,0,0,
        0x66,0x83,0x3d,0x40,0xa0,0,0,1,0x75,0xf6,
        0xc7,0x05,0x00,0x03,0xe0,0xfe,0x41,8,0,0,
        0x66,0x83,0x3d,0x40,0xa0,0,0,2,0x75,0xf6,
        0xc7,0x05,0x20,3,0xe0,0xfe,0x40,0,0,0,
        0xc7,0x05,0xe0,3,0xe0,0xfe,0xb,0,0,0,
        0xc7,0x05,0x80,3,0xe0,0xfe,0xa0,0x86,1,0,
        0xfa,0x83,0x3d,0x30,0xa0,0,0,3,0x74,4,0xfb,0xf4,0xeb,0xf2,
        0xc7,0x05,0x20,3,0xe0,0xfe,0x40,0,2,0,
        0xc7,0x05,0x80,3,0xe0,0xfe,0x80,0x84,0x1e,0,
        0xfa,0x83,0x3d,0x30,0xa0,0,0,5,0x74,4,0xfb,0xf4,0xeb,0xf2,
        0xc7,0x05,0x80,3,0xe0,0xfe,0,0,0,0,
        0xc7,0x05,0,0,0xc0,0xfe,0x11,0,0,0,
        0xc7,0x05,0x10,0,0xc0,0xfe,0,0,0,0,
        0xc7,0x05,0,0,0xc0,0xfe,0x10,0,0,0,
        0xc7,0x05,0x10,0,0xc0,0xfe,0x42,0x80,0,0,
        0xfa,0x83,0x3d,0x50,0xa0,0,0,2,0x74,4,0xfb,0xf4,0xeb,0xf2]);
    // Route the actual platform PIT IRQ0 through GSI2/vector43, not a host flag.
    let io_write=|code:&mut Vec<u8>,register:u32,value:u32| {
        code.extend_from_slice(&[0xc7,0x05,0,0,0xc0,0xfe]);code.extend_from_slice(&register.to_le_bytes());
        code.extend_from_slice(&[0xc7,0x05,0x10,0,0xc0,0xfe]);code.extend_from_slice(&value.to_le_bytes());
    };
    io_write(&mut code,0x15,0);io_write(&mut code,0x14,0x43);
    code.extend_from_slice(&[0xb0,0x30,0xe6,0x43,0xb0,0x9c,0xe6,0x40,0xb0,0x2e,0xe6,0x40,
        0xfa,0x83,0x3d,0x60,0xa0,0,0,1,0x74,4,0xfb,0xf4,0xeb,0xf2]);
    // COM1 OUT2+THRE IRQ4, device IIR read clears the source before level EOI.
    io_write(&mut code,0x19,0);io_write(&mut code,0x18,0x8044);
    code.extend_from_slice(&[0x66,0xba,0xfc,3,0xb0,8,0xee,
        0x66,0xba,0xf9,3,0xb0,2,0xee,
        0xfa,0x83,0x3d,0x70,0xa0,0,0,1,0x74,4,0xfb,0xf4,0xeb,0xf2,
        0x66,0xba,0xf8,3,0xb0,0x55,0xee,
        0xfa,0x83,0x3d,0x70,0xa0,0,0,2,0x74,4,0xfb,0xf4,0xeb,0xf2,0xfa,0xf4]);
    f.idt_limit=0x227;
    f.ram[cpu::IDT+0x218..cpu::IDT+0x220].copy_from_slice(&[0,0x9c,8,0,0,0x8e,0,0]);
    f.ram[cpu::IDT+0x220..cpu::IDT+0x228].copy_from_slice(&[0,0x9e,8,0,0,0x8e,0,0]);
    let pit_handler=[0xff,0x05,0x60,0xa0,0,0,0xc7,0x05,0xb0,0,0xe0,0xfe,0,0,0,0,0xcf];
    f.ram[0x9c00..0x9c00+pit_handler.len()].copy_from_slice(&pit_handler);
    let uart_handler=[0x50,0x52,0x66,0xba,0xfa,3,0xec,0x0f,0xb6,0xc0,0xa3,0x74,0xa0,0,0,
        0xff,0x05,0x70,0xa0,0,0,0xc7,0x05,0xb0,0,0xe0,0xfe,0,0,0,0,0x5a,0x58,0xcf];
    f.ram[0x9e00..0x9e00+uart_handler.len()].copy_from_slice(&uart_handler);
    f.ram[cpu::IDT+0x210..cpu::IDT+0x218].copy_from_slice(&[0,0x9a,8,0,0,0x8e,0,0]);
    f.ram[cpu::IDT+0x200..cpu::IDT+0x208].copy_from_slice(&[0,0x94,8,0,0,0x8e,0,0]);
    let handler=[0xff,0x05,0x30,0xa0,0,0,
        0x83,0x3d,0x30,0xa0,0,0,5,0x75,10,
        0xc7,0x05,0x20,3,0xe0,0xfe,0x40,0,3,0,
        0xc7,0x05,0xb0,0,0xe0,0xfe,0,0,0,0,0xcf];
    f.ram[0x9400..0x9400+handler.len()].copy_from_slice(&handler);
    let io_handler=[0xff,0x05,0x50,0xa0,0,0,
        0x83,0x3d,0x50,0xa0,0,0,2,0x75,20,
        0xc7,0x05,0,0,0xc0,0xfe,0x10,0,0,0,
        0xc7,0x05,0x10,0,0xc0,0xfe,0x42,0x80,1,0,
        0xc7,0x05,0xb0,0,0xe0,0xfe,0,0,0,0,0xcf];
    f.ram[0x9a00..0x9a00+io_handler.len()].copy_from_slice(&io_handler);
    f.ram[cpu::ENTRY..cpu::ENTRY+code.len()].copy_from_slice(&code);
    // AP startup in real mode: DS=0, address-size override selects 32-bit RAM GPA.
    let ap=[0x31,0xc0,0x8e,0xd8,0x8e,0xd0,0xbc,0,0x70,
        0x66,0xb8,1,0,0,0,0x66,0x31,0xc9,0x0f,0xa2,0x66,0x67,0x89,0x1d,0x20,0xa0,0,0,
        0x66,0xb8,0xb,0,0,0,0x66,0xb9,1,0,0,0,0x0f,0xa2,0x66,0x67,0x89,0x15,0x24,0xa0,0,0,
        0x66,0x67,0x89,0x1d,0x28,0xa0,0,0,
        0x0f,0x01,0x16,0,0x52, // LGDT [5200]
        0x0f,0x20,0xc0,0x66,0x83,0xc8,1,0x0f,0x22,0xc0,
        0x66,0xea,0,0x96,0,0,8,0]; // enter protected32 at 9600

    f.ram[0x9000..0x9000+ap.len()].copy_from_slice(&ap);
    // High LAPIC MMIO must not be accessed through real-mode 64KiB segments.
    // AP loads its GDT, sets PE and far-jumps before enabling interrupts/MMIO.
    f.ram[0x5200..0x5206].copy_from_slice(&[23,0,0,0x50,0,0]);
    f.ram[0x5300..0x5306].copy_from_slice(&[0x0f,2,0,0xb0,0,0]);
    let ap_protected=[0x66,0xb8,16,0,0x8e,0xd8,0x8e,0xd0,0xbc,0,0x70,0,0,
        0x0f,0x01,0x1d,0,0x53,0,0,
        0xc7,0x05,0xf0,0,0xe0,0xfe,0xff,1,0,0,
        0xc7,0x05,0xd0,0,0xe0,0xfe,0,0,0,1,0xfb,0x90,
        0xc6,0x05,0,0xa0,0,0,0x5a,
        0xfa,0x83,0x3d,0x40,0xa0,0,0,2,0x74,4,0xfb,0xf4,0xeb,0xf2,0xfa,0xf4];
    f.ram[0x9600..0x9600+ap_protected.len()].copy_from_slice(&ap_protected);
    f.ram[0xb208..0xb210].copy_from_slice(&[0,0x98,8,0,0,0x8e,0,0]);
    let ap_handler=[0xff,0x05,0x40,0xa0,0,0,
        0xc7,0x05,0xb0,0,0xe0,0xfe,0,0,0,0,0xcf];
    f.ram[0x9800..0x9800+ap_handler.len()].copy_from_slice(&ap_handler);
    unsafe {std::ptr::copy_nonoverlapping(f.ram.as_ptr(),memory.address.cast::<u8>(),f.ram.len());}
    configure_fixture(&partition,&f)?;
    registers_at(&partition,1,&[0x1c,0x1e,0x1f,0x2001,0x13,0x15,0x12,0x14,0x10,0x11,0x1a],&[
        Register::scalar(0x60000010),Register::scalar(0),Register::scalar(0),Register::scalar(0),
        Register::segment(0x9b),Register::segment(0x93),Register::segment(0x93),Register::segment(0x93),
        Register::scalar(0),Register::scalar(2),Register::table(0,0x3ff)])?;
    let handle=partition.0 as usize;let ram=memory.address as usize;let bytes=memory.bytes;
    let lapics=std::sync::Arc::new(std::sync::Mutex::new(ApicState{platform:crate::platform::EarlyPlatform::default(),pit_edges:0,epoch:std::time::Instant::now(),cpus:vec![crate::lapic::LocalApic::new(0),crate::lapic::LocalApic::new(1)],io:crate::ioapic::IoApic::new()}));
    // Test-only device source: hold IOAPIC input 0 high before guest unmasks it.
    // It must retrigger only after guest EOI, and stop when the handler masks it.
    lapics.lock().map_err(|_|"LAPIC lock poisoned")?.io.set_line(0,true)?;
    let ((bsp,ap,ap_halts),expired)=timer_pump(&partition,||bounded(&partition,5000,||std::thread::scope(|scope| {
        let (tx,rx)=std::sync::mpsc::channel();
        let ap_lapics=lapics.clone();
        let ap=scope.spawn(move || {
            let mut initialized=false;
            loop {
                match rx.recv_timeout(std::time::Duration::from_secs(4)).map_err(|_|"AP received no startup event")? {
                    StartupEvent::Init=>initialized=true,
                    StartupEvent::Sipi(vector) if initialized=>{
                        let mut cs=Register::segment(0x9b);
                        cs.0[..8].copy_from_slice(&(u64::from(vector)<<12).to_le_bytes());
                        cs.0[12..14].copy_from_slice(&(u16::from(vector)<<8).to_le_bytes());
                        let names=[0x13,0x10];let values=[cs,Register::scalar(0)];
                        unsafe {check(WHvSetVirtualProcessorRegisters(handle as *mut c_void,1,names.as_ptr(),2,values.as_ptr()),"Guest-requested SIPI entry")?;}
                        return run_index(handle,1,ram,bytes,None,ap_lapics);
                    }
                    StartupEvent::Sipi(_)=>return Err("SIPI received before INIT".into()),
                }
            }
        });
        let bsp=run_index(handle,0,ram,bytes,Some(tx),lapics.clone());
        let ap=ap.join().map_err(|_|"AP run thread panicked")?;
        let (ap,ap_halts)=ap?;
        if ap!=8 {return Err(format!("AP stopped at unexpected exit 0x{ap:x}"));}
        Ok((bsp?.0,ap,ap_halts))
    })))?;
    if expired||bsp!=8||ap!=8 {return Err(format!("SMP INIT/SIPI failed: BSP exit={bsp:x} AP exit={ap:x} timeout={expired}"));}
    // Both vCPUs have stopped and workers joined before accessing shared RAM.
    let marker=unsafe {*memory.address.cast::<u8>().add(0xa000)};
    if marker!=0x5a {return Err("AP startup marker missing".into());}
    let word=|at:usize|unsafe {std::ptr::read_unaligned(memory.address.cast::<u8>().add(at).cast::<u32>())};
    if word(0xa030)!=5||word(0xa034)!=0||word(0xa038)!=1||word(0xa03c)!=0||scalar_register(&partition,4)?!=cpu::STACK {
        return Err("Local IPI/timer handler/EOI/restored stack verification failed".into());
    }
    if word(0xa040)!=2 {return Err("Remote AP physical/logical IPI handlers did not run exactly twice".into());}
    let mut ap_stack=Register::scalar(0);let rsp=4;
    unsafe {check(WHvGetVirtualProcessorRegisters(partition.0,1,&rsp,1,&mut ap_stack),"AP restored stack")?;}
    if u64::from_le_bytes(ap_stack.0[..8].try_into().unwrap())!=0x7000 {return Err("AP interrupt stack was not restored".into());}
    let mut fabric=lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
    if word(0xa050)!=2 {return Err("IOAPIC held-level source did not deliver exactly two guest IRQs".into());}
    fabric.io.write(0,0x10)?;
    if fabric.io.read(0x10)?!=0x18042 {return Err("IOAPIC source not masked or remote IRR not cleared by guest EOI".into());}
    fabric.io.set_line(0,false)?;
    if word(0xa060)!=1||fabric.platform.pit0.edges!=1||fabric.pit_edges!=1 {return Err("Real PIT source failed IOAPIC edge delivery".into());}
    if word(0xa070)!=2||word(0xa074)!=2||fabric.platform.uart.irq_asserted()||fabric.platform.uart.output!=b"U" {return Err("Real UART source failed IOAPIC level delivery/device ACK".into());}
    fabric.io.write(0,0x18)?;
    if fabric.io.read(0x10)?!=0x8044 {return Err("UART IOAPIC remote IRR remains after device ACK and EOI".into());}
    for local in fabric.iter() {
        if local.pending().is_some()||(0x100..=0x170).chain(0x200..=0x270).filter(|offset|offset%16==0).any(|offset|local.read(offset)!=Ok(0)) {return Err("IPI remains pending/in-service after guest EOI".into());}
    }
    if fabric[0].read(0x390)?!=0||fabric[0].read(0x320)?!=0x30040 {return Err("Guest periodic timer was not masked and stopped".into());}
    for (index,base) in [(0,0xa010),(1,0xa020)] {
        if word(base)>>24!=index||(word(base)>>16)&255!=2||word(base+4)!=index||word(base+8)!=2 {
            return Err(format!("Guest CPU{index} topology disagrees with allocated vCPUs"));
        }
    }
    Ok(ap_halts)
}

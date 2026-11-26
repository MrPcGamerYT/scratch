// Included inside whp::windows. Experimental two-vCPU startup frontend.
// Guest INIT/SIPI commands drive architectural AP startup; markers are guest stores.
// Exercises fixed IPI routing, not the production Linux SMP scheduler or full LAPIC/IOAPIC.
fn registers_at(partition:&Partition,index:u32,names:&[u32],values:&[Register])->Result<(),String> {
    if index>=partition.2||names.len()!=values.len() {return Err("Invalid indexed register access".into());}
    unsafe {check(WHvSetVirtualProcessorRegisters(partition.0,index,names.as_ptr(),names.len() as u32,values.as_ptr()),"WHP indexed registers")}
}
struct ApicDisk {device:crate::virtio_mmio::BlockMmio,backend:crate::overlay::Overlay,accesses:u64,completions:u64}
type RamGate=std::sync::Arc<std::sync::RwLock<()>>;
struct ApicState {dma_failure:Option<crate::kernel::DmaFailure>,started_aps:Vec<bool>,ram_gate:RamGate,dma_rendezvous:u64,disk:Option<ApicDisk>,os:bool,stop:bool,startup_routes:Vec<Option<std::sync::mpsc::Sender<StartupEvent>>>,injections:u64,timer_injections:u64,io_accesses:u64,platform:crate::platform::EarlyPlatform,pit_edges:u64,epoch:std::time::Instant,cpus:Vec<crate::lapic::LocalApic>,io:crate::ioapic::IoApic}
impl std::ops::Deref for ApicState {type Target=[crate::lapic::LocalApic];fn deref(&self)->&Self::Target {&self.cpus}}
impl std::ops::DerefMut for ApicState {fn deref_mut(&mut self)->&mut Self::Target {&mut self.cpus}}
type ApicFabric=std::sync::Arc<std::sync::Mutex<ApicState>>;
#[derive(Clone,Copy)]
enum StartupEvent {Init,Sipi(u8)}
// Dropped inside the thread scope before scope's implicit joins on every exit.
struct StopApic(ApicFabric);
impl Drop for StopApic {fn drop(&mut self) {
    let mut state=self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner);state.stop=true;
}}
struct ApicContext {ram_gate:RamGate,timer_clock:std::time::Instant,partition:*mut c_void,index:u32,ram:*mut u8,bytes:usize,icr_high:u32,lapics:ApicFabric,error:Option<String>}
fn sync_cr8(c:&ApicContext,local:&mut crate::lapic::LocalApic)->Result<(),String> {
    let name=0x20;let mut value=Register::scalar(0);
    unsafe {check(WHvGetVirtualProcessorRegisters(c.partition,c.index,&name,1,&mut value),"APIC read CR8")?;}
    local.set_cr8(u64::from_le_bytes(value.0[..8].try_into().unwrap()))
}
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
        let _ram=c.ram_gate.read().map_err(|_|"RAM rendezvous poisoned")?;
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
            let _ram=c.ram_gate.read().map_err(|_|"RAM rendezvous poisoned")?;
            unsafe {std::ptr::copy_nonoverlapping(c.ram.add(a.gpa as usize),a.data.as_mut_ptr(),n);}return Ok(());
        }
        if (MMIO_BASE..MMIO_BASE+4096).contains(&a.gpa) {
            if ![1,2,4].contains(&a.size)||end>MMIO_BASE+4096 {return Err("Invalid APIC-profile block MMIO shape".into());}
            let mut state=c.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
            let disk=state.disk.as_mut().ok_or("Unconfigured APIC block device")?;
            let offset=a.gpa-MMIO_BASE;
            if a.direction==0 {a.data[..n].copy_from_slice(&disk.device.read(offset,a.size)?.to_le_bytes()[..n]);}
            else {
                let config=(0x100..0x200).contains(&offset)&&end<=MMIO_BASE+0x200;
                let readonly=[0,4,8,0xc,0x10,0x34,0x60,0xfc].contains(&offset)&&a.size==4&&offset%4==0;
                if !config&&!readonly {disk.device.write(offset,a.size,u32::from_le_bytes(a.data[..4].try_into().unwrap()))?;}
            }
            disk.accesses+=1;let irq=disk.device.irq_asserted();
            // Preserve ACK deassertion before another CPU can complete more work.
            // Sampling only at the next run-loop tick can lose an edge on ISA GSI5.
            state.io.set_line(5,irq)?;return Ok(());
        }
        if (0xfec00000..0xfec00020).contains(&a.gpa) {
            if a.size!=4 {return Err("IOAPIC requires 32-bit MMIO".into());}
            let mut state=c.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
            state.io_accesses+=1;
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
        if offset==0x80 {let mut state=c.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;sync_cr8(c,&mut state[c.index as usize])?;}
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
                if ![5,6].contains(&kind)||value&(1<<11)!=0 {return Err("Unsupported APIC startup delivery mode".into());}
                let event=if kind==5 {StartupEvent::Init} else {StartupEvent::Sipi(value as u8)};
                // Level-triggered INIT deassert is not a second CPU reset.
                if kind==5&&value&(1<<15)!=0&&value&(1<<14)==0 {return Ok(());}
                let shorthand=(value>>18)&3;
                {
                    let mut state=c.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
                    let targets:Vec<usize>=match shorthand {
                        0=>vec![(c.icr_high>>24) as usize],
                        3=>(0..state.len()).filter(|id|*id!=c.index as usize).collect(),
                        _=>return Err("Self/all startup reset unsupported".into()),
                    };
                    // Validate every destination before sending any event. Initial
                    // INIT is consumed by the target's worker before it reenters WHP.
                    for &target in &targets {
                        state.startup_routes.get(target).and_then(Option::as_ref).ok_or("Startup targets absent/BSP CPU")?;
                    }
                    for target in targets {
                        if kind==6&&state.started_aps.get(target)==Some(&true) {continue;} // duplicate SIPI is ignored architecturally
                        state.startup_routes[target].as_ref().ok_or("Missing startup route")?.send(event).map_err(|_|"OS AP startup worker disconnected")?;
                        state.started_aps[target]=kind==6;
                    }
                }
            } else {
                let mut state=c.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
                if offset==0xb0 {
                    let local=&mut state[c.index as usize];
                    let level=local.service_vector().filter(|v|local.in_service_level(*v));
                    local.eoi();if let Some(vector)=level {state.io.eoi(vector);}
                }
                else {
                    state[c.index as usize].write(offset,value)?;
                    if offset==0x80 {
                        let name=0x20;let cr8=Register::scalar(u64::from((value>>4)&15));
                        unsafe {check(WHvSetVirtualProcessorRegisters(c.partition,c.index,&name,1,&cr8),"APIC TPR to CR8")?;}
                    }
                }

            }
        }Ok(())
    })
}
fn dma_fault_after(failure:crate::kernel::DmaFailure)->bool {matches!(failure,crate::kernel::DmaFailure::ErrorAfter|crate::kernel::DmaFailure::PanicAfter)}
fn inject_dma_fault(failure:crate::kernel::DmaFailure,index:u32,count:u32,started:usize,completed:usize)->Result<(),String> {
    let before=!dma_fault_after(failure);
    eprintln!("NATIVE_DMA_FAILURE_INJECTED kind={failure:?} cpu={index} cpus={count} started_aps={started} exclusive=true before_service={before} completed={completed}");
    match failure {
        crate::kernel::DmaFailure::Error|crate::kernel::DmaFailure::ErrorAfter=>Err("INJECTED_DMA_ERROR".into()),
        crate::kernel::DmaFailure::Panic|crate::kernel::DmaFailure::PanicAfter=>panic!("INJECTED_DMA_PANIC"),
    }
}
type ApicObserver<'a>=Option<&'a mut dyn FnMut(&mut ApicState)->Result<bool,String>>;
fn run_index(handle:usize,index:u32,ram:usize,bytes:usize,lapics:ApicFabric,mut observe:ApicObserver<'_>,reset:Option<&std::sync::mpsc::Receiver<StartupEvent>>)->Result<(u32,u64),String> {
    let callbacks=Box::new(EmulatorCallbacks{size:std::mem::size_of::<EmulatorCallbacks>() as u32,reserved:0,io:apic_io,memory:apic_memory,get:apic_get,set:apic_set,translate:apic_translate});
    let mut emulator=std::ptr::null_mut();
    unsafe {check(WHvEmulatorCreateEmulator(&*callbacks,&mut emulator),"APIC create emulator")?;}
    if emulator.is_null() {return Err("APIC emulator returned null handle".into());}
    let emulator=Emulator{handle:emulator,_callbacks:callbacks};
    let ram_gate=lapics.lock().map_err(|_|"LAPIC lock poisoned")?.ram_gate.clone();
    let mut ctx=ApicContext{ram_gate,timer_clock:std::time::Instant::now(),partition:handle as *mut c_void,index,ram:ram as *mut u8,bytes,icr_high:0,lapics,error:None};
    let os=ctx.lapics.lock().map_err(|_|"LAPIC lock poisoned")?.os;
    let count=ctx.lapics.lock().map_err(|_|"LAPIC lock poisoned")?.len() as u32;
    let deadline=std::time::Instant::now()+std::time::Duration::from_secs(if os {30} else {5});
    let mut halted=false;let mut halt_waits=0;
    for _ in 0..500000 {
        if let Some(rx)=reset {
            match rx.try_recv() {
                Ok(StartupEvent::Init)=>return Ok((0x3000,halt_waits)),
                Ok(StartupEvent::Sipi(_))=>continue, // redundant SIPI while running
                Err(std::sync::mpsc::TryRecvError::Empty)=>{},
                Err(_)=>return Err("AP startup channel disconnected while running".into()),
            }
        }
        if std::time::Instant::now()>=deadline {
            let names=[0x10,0x11];let mut state=[Register::scalar(0);2];
            unsafe {WHvGetVirtualProcessorRegisters(ctx.partition,index,names.as_ptr(),2,state.as_mut_ptr());}
            return Err(format!("LAPIC CPU{index} run deadline exceeded; RIP=0x{:x} flags=0x{:x}",u64::from_le_bytes(state[0].0[..8].try_into().unwrap()),u64::from_le_bytes(state[1].0[..8].try_into().unwrap())));
        }
        {
        let mut fabric=ctx.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
        sync_cr8(&ctx,&mut fabric[index as usize])?;
        {
            if fabric.stop {return Ok((0,halt_waits));}
            if let Some(observer)=observe.as_mut() {if observer(&mut fabric)? {fabric.stop=true;return Ok((0,halt_waits));}}
            // Fault-only scheduling: exercise BSP failure before service and AP
            // failure after service. Other CPUs leave pending work for that VP.
            let fault_turn=fabric.dma_failure.is_none_or(|failure|index==if dma_fault_after(failure) {count-1} else {0});
            if fault_turn&&fabric.disk.as_ref().is_some_and(|disk|disk.device.work_pending()) {
                // Every vCPU holds a shared gate ONLY while inside WHP Run.
                // Scheduling cancellation releases those guards; this exclusive
                // guard prevents reentry and waits for all CPUs to be stopped.
                // Callbacks never hold a gate while acquiring the fabric lock.
                let _exclusive=ctx.ram_gate.write().map_err(|_|"RAM rendezvous poisoned")?;
                let fault=fabric.dma_failure.take();
                let started=fabric.started_aps.iter().filter(|started|**started).count();
                if let Some(failure)=fault.filter(|failure|!dma_fault_after(*failure)) {inject_dma_fault(failure,index,count,started,0)?;}
                let disk=fabric.disk.as_mut().ok_or("Missing pending block device")?;
                let ram=unsafe {std::slice::from_raw_parts_mut(ctx.ram,ctx.bytes)};
                let mut memory=crate::guest_memory::GuestMemory::new(vec![crate::guest_memory::RamRegion{gpa:0,bytes:ram,writable:true}])?;
                let completed=disk.device.service(&mut memory,&mut disk.backend,256)?.len();
                disk.completions+=completed as u64;
                if let Some(failure)=fault {
                    if completed!=0 {inject_dma_fault(failure,index,count,started,completed)?;}
                    fabric.dma_failure=Some(failure); // wait for a nonempty queue service
                }
                fabric.dma_rendezvous+=1;
            }
            let ApicState{cpus,io,platform,pit_edges,epoch,disk,..}=&mut *fabric;
            platform.tick(epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64);
            platform.route_ioapic(io,pit_edges,disk.as_ref().is_some_and(|d|d.device.irq_asserted()))?;io.service(cpus)?;
        }
        let local=&mut fabric[index as usize];
        let now=std::time::Instant::now();
        local.advance_ns(now.duration_since(ctx.timer_clock).as_nanos().min(u128::from(u64::MAX)) as u64)?;ctx.timer_clock=now;
        if let Some(vector)=local.pending() {
            let names=[0x11,0x80000001,0x80000000,0x80000002];let mut values=[Register::scalar(0);4];
            unsafe {check(WHvGetVirtualProcessorRegisters(ctx.partition,index,names.as_ptr(),4,values.as_mut_ptr()),"LAPIC interrupt window")?;}
            let scalar=|n:usize|u64::from_le_bytes(values[n].0[..8].try_into().unwrap());
            if scalar(0)&0x200!=0&&scalar(1)&1==0&&scalar(2)&1==0&&scalar(3)&1==0 {
                let name=0x80000000;let value=Register::scalar(1|(u64::from(vector)<<16));
                unsafe {check(WHvSetVirtualProcessorRegisters(ctx.partition,index,&name,1,&value),"LAPIC fixed interrupt injection")?;}
                let timer=local.read(0x320)? as u8==vector&&local.read(0x380)?!=0;
                local.acknowledge(vector)?;halted=false;fabric.injections+=1;if timer {fabric.timer_injections+=1;}
            }
        }
        }
        if halted {std::thread::sleep(std::time::Duration::from_millis(1));continue;}
        let mut exit=ExitContext([0;224]);
        {
            let _running=ctx.ram_gate.read().map_err(|_|"RAM rendezvous poisoned")?;
            unsafe {check(WHvRunVirtualProcessor(ctx.partition,index,exit.0.as_mut_ptr().cast(),224),"WHP indexed run")?;}
        }
        if handle_cpuid(ctx.partition,index,count,true,&exit)? {continue;}
        if exit.reason()==0x2001 {continue;}
        if exit.reason()==0x1000 {
            let word=|at|u32::from_le_bytes(exit.0[at..at+4].try_into().unwrap());
            let msr=word(52);let write=word(48)&1!=0;
            let invariant_hwcr=msr==0xc0010015&&!write;
            if msr!=0x1b&&!invariant_hwcr {
                // Unsupported RDMSR/WRMSR architecturally faults at the current RIP.
                // Linux rdmsr_safe probes rely on #GP fixups, not invented values.
                let name=0x80000002;let value=Register::scalar(1|(1<<8)|(13<<16));
                unsafe {check(WHvSetVirtualProcessorRegisters(ctx.partition,index,&name,1,&value),"Unsupported MSR #GP(0)")?;}
                continue;
            }
            let base=if invariant_hwcr {1<<24} else {0xfee00800u64|if index==0 {0x100} else {0}};
            if write {
                let value=u64::from(word(56))|(u64::from(word(64))<<32);
                if value!=base {return Err(format!("APIC base relocation/disable unsupported: 0x{value:x}"));}
            } else {
                let names=[0,2];let values=[Register::scalar(base),Register::scalar(0)];
                unsafe {check(WHvSetVirtualProcessorRegisters(ctx.partition,index,names.as_ptr(),2,values.as_ptr()),"APIC base MSR read")?;}
            }
            let mode=crate::cpu::CpuMode::from_exit(&exit.0)?;
            let rip=u64::from_le_bytes(exit.0[32..40].try_into().unwrap());let rip=Register::scalar(mode.next_rip(rip,exit.0[10]&15)?);let name=0x10;
            unsafe {check(WHvSetVirtualProcessorRegisters(ctx.partition,index,&name,1,&rip),"APIC base MSR advance")?;}
            continue;
        }
        if exit.reason()==2 {
            let io=crate::io::IoExit::decode(&exit.0)?;
            let mode=crate::cpu::CpuMode::from_exit(&exit.0)?;
            let response={
                let mut state=ctx.lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
                let now=state.epoch.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
                let response=state.platform.access(io,now)?;
                let block_irq=state.disk.as_ref().is_some_and(|d|d.device.irq_asserted());
                let ApicState{platform,io,pit_edges,..}=&mut *state;
                platform.route_ioapic(io,pit_edges,block_irq)?;
                response
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
    priority_cycle()?;
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
    let tables=crate::firmware::mp_tables(2)?;
    f.ram[crate::firmware::MP_BASE..crate::firmware::MP_BASE+tables.len()].copy_from_slice(&tables);
    // BSP enables its APIC, targets APIC ID1, issues INIT followed by SIPI vector9.
    // The dormant AP receives its architectural CS:IP only after the guest SIPI.
    // Guest reads and parses MP discovery data before executing its CPUID/startup path.
    let mut code=vec![0xbe,0x00,0x00,0x0f,0x00,
        0x81,0x3e,0x5f,0x4d,0x50,0x5f,0x74,13,
        0x83,0xc6,16,0x81,0xfe,0,0,0x10,0,0x75,0xed,0xfa,0xf4,
        0x8b,0x06,0xa3,0x9c,0xa0,0x00,0x00,0x8b,0x7e,0x04,0x89,0xf8,0xa3,0x84,0xa0,0x00,0x00,0x31,0xc0,0xb9,0x10,0x00,0x00,0x00,0x02,0x06,0x46,0xe2,0xfb,0xa3,0x80,0xa0,0x00,0x00,0x89,0xfe,0x8b,0x06,0xa3,0xa0,0xa0,0x00,0x00,0x0f,0xb7,0x4e,0x04,0x31,0xc0,0x02,0x06,0x46,0xe2,0xfb,0xa3,0x88,0xa0,0x00,0x00,0x0f,0xb7,0x4f,0x22,0x8d,0x77,0x2c,0x31,0xd2,0x31,0xdb,0x80,0x3e,0x00,0x75,0x13,0xf6,0x46,0x03,0x01,0x74,0x08,0x42,0x0f,0xb6,0x46,0x01,0x0f,0xab,0xc3,0x83,0xc6,0x14,0xeb,0x10,0x80,0x3e,0x02,0x75,0x08,0x8b,0x46,0x04,0xa3,0x98,0xa0,0x00,0x00,0x83,0xc6,0x08,0xe2,0xd6,0x89,0x15,0x8c,0xa0,0x00,0x00,0x89,0x1d,0x90,0xa0,0x00,0x00,0x8b,0x47,0x24,0xa3,0x94,0xa0,0x00,0x00];
    // Verify the IOAPIC discovered in the MP table is the actual MMIO device.
    code.extend_from_slice(&[0x8b,0x1d,0x98,0xa0,0,0,
        0xc7,3,0,0,0,0,0x8b,0x43,0x10,0xc1,0xe8,24,0xa3,0xa4,0xa0,0,0,
        0xc7,3,1,0,0,0,0x8b,0x43,0x10,0xa3,0xa8,0xa0,0,0,
        0x8b,0x1d,0x94,0xa0,0,0,0x8b,0x43,0x20,0xa3,0xac,0xa0,0,0]);
    code.extend_from_slice(&[0xb8,1,0,0,0,0x31,0xc9,0x0f,0xa2,0x89,0x1d,0x10,0xa0,0,0,
        0xb8,0xb,0,0,0,0xb9,1,0,0,0,0x0f,0xa2,0x89,0x15,0x14,0xa0,0,0,0x89,0x1d,0x18,0xa0,0,0]);
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
    code.truncate(code.len()-2); // replace final CLI/HLT with runtime reset tests
    for generation in 2..=4u8 {
        code.extend_from_slice(&[0xc7,0x05,0x10,3,0xe0,0xfe,0,0,0,1]);
        // Assert INIT, deassert it (not another reset), SIPI twice.
        for command in [0xc500u32,0x8500,0x4609,0x4609] {
            code.extend_from_slice(&[0xc7,0x05,0,3,0xe0,0xfe]);code.extend_from_slice(&command.to_le_bytes());
        }
        code.extend_from_slice(&[0x83,0x3d,0xc4,0xa0,0,0,generation,0x75,0xf7]);
    }
    code.extend_from_slice(&[0xfa,0xf4]);
    // Unknown MSR read/write must fault, preserve registers, and return via #GP fixup.
    let mut msr_probe=vec![0xb8,0x34,0x12,0,0,0xba,0x78,0x56,0,0,0xb9,0xef,0xbe,0xad,0xde,
        0x0f,0x32,0x0f,0x30,0xa3,0xb8,0xa0,0,0,0x89,0x15,0xbc,0xa0,0,0];
    msr_probe.extend_from_slice(&code);code=msr_probe;
    f.ram[cpu::IDT+0x68..cpu::IDT+0x70].copy_from_slice(&[0,0xba,8,0,0,0x8e,0,0]);
    let gp_handler=[0xff,0x05,0xb0,0xa0,0,0,0x50,0x8b,0x44,0x24,4,0xa3,0xb4,0xa0,0,0,0x58,
        0x83,0x44,0x24,4,2,0x83,0xc4,4,0xcf];
    f.ram[0xba00..0xba00+gp_handler.len()].copy_from_slice(&gp_handler);
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
    let mut ap_protected=vec![0x66,0xb8,16,0,0x8e,0xd8,0x8e,0xd0,0xbc,0,0x70,0,0,
        0x0f,0x01,0x1d,0,0x53,0,0,
        0xc7,0x05,0xf0,0,0xe0,0xfe,0xff,1,0,0,
        0xc7,0x05,0xd0,0,0xe0,0xfe,0,0,0,1,0xfb,0x90,
        0xc6,0x05,0,0xa0,0,0,0x5a,
        0xfa,0x83,0x3d,0x40,0xa0,0,0,2,0x74,4,0xfb,0xf4,0xeb,0xf2,0xfa,0xf4];
    // After segment/stack/IDT setup, before enabling the reset LAPIC, check
    // defaults and aggregate any stale TPR/SVR bits over every restart.
    let probe=[0xa1,0x30,2,0xe0,0xfe,0x09,0x05,0xc8,0xa0,0,0,
        0xa1,0x30,1,0xe0,0xfe,0x09,0x05,0xc8,0xa0,0,0,
        0xa1,0x80,0,0xe0,0xfe,0x09,0x05,0xc8,0xa0,0,0,
        0xa1,0xf0,0,0xe0,0xfe,0x35,0xff,0,0,0,0x09,0x05,0xc8,0xa0,0,0,
        0xff,0x05,0xc0,0xa0,0,0];
    ap_protected.splice(20..20,probe);
    // Dirty the LAPIC before each final halt; INIT must discard this TPR.
    ap_protected.truncate(ap_protected.len()-2);
    ap_protected.extend_from_slice(&[0xc7,0x05,0x80,0,0xe0,0xfe,0xf0,0,0,0,
        0xc7,0x05,0,3,0xe0,0xfe,0x60,0,4,0, // pending masked self-IPI must not survive INIT
        0xff,0x05,0xc4,0xa0,0,0,
        0x83,0x3d,0xc0,0xa0,0,0,4,0x74,2,0xeb,0xfe, // active IF=0 spin until guest INIT; final generation halts
        0xfa,0xf4]);
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
    let lapics=std::sync::Arc::new(std::sync::Mutex::new(ApicState{dma_failure:None,started_aps:Vec::new(),ram_gate:std::sync::Arc::new(std::sync::RwLock::new(())),dma_rendezvous:0,disk:None,os:false,stop:false,startup_routes:Vec::new(),injections:0,timer_injections:0,io_accesses:0,platform:crate::platform::EarlyPlatform::default(),pit_edges:0,epoch:std::time::Instant::now(),cpus:vec![crate::lapic::LocalApic::new(0),crate::lapic::LocalApic::new(1)],io:crate::ioapic::IoApic::new()}));
    {let mut state=lapics.lock().map_err(|_|"LAPIC lock poisoned")?;
        state.io.write(0,0)?;state.io.write(0x10,u32::from(crate::firmware::IOAPIC_ID)<<24)?;}
    // Test-only device source: hold IOAPIC input 0 high before guest unmasks it.
    // It must retrigger only after guest EOI, and stop when the handler masks it.
    lapics.lock().map_err(|_|"LAPIC lock poisoned")?.io.set_line(0,true)?;
    let ((bsp,ap,ap_halts),expired)=timer_pump(&partition,||bounded(&partition,5000,||std::thread::scope(|scope| {
        let _stop=StopApic(lapics.clone());
        let (tx,rx)=std::sync::mpsc::channel();
        {let mut state=lapics.lock().map_err(|_|"LAPIC lock poisoned")?;state.startup_routes=vec![None,Some(tx)];state.started_aps=vec![false;2];}
        let ap_lapics=lapics.clone();
        let ap=scope.spawn(move ||ap_worker(handle,1,ram,bytes,rx,ap_lapics));
        let bsp=run_index(handle,0,ram,bytes,lapics.clone(),None,None);
        lapics.lock().map_err(|_|"LAPIC lock poisoned")?.stop=true;
        let ap_halts=ap.join().map_err(|_|"AP run thread panicked")??;
        Ok((bsp?.0,8,ap_halts))
    })))?;
    if expired||bsp!=8||ap!=8 {return Err(format!("SMP INIT/SIPI failed: BSP exit={bsp:x} AP exit={ap:x} timeout={expired}"));}
    // Both vCPUs have stopped and workers joined before accessing shared RAM.
    let marker=unsafe {*memory.address.cast::<u8>().add(0xa000)};
    if marker!=0x5a {return Err("AP startup marker missing".into());}
    let word=|at:usize|unsafe {std::ptr::read_unaligned(memory.address.cast::<u8>().add(at).cast::<u32>())};
    if word(0xa0b0)!=2||word(0xa0b4)!=0||word(0xa0b8)!=0x1234||word(0xa0bc)!=0x5678 {return Err("Unsupported RDMSR/WRMSR #GP recovery/preserved register verification failed".into());}
    if word(0xa080)!=0||word(0xa084)!=crate::firmware::CONFIG_BASE as u32||word(0xa088)!=0||word(0xa08c)!=2||word(0xa090)!=3||word(0xa094)!=crate::firmware::LAPIC_ADDRESS||word(0xa098)!=crate::firmware::IOAPIC_ADDRESS||word(0xa09c)!=u32::from_le_bytes(*b"_MP_")||word(0xa0a0)!=u32::from_le_bytes(*b"PCMP") {
        return Err("Guest MP table checksum/topology/controller enumeration mismatch".into());
    }
    if word(0xa0a4)!=u32::from(crate::firmware::IOAPIC_ID)||word(0xa0a8)!=0x170011||word(0xa0ac)!=0 {return Err("MP-discovered controller identity disagrees with actual MMIO device".into());}
    if word(0xa030)!=5||word(0xa034)!=0||word(0xa038)!=1||word(0xa03c)!=0||scalar_register(&partition,4)?!=cpu::STACK {
        return Err("Local IPI/timer handler/EOI/restored stack verification failed".into());
    }
    if word(0xa0c0)!=4||word(0xa0c4)!=4||word(0xa0c8)!=0 {return Err(format!("Runtime AP reset defaults/restart mismatch starts={} stops={} stale={:x}",word(0xa0c0),word(0xa0c4),word(0xa0c8)));}
    println!("NATIVE_AP_RESET_OK restarts=3 starts=4 stops=4 stale_lapic=0 duplicate_sipi_ignored=true active_reset=true");
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

fn run_apic_os(partition:&Partition,memory:&Mapping<'_>,serial:&mut dyn std::io::Write,trace:&mut crate::timing::BootTrace,start:std::time::Instant,devices:(Option<crate::overlay::Overlay>,bool,Option<crate::kernel::DmaFailure>))->Result<(),String> {
    let (backend,bridge_probe,dma_failure)=devices;
    let mut io=crate::ioapic::IoApic::new();io.write(0,0)?;io.write(0x10,u32::from(crate::firmware::IOAPIC_ID)<<24)?;
    let fabric=std::sync::Arc::new(std::sync::Mutex::new(ApicState{dma_failure:None,started_aps:Vec::new(),ram_gate:std::sync::Arc::new(std::sync::RwLock::new(())),dma_rendezvous:0,disk:None,os:true,stop:false,startup_routes:Vec::new(),injections:0,timer_injections:0,io_accesses:0,platform:crate::platform::EarlyPlatform::default(),pit_edges:0,epoch:std::time::Instant::now(),cpus:(0..partition.2).map(|id|crate::lapic::LocalApic::new(id as u8)).collect(),io}));
    fabric.lock().map_err(|_|"LAPIC lock poisoned")?.dma_failure=dma_failure;
    let live=std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    struct WorkerCount(std::sync::Arc<std::sync::atomic::AtomicUsize>);
    impl Drop for WorkerCount {fn drop(&mut self) {self.0.fetch_sub(1,std::sync::atomic::Ordering::SeqCst);}}
    struct CleanupEvidence {enabled:bool,live:std::sync::Arc<std::sync::atomic::AtomicUsize>,gate:RamGate}
    impl Drop for CleanupEvidence {fn drop(&mut self) {if self.enabled {
        let released=!matches!(self.gate.try_write(),Err(std::sync::TryLockError::WouldBlock));
        eprintln!("NATIVE_APIC_CLEANUP ap_workers={} ram_gate_released={released}",self.live.load(std::sync::atomic::Ordering::SeqCst));
    }}}
    let _cleanup=CleanupEvidence{enabled:dma_failure.is_some(),live:live.clone(),gate:fabric.lock().map_err(|_|"LAPIC lock poisoned")?.ram_gate.clone()};
    if let Some(backend)=backend {
        let device=crate::virtio_mmio::BlockMmio::new(backend.disk_bytes()/512,false)?;
        fabric.lock().map_err(|_|"LAPIC lock poisoned")?.disk=Some(ApicDisk{device,backend,accesses:0,completions:0});
    }
    if bridge_probe {fabric.lock().map_err(|_|"LAPIC lock poisoned")?.platform.bridge=Some(crate::bridge::Device::default());}
    let mut emitted=0;
    trace.vm_ready_ms=Some(start.elapsed().as_millis() as u64);trace.vcpu_run_requested_ms=trace.vm_ready_ms;
    let mut observe=|state:&mut ApicState|->Result<bool,String> {
        let bytes=&state.platform.uart.output[emitted..];
        serial.write_all(bytes).map_err(|e|e.to_string())?;serial.flush().map_err(|e|e.to_string())?;
        trace.observe_serial(bytes,start.elapsed().as_millis() as u64);emitted=state.platform.uart.output.len();
        if let Some(bridge)=&mut state.platform.bridge {bridge.tick()?;}
        Ok(trace.linux_userspace_ready_ms.is_some()&&state.platform.bridge.as_ref().is_none_or(|b|b.client.complete()))
    };
    let handle=partition.0 as usize;let ram=memory.address as usize;let bytes=memory.bytes;
    let result=timer_pump(partition,||bounded(partition,30000,||std::thread::scope(|scope| {
        let _stop=StopApic(fabric.clone());
        let mut workers=Vec::new();let mut routes=vec![None];
        for index in 1..partition.2 {
            registers_at(partition,index,&[0x1c,0x1e,0x1f,0x2001,0x13,0x15,0x12,0x14,0x10,0x11,0x1a],&[
                Register::scalar(0x60000010),Register::scalar(0),Register::scalar(0),Register::scalar(0),
                Register::segment(0x9b),Register::segment(0x93),Register::segment(0x93),Register::segment(0x93),
                Register::scalar(0),Register::scalar(2),Register::table(0,0x3ff)])?;
            let (tx,rx)=std::sync::mpsc::channel();routes.push(Some(tx));let state=fabric.clone();
            let live=live.clone();
            workers.push(scope.spawn(move || {
                live.fetch_add(1,std::sync::atomic::Ordering::SeqCst);let _count=WorkerCount(live);
                let result=ap_worker(handle,index,ram,bytes,rx,state.clone()).map(|_|());
                if result.is_err() {if let Ok(mut state)=state.lock() {state.stop=true;}}
                result
            }));
        }
        {let mut state=fabric.lock().map_err(|_|"LAPIC lock poisoned")?;
            state.startup_routes=routes;state.started_aps=vec![false;partition.2 as usize];}
        let bsp=run_index(handle,0,ram,bytes,fabric.clone(),Some(&mut observe),None);
        fabric.lock().map_err(|_|"LAPIC lock poisoned")?.stop=true;
        for worker in workers {worker.join().map_err(|_|"Linux AP worker panicked")??;}
        bsp
    })));
    let state=fabric.lock().map_err(|_|"LAPIC lock poisoned")?;
    observe_serial_tail(&state.platform.uart.output,emitted,serial,trace,start)?;
    let ((reason,waits),expired)=result.map_err(|error|format!("{error}; APIC_DIAG injections={} pit_programs={} pit_edges={} cursor={} SVR={:x?} TPR={:x?} LDR={:x?} pending={:?}",state.injections,state.platform.pit0.programs,state.platform.pit0.edges,state.pit_edges,state[0].read(0xf0),state[0].read(0x80),state[0].read(0xd0),state[0].pending()))?;trace.halt_waits=waits;
    if trace.linux_userspace_ready_ms.is_none()||expired||reason!=0||state.io_accesses==0||state.injections==0||state.timer_injections==0 {return Err(format!("APIC OS checkpoint incomplete reason=0x{reason:x} expired={expired} io_mmio={} injections={}",state.io_accesses,state.injections));}
    if let Some(bridge)=&state.platform.bridge {
        if !bridge.client.complete()||state.io.delivery_count(3)?==0 {return Err("APIC COM2 query/interrupt incomplete".into());}
        println!("NATIVE_APIC_BRIDGE_OK app={} ioapic_gsi3_deliveries={}",bridge.client.app().unwrap_or("-"),state.io.delivery_count(3)?);
    }
    if let Some(disk)=&state.disk {
        if disk.completions==0||disk.accesses==0||state.io.delivery_count(5)?==0 {return Err("APIC disk checkpoint completed without real block work".into());}
        println!("NATIVE_APIC_DISK_OK completions={} mmio_accesses={} ioapic_gsi5_deliveries={} dma_rendezvous={}",disk.completions,disk.accesses,state.io.delivery_count(5)?,state.dma_rendezvous);
    }
    println!("NATIVE_APIC_OS_PID1_OK cpus={} ram_bytes={} io_mmio={} injections={} lapic_timer_irqs={} pit_edges={}; not Android certification",partition.2,memory.bytes,state.io_accesses,state.injections,state.timer_injections,state.platform.pit0.edges);
    Ok(())
}
fn observe_serial_tail(output:&[u8],emitted:usize,serial:&mut dyn std::io::Write,trace:&mut crate::timing::BootTrace,start:std::time::Instant)->Result<(),String> {
    serial.write_all(&output[emitted..]).map_err(|e|e.to_string())?;serial.flush().map_err(|e|e.to_string())?;
    trace.observe_serial(&output[emitted..],start.elapsed().as_millis() as u64);Ok(())
}

// Actual long-mode MOV CR8 / xAPIC TPR alias and deferred self-IPI acceptance.
fn priority_cycle()->Result<(),String> {
    let partition=create_machine(1,true)?;
    let memory=allocate(&partition,crate::cpu::RAM_BYTES)?;
    unsafe {check(WHvCreateVirtualProcessor(partition.0,0,0),"Create priority vCPU")?;}
    let _processor=Processor(&partition,0);
    let mut f=crate::cpu::fixture(crate::cpu::CpuMode::Long64)?;
    // Map the LAPIC's high MMIO page (2MiB mapping); WHP handles unmapped GPA.
    f.ram[0x2018..0x2020].copy_from_slice(&0x7003u64.to_le_bytes());
    f.ram[0x7fb8..0x7fc0].copy_from_slice(&0xfee00083u64.to_le_bytes());
    let code=[0xbb,0x0,0x0,0xe0,0xfe,0xc7,0x83,0xf0,0x0,0x0,0x0,0xff,0x1,0x0,0x0,0xc7,0x83,0x80,0x0,0x0,0x0,0x7,0x0,0x0,0x0,0x48,0xc7,0xc0,0x4,0x0,0x0,0x0,0x44,0xf,0x22,0xc0,0xfb,0x90,0xc7,0x83,0x0,0x3,0x0,0x0,0x40,0x0,0x4,0x0,0x8b,0x83,0x80,0x0,0x0,0x0,0x89,0x4,0x25,0x4,0xa0,0x0,0x0,0x8b,0x83,0x20,0x2,0x0,0x0,0x89,0x4,0x25,0x8,0xa0,0x0,0x0,0x8b,0x4,0x25,0x0,0xa0,0x0,0x0,0x89,0x4,0x25,0xc,0xa0,0x0,0x0,0x48,0xc7,0xc0,0x3,0x0,0x0,0x0,0x44,0xf,0x22,0xc0,0x83,0x3c,0x25,0x0,0xa0,0x0,0x0,0x1,0x75,0xf6,0xc7,0x83,0x80,0x0,0x0,0x0,0x5b,0x0,0x0,0x0,0x44,0xf,0x20,0xc0,0x89,0x4,0x25,0x10,0xa0,0x0,0x0,0xfa,0xf4];
    f.ram[crate::cpu::ENTRY..crate::cpu::ENTRY+code.len()].copy_from_slice(&code);
    let handler=[0xff,0x04,0x25,0,0xa0,0,0,0xc7,0x83,0xb0,0,0,0,0,0,0,0,0x48,0xcf];
    f.ram[0x9000..0x9000+handler.len()].copy_from_slice(&handler);
    let gate=[0,0x90,8,0,0,0x8e,0,0,0,0,0,0,0,0,0,0];
    f.ram[crate::cpu::IDT+0x400..crate::cpu::IDT+0x410].copy_from_slice(&gate);
    unsafe {std::ptr::copy_nonoverlapping(f.ram.as_ptr(),memory.address.cast::<u8>(),f.ram.len());}
    configure_fixture(&partition,&f)?;
    let fabric=std::sync::Arc::new(std::sync::Mutex::new(ApicState{dma_failure:None,started_aps:Vec::new(),ram_gate:std::sync::Arc::new(std::sync::RwLock::new(())),dma_rendezvous:0,disk:None,os:false,stop:false,startup_routes:Vec::new(),injections:0,timer_injections:0,io_accesses:0,platform:crate::platform::EarlyPlatform::default(),pit_edges:0,epoch:std::time::Instant::now(),cpus:vec![crate::lapic::LocalApic::new(0)],io:crate::ioapic::IoApic::new()}));
    let ((reason,_),expired)=timer_pump(&partition,||bounded(&partition,5000,||run_index(partition.0 as usize,0,memory.address as usize,memory.bytes,fabric,None,None)))?;
    let mut evidence=[0u32;5];
    unsafe {std::ptr::copy_nonoverlapping(memory.address.cast::<u8>().add(0xa000).cast::<u32>(),evidence.as_mut_ptr(),5);}
    if expired||reason!=8||evidence!=[1,0x47,1,0,5] {return Err(format!("CR8/TPR deferred IPI failed: reason={reason} expired={expired} evidence={evidence:x?}"));}
    println!("NATIVE_CR8_TPR_OK masked_irr=1 premature_delivery=0 delivered_after_lower=1 tpr_read=47 cr8_read=5");Ok(())
}

fn ap_worker(handle:usize,index:u32,ram:usize,bytes:usize,rx:std::sync::mpsc::Receiver<StartupEvent>,state:ApicFabric)->Result<u64,String> {
    let mut initialized=false;let mut reset_pending=false;let mut waits=0;
    loop {
        if state.lock().map_err(|_|"LAPIC lock poisoned")?.stop {return Ok(waits);}
        let event=if reset_pending {reset_pending=false;StartupEvent::Init} else {
            match rx.recv_timeout(std::time::Duration::from_millis(10)) {
                Ok(event)=>event,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)=>continue,
                Err(_)=>return Err("AP startup channel disconnected".into()),
            }
        };
        match event {
            StartupEvent::Init=>{
                // No WHvRun is active on this VP. Recreate it to discard stale
                // architectural, pending-event and translation state together.
                let mut fabric=state.lock().map_err(|_|"LAPIC lock poisoned")?;
                unsafe {
                    check(WHvDeleteVirtualProcessor(handle as *mut c_void,index),"INIT delete inactive VP")?;
                    check(WHvCreateVirtualProcessor(handle as *mut c_void,index,0),"INIT recreate VP")?;
                }
                fabric[index as usize]=crate::lapic::LocalApic::new(index as u8);
                let names=[0x1c,0x1e,0x1f,0x2001,0x20,0x13,0x15,0x12,0x14,0x10,0x11,0x1a];
                let values=[Register::scalar(0x60000010),Register::scalar(0),Register::scalar(0),Register::scalar(0),Register::scalar(0),
                    Register::segment(0x9b),Register::segment(0x93),Register::segment(0x93),Register::segment(0x93),
                    Register::scalar(0),Register::scalar(2),Register::table(0,0x3ff)];
                unsafe {check(WHvSetVirtualProcessorRegisters(handle as *mut c_void,index,names.as_ptr(),names.len() as u32,values.as_ptr()),"INIT reset entry")?;}
                initialized=true;
            }
            StartupEvent::Sipi(vector) if initialized=>{
                let mut cs=Register::segment(0x9b);
                cs.0[..8].copy_from_slice(&(u64::from(vector)<<12).to_le_bytes());
                cs.0[12..14].copy_from_slice(&(u16::from(vector)<<8).to_le_bytes());
                let names=[0x13,0x10];let values=[cs,Register::scalar(0)];
                unsafe {check(WHvSetVirtualProcessorRegisters(handle as *mut c_void,index,names.as_ptr(),2,values.as_ptr()),"Guest SIPI restart entry")?;}
                initialized=false;
                let (reason,halt_waits)=run_index(handle,index,ram,bytes,state.clone(),None,Some(&rx))?;waits+=halt_waits;
                match reason {
                    0=>return Ok(waits),
                    0x3000=>reset_pending=true,
                    8 if !state.lock().map_err(|_|"LAPIC lock poisoned")?.os=>{},
                    _=>return Err(format!("AP CPU{index} unexpected exit 0x{reason:x}")),
                }
            }
            StartupEvent::Sipi(_)=>{}, // SIPIs outside wait-for-SIPI are ignored
        }
    }
}

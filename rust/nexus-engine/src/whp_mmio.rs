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
struct MmioContext {
    partition:*mut c_void,ram:*mut u8,bytes:usize,device:crate::virtio_mmio::BlockMmio,
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
        if a.gpa<MMIO_BASE||end>MMIO_BASE+4096 {return Err("Emulator memory access outside RAM/device page".into());}
        if ![1,2,4].contains(&a.size) {return Err("Unsupported MMIO register access width".into());}
        let offset=a.gpa-MMIO_BASE;
        if a.direction==0 {let value=ctx.device.read(offset,a.size)?.to_le_bytes();a.data[..n].copy_from_slice(&value[..n]);}
        else {
            // Defined RO registers/config ignore writes, without changing device state.
            let config=(0x100..0x200).contains(&offset)&&end<=MMIO_BASE+0x200&&[1,2,4].contains(&a.size);
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
impl Drop for Emulator {fn drop(&mut self) {unsafe {WHvEmulatorDestroyEmulator(self.handle);}}}

pub fn mmio_smoke()->Result<(),String> {
    use crate::cpu;
    for long in [false,true] {
        let mode=if long {cpu::CpuMode::Long64} else {cpu::CpuMode::Protected32};let mut f=cpu::fixture(mode)?;
        // Guest VA/GPA d0000000 mapped in guest page tables, but absent from host GPA mapping.
        if long {
            f.ram[0x2018..0x2020].copy_from_slice(&0x7003u64.to_le_bytes());
            f.ram[0x7400..0x7408].copy_from_slice(&(MMIO_BASE|0x83).to_le_bytes());
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
        load(&mut code,0x70,4,0x1b01c);load(&mut code,0x108,1,0x1b020);write(&mut code,0x70,0);load(&mut code,0x70,4,0x1b024);code.push(0xf4);
        // Execute long-mode code through the nonidentity VA 400000 -> PA 18000 alias.
        let physical=if long {cpu::DATA} else {cpu::ENTRY};f.ram[physical..physical+code.len()].copy_from_slice(&code);
        with_guest_at(&[],0,cpu::RAM_BYTES,|partition,memory| {
            unsafe {std::ptr::copy_nonoverlapping(f.ram.as_ptr(),memory.address.cast::<u8>(),f.ram.len());}
            configure_fixture(partition,&f)?;if long {set_registers(partition,&[0x10],&[Register::scalar(cpu::ALIAS)])?;}
            let emulator=Emulator::new()?;let mut ctx=MmioContext{partition:partition.0,ram:memory.address.cast(),bytes:memory.bytes,device:crate::virtio_mmio::BlockMmio::new(4,false)?,error:None,translations:0,accesses:0};
            let (result,timed_out)=bounded(partition,5000,|| {
                for _ in 0..128 {
                    let exit=run_once(partition)?;
                    match exit.reason() {8=>return Ok(Ok(())),1=>emulator.emulate(&mut ctx,&exit,long)?,reason=>return Err(format!("Unexpected MMIO fixture exit 0x{reason:x}"))}
                }Ok(Err("MMIO fixture exit budget exceeded".to_string()))
            })?;
            if timed_out {return Err("MMIO fixture deadline exceeded".into());}result?;
            let bytes=unsafe {std::slice::from_raw_parts(memory.address.cast::<u8>().add(0x1b000),40)};
            for (n,expected) in [0x74726976,2,2,518,131072,1,0x74726976,11,0,0].into_iter().enumerate() {
                let actual=u32::from_le_bytes(bytes[n*4..n*4+4].try_into().unwrap());if actual!=expected {return Err(format!("MMIO register {n} mismatch 0x{actual:x}"));}
            }
            if ctx.accesses<10||(long&&ctx.translations==0) {return Err("Missing real MMIO/translation callback execution".into());}Ok(())
        })?;
    }Ok(())
}

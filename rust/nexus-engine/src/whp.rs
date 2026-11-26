//! Direct Windows Hypervisor Platform API — no QEMU dependency.
//! Bounded real-mode CPU/RAM, private port I/O and cancellation tests. No Android machine.
#[cfg(windows)]
mod windows {
    use std::ffi::c_void;
    #[link(name = "WinHvPlatform")]
    extern "system" {
        fn WHvGetCapability(code:u32, buffer:*mut c_void, size:u32, written:*mut u32)->i32;
        fn WHvCreatePartition(partition:*mut *mut c_void)->i32;
        fn WHvSetPartitionProperty(partition:*mut c_void, code:u32, property:*const c_void, size:u32)->i32;
        fn WHvSetupPartition(partition:*mut c_void)->i32;
        fn WHvDeletePartition(partition:*mut c_void)->i32;
        fn WHvMapGpaRange(partition:*mut c_void, source:*mut c_void, guest:u64, size:u64, flags:u32)->i32;
        fn WHvUnmapGpaRange(partition:*mut c_void, guest:u64, size:u64)->i32;
        fn WHvCreateVirtualProcessor(partition:*mut c_void,index:u32,flags:u32)->i32;
        fn WHvDeleteVirtualProcessor(partition:*mut c_void,index:u32)->i32;
        fn WHvSetVirtualProcessorRegisters(partition:*mut c_void,index:u32,names:*const u32,count:u32,values:*const Register)->i32;
        fn WHvGetVirtualProcessorRegisters(partition:*mut c_void,index:u32,names:*const u32,count:u32,values:*mut Register)->i32;
        fn WHvRunVirtualProcessor(partition:*mut c_void,index:u32,context:*mut c_void,size:u32)->i32;
        fn WHvCancelRunVirtualProcessor(partition:*mut c_void,index:u32,flags:u32)->i32;
    }
    #[link(name="kernel32")]
    extern "system" {
        fn VirtualAlloc(address:*mut c_void,size:usize,kind:u32,protection:u32)->*mut c_void;
        fn VirtualFree(address:*mut c_void,size:usize,kind:u32)->i32;
    }
    // Windows SDK WHV_REGISTER_VALUE and WHV_RUN_VP_EXIT_CONTEXT (AMD64).
    #[repr(C,align(16))]
    #[derive(Clone,Copy)]
    struct Register([u8;16]);
    impl Register {
        fn scalar(value:u64)->Self { let mut r=Self([0;16]);r.0[..8].copy_from_slice(&value.to_le_bytes());r }
        fn segment(attributes:u16)->Self { let mut r=Self([0;16]);r.0[8..12].copy_from_slice(&0xffffu32.to_le_bytes());r.0[14..16].copy_from_slice(&attributes.to_le_bytes());r }
    }
    #[repr(C,align(16))]
    struct ExitContext([u8;224]);
    struct Mapping<'a> { partition:&'a Partition, address:*mut c_void, bytes:usize }
    impl Drop for Mapping<'_> { fn drop(&mut self) { unsafe { WHvUnmapGpaRange(self.partition.0,0,self.bytes as u64); VirtualFree(self.address,0,0x8000); } } }
    struct Processor<'a>(&'a Partition);
    impl Drop for Processor<'_> { fn drop(&mut self) { unsafe { WHvDeleteVirtualProcessor(self.0.0,0); } } }
    fn allocate(partition:&Partition,bytes:usize)->Result<Mapping<'_>,String> {
        if bytes==0 || bytes%4096!=0 || bytes>1024*1024 {return Err("Invalid test RAM mapping size".into());}
        let address=unsafe { VirtualAlloc(std::ptr::null_mut(),bytes,0x3000,4) };
        if address.is_null() { return Err("VirtualAlloc failed".into()); }
        let hr=unsafe { WHvMapGpaRange(partition.0,address,0,bytes as u64,7) };
        if let Err(error)=check(hr,"WHvMapGpaRange") { unsafe { VirtualFree(address,0,0x8000); } return Err(error); }
        Ok(Mapping{partition,address,bytes})

    }
    fn check(hr:i32, operation:&str)->Result<(),String> {
        if hr < 0 { Err(format!("{operation}: HRESULT 0x{:08x}",hr as u32)) } else { Ok(()) }
    }
    struct Partition(*mut c_void);
    impl Drop for Partition { fn drop(&mut self) { unsafe { WHvDeletePartition(self.0); } } }
    fn create()->Result<Partition,String> {
        let mut present=0u32; let mut written=0u32;
        // WHvCapabilityCodeHypervisorPresent = 0; output is Windows BOOL.
        unsafe { check(WHvGetCapability(0,(&mut present as *mut u32).cast(),4,&mut written),"WHvGetCapability")?; }
        if written!=4 || present==0 { return Err("Windows hypervisor unavailable; this is not an Android boot test".into()); }
        let mut handle=std::ptr::null_mut();
        unsafe { check(WHvCreatePartition(&mut handle),"WHvCreatePartition")?; }
        let partition=Partition(handle); let processors=1u32;
        // WHvPartitionPropertyCodeProcessorCount = 0x1fff (Windows SDK).
        unsafe {
            check(WHvSetPartitionProperty(partition.0,0x1fff,(&processors as *const u32).cast(),4),"WHvSetPartitionProperty")?;
            check(WHvSetupPartition(partition.0),"WHvSetupPartition")?;
        }
        Ok(partition)
    }
    pub fn probe()->Result<(),String> { let _partition=create()?; Ok(()) }
    fn with_guest<T>(code:&[u8], action:impl FnOnce(&Partition,&Mapping<'_>)->Result<T,String>)->Result<T,String> {
        with_guest_at(code,0,4096,action)
    }
    fn with_guest_at<T>(code:&[u8],entry:usize,ram_bytes:usize,action:impl FnOnce(&Partition,&Mapping<'_>)->Result<T,String>)->Result<T,String> {
        if entry>0xffff || entry.checked_add(code.len()).filter(|&n|n<=ram_bytes).is_none() {return Err("Test code exceeds real-mode/RAM bounds".into());}
        let partition=create()?;
        let memory=allocate(&partition,ram_bytes)?;
        unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(),memory.address.cast::<u8>().add(entry),code.len()); }
        unsafe { check(WHvCreateVirtualProcessor(partition.0,0,0),"WHvCreateVirtualProcessor")?; }
        let _processor=Processor(&partition);
        let names=[0x1c,0x1e,0x1f,0x13,0x15,0x12,0x14,0x10,0x11,0x00];
        let values=[Register::scalar(0x60000010),Register::scalar(0),Register::scalar(0),Register::segment(0x9b),Register::segment(0x93),Register::segment(0x93),Register::segment(0x93),Register::scalar(entry as u64),Register::scalar(2),Register::scalar(0)];
        set_registers(&partition,&names,&values)?;
        action(&partition,&memory)
    }
    fn set_registers(partition:&Partition,names:&[u32],values:&[Register])->Result<(),String> {
        if names.len()!=values.len() { return Err("Mismatched register arrays".into()); }
        unsafe { check(WHvSetVirtualProcessorRegisters(partition.0,0,names.as_ptr(),names.len() as u32,values.as_ptr()),"WHvSetVirtualProcessorRegisters") }
    }
    fn accumulator(partition:&Partition)->Result<u64,String> {
        let mut result=Register([0;16]);let name=0u32;
        unsafe { check(WHvGetVirtualProcessorRegisters(partition.0,0,&name,1,&mut result),"WHvGetVirtualProcessorRegisters")?; }
        Ok(u64::from_le_bytes(result.0[..8].try_into().unwrap()))
    }
    fn run_once(partition:&Partition)->Result<ExitContext,String> {
        let mut context=ExitContext([0;224]);
        unsafe { check(WHvRunVirtualProcessor(partition.0,0,context.0.as_mut_ptr().cast(),224),"WHvRunVirtualProcessor")?; }
        Ok(context)
    }
    impl ExitContext {
        fn reason(&self)->u32 {u32::from_le_bytes(self.0[..4].try_into().unwrap())}
    }
    // Covers the ENTIRE exit loop, not a fresh deadline after each I/O exit.
    // Partition stays alive until action completes AND watchdog joins. A wedged
    // API has a hard process deadline, never a detached thread using freed RAM.
    fn bounded<T>(partition:&Partition,millis:u64,action:impl FnOnce()->Result<T,String>)->Result<(T,bool),String> {
        use std::sync::{Arc,atomic::{AtomicBool,Ordering},mpsc};
        use std::time::Duration;
        let handle=partition.0 as usize;
        let canceled=Arc::new(AtomicBool::new(false));let flag=Arc::clone(&canceled);
        let (tx,rx)=mpsc::channel();
        let watchdog=std::thread::spawn(move || {
            if matches!(rx.recv_timeout(Duration::from_millis(millis)),Err(mpsc::RecvTimeoutError::Timeout)) {
                flag.store(true,Ordering::SeqCst);
                unsafe { WHvCancelRunVirtualProcessor(handle as *mut c_void,0,0); }
                if matches!(rx.recv_timeout(Duration::from_secs(5)),Err(mpsc::RecvTimeoutError::Timeout)) {
                    eprintln!("WHP guest deadline exceeded; terminating test process");
                    std::process::exit(2);
                }
            }
        });
        let result=action();
        let _=tx.send(());watchdog.join().map_err(|_|"WHP watchdog panicked")?;
        Ok((result?,canceled.load(Ordering::SeqCst)))
    }
    fn run_to_halt(partition:&Partition,bus:&mut crate::io::TestBus)->Result<usize,String> {
        let (count,timed_out)=bounded(partition,5000,|| {
            // An I/O-exit storm cannot bypass the wall deadline or exit budget.
            for count in 0..64 {
                let context=run_once(partition)?;
                match context.reason() {
                    8=>return Ok(count),
                    2=>{
                        let io=crate::io::IoExit::decode(&context.0)?;
                        let rip=Register::scalar(io.next_real_mode_rip()?);
                        if let Some(rax)=bus.access(io)? {
                            set_registers(partition,&[0,0x10],&[Register::scalar(rax),rip])?;
                        } else {set_registers(partition,&[0x10],&[rip])?;}
                    }
                    reason=>return Err(format!("Unsupported guest exit 0x{reason:x}")),
                }
            }
            Err("Guest exit budget exceeded".into())
        })?;
        if timed_out {return Err("Guest wall deadline exceeded".into());}
        Ok(count)
    }
    const RAM_PROGRAM:&[u8]=&[0xb8,0x17,0x5a,0xa3,0x00,0x02,0x31,0xc0,0xa1,0x00,0x02,0xf4];
    fn check_ram(partition:&Partition,memory:&Mapping<'_>)->Result<(),String> {
        let stored=unsafe {std::slice::from_raw_parts(memory.address.cast::<u8>().add(0x200),2)};
        if accumulator(partition)?!=0x5a17 || stored!=[0x17,0x5a] {return Err("Guest CPU/RAM result mismatch".into());}
        Ok(())
    }
    pub fn cpu_smoke()->Result<(),String> {
        with_guest(RAM_PROGRAM,|partition,memory| {
            run_to_halt(partition,&mut crate::io::TestBus::default())?;
            check_ram(partition,memory)
        })
    }
    pub fn io_smoke()->Result<(),String> {
        let mut code=vec![0xba,0xe9,0x00]; // DX=private debug port, NOT host hardware.
        for byte in b"NEXUS\n" {code.extend_from_slice(&[0xb0,*byte,0xee]);}
        code.extend_from_slice(&[
            0xba,0x04,0x05,                     // DX=test latch
            0x66,0xb8,0x78,0x56,0x34,0x12,     // EAX=12345678
            0x66,0xef,                          // OUT DX,EAX
            0x66,0xb8,0xdd,0xcc,0xbb,0xaa,     // EAX=AABBCCDD
            0xec,0x66,0xa3,0x04,0x02,          // IN AL; store EAX at 204
            0xed,0x66,0xa3,0x08,0x02,          // IN AX; store EAX at 208
            0x66,0xed,0x66,0xa3,0x00,0x02,     // IN EAX; store EAX at 200
            0xf4]);
        with_guest(&code,|partition,memory| {
            let mut bus=crate::io::TestBus::default();
            let count=run_to_halt(partition,&mut bus)?;
            let stored=unsafe {std::slice::from_raw_parts(memory.address.cast::<u8>().add(0x200),12)};
            if count!=10 || bus.console!=b"NEXUS\n" || accumulator(partition)?!=0x12345678
                || stored!=[0x78,0x56,0x34,0x12,0x78,0xcc,0xbb,0xaa,0x78,0x56,0xbb,0xaa] {
                return Err(format!("Guest port-I/O result mismatch: exits={count}, data={stored:?}"));
            }
            Ok(())
        })?;
        // Fail closed on REP/strings, host-port requests and excessive exits.
        for (code,expected) in [
            (&[0xba,0xe9,0x00,0xb9,0x01,0x00,0xf3,0x6e,0xf4][..],"String/REP"),
            (&[0xba,0xff,0xff,0xec,0xf4][..],"Unsupported test port"),
            (&[0xba,0xe9,0x00,0xb0,0x78,0xee,0xeb,0xfd][..],"exit budget"),
        ] {
            let result=with_guest(code,|partition,_|run_to_halt(partition,&mut crate::io::TestBus::default()));
            match result {Err(error) if error.contains(expected)=>{},other=>return Err(format!("Negative I/O gate failed ({expected}): {other:?}"))}
        }
        Ok(())
    }
    pub fn boot_test_sector(path:&std::path::Path)->Result<Vec<u8>,String> {
        let sector=crate::boot::read_test_sector(path)?;
        with_guest_at(&sector,0x7c00,1024*1024,|partition,_| {
            let mut bus=crate::io::TestBus::default();
            run_to_halt(partition,&mut bus)?;
            Ok(bus.console)
        })
    }
    pub fn cancel_smoke()->Result<(),String> {
        with_guest(&[0xeb,0xfe],|partition,memory| { // Deliberate no-exit infinite loop.
            let (context,canceled)=bounded(partition,250,||run_once(partition))?;
            if !canceled || context.reason()!=0x2001 {return Err(format!("Expected cancellation exit, got 0x{:x}",context.reason()));}
            // Prove cancellation did not destroy the vCPU: restart the SAME one.
            unsafe {std::ptr::copy_nonoverlapping(RAM_PROGRAM.as_ptr(),memory.address.cast::<u8>(),RAM_PROGRAM.len());}
            set_registers(partition,&[0x10,0],&[Register::scalar(0),Register::scalar(0)])?;
            run_to_halt(partition,&mut crate::io::TestBus::default())?;
            check_ram(partition,memory)
        })
    }

}
#[cfg(windows)]
pub use windows::{probe,cpu_smoke,io_smoke,cancel_smoke,boot_test_sector};
#[cfg(not(windows))]
pub fn probe()->Result<(),String> { Err("Direct WHP backend requires Windows 10 x64".into()) }

#[cfg(not(windows))]
pub fn cpu_smoke()->Result<(),String> { Err("Direct WHP CPU test requires Windows 10 x64".into()) }

#[cfg(not(windows))]
pub fn io_smoke()->Result<(),String> {Err("Direct WHP port I/O requires Windows 10 x64".into())}
#[cfg(not(windows))]
pub fn cancel_smoke()->Result<(),String> {Err("Direct WHP cancellation requires Windows 10 x64".into())}

#[cfg(not(windows))]
pub fn boot_test_sector(_path:&std::path::Path)->Result<Vec<u8>,String> {Err("Direct WHP test-sector execution requires Windows 10 x64".into())}

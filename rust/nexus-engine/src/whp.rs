//! Direct Windows Hypervisor Platform API — no QEMU dependency.
//! Partition lifecycle plus bounded real-mode CPU/RAM smoke test. No Android machine.
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
    struct Mapping<'a> { partition:&'a Partition, address:*mut c_void }
    impl Drop for Mapping<'_> { fn drop(&mut self) { unsafe { WHvUnmapGpaRange(self.partition.0,0,4096); VirtualFree(self.address,0,0x8000); } } }
    struct Processor<'a>(&'a Partition);
    impl Drop for Processor<'_> { fn drop(&mut self) { unsafe { WHvDeleteVirtualProcessor(self.0.0,0); } } }
    fn allocate(partition:&Partition)->Result<Mapping<'_>,String> {
        let address=unsafe { VirtualAlloc(std::ptr::null_mut(),4096,0x3000,4) };
        if address.is_null() { return Err("VirtualAlloc failed".into()); }
        let hr=unsafe { WHvMapGpaRange(partition.0,address,0,4096,7) };
        if let Err(error)=check(hr,"WHvMapGpaRange") { unsafe { VirtualFree(address,0,0x8000); } return Err(error); }
        Ok(Mapping{partition,address})

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
    pub fn cpu_smoke()->Result<(),String> {
        let partition=create()?;
        let memory=allocate(&partition)?;
        // 16-bit real-mode guest: store 0x5a17 to guest RAM, clear AX, load it,
        // halt. No disk, firmware, QEMU, OS calls or host executable guest data.
        let code=[0xb8u8,0x17,0x5a,0xa3,0x00,0x02,0x31,0xc0,0xa1,0x00,0x02,0xf4];
        unsafe { std::ptr::copy_nonoverlapping(code.as_ptr(),memory.address.cast::<u8>(),code.len()); }
        unsafe { check(WHvCreateVirtualProcessor(partition.0,0,0),"WHvCreateVirtualProcessor")?; }
        let _processor=Processor(&partition);
        let names=[0x1c,0x1e,0x1f,0x13,0x15,0x12,0x14,0x10,0x11,0x00];
        let values=[Register::scalar(0x60000010),Register::scalar(0),Register::scalar(0),Register::segment(0x9b),Register::segment(0x93),Register::segment(0x93),Register::segment(0x93),Register::scalar(0),Register::scalar(2),Register::scalar(0)];
        unsafe { check(WHvSetVirtualProcessorRegisters(partition.0,0,names.as_ptr(),names.len() as u32,values.as_ptr()),"WHvSetVirtualProcessorRegisters")?; }
        // Partition remains alive until run returns AND watchdog joins. A wedged
        // API has a hard process deadline, never a detached thread using freed RAM.
        let handle=partition.0 as usize;
        let (tx,rx)=std::sync::mpsc::channel();
        let watchdog=std::thread::spawn(move || {
            use std::sync::mpsc::RecvTimeoutError::Timeout;
            use std::time::Duration;
            if matches!(rx.recv_timeout(Duration::from_secs(5)),Err(Timeout)) {
                unsafe { WHvCancelRunVirtualProcessor(handle as *mut c_void,0,0); }
                if matches!(rx.recv_timeout(Duration::from_secs(5)),Err(Timeout)) {
                    eprintln!("WHP CPU smoke deadline exceeded; terminating test process");
                    std::process::exit(2);
                }
            }
        });
        let mut context=ExitContext([0;224]);
        let hr=unsafe { WHvRunVirtualProcessor(partition.0,0,context.0.as_mut_ptr().cast(),224) };
        let _=tx.send(()); watchdog.join().map_err(|_|"WHP watchdog panicked")?;
        check(hr,"WHvRunVirtualProcessor")?;
        let reason=u32::from_le_bytes(context.0[..4].try_into().unwrap());
        if reason!=8 { return Err(format!("Expected HLT exit 8, got 0x{reason:x}")); }
        let mut result=Register([0;16]); let rax=0u32;
        unsafe { check(WHvGetVirtualProcessorRegisters(partition.0,0,&rax,1,&mut result),"WHvGetVirtualProcessorRegisters")?; }
        let ax=u16::from_le_bytes(result.0[..2].try_into().unwrap());
        let stored=unsafe { std::slice::from_raw_parts(memory.address.cast::<u8>().add(0x200),2) };
        if ax!=0x5a17 || stored!=[0x17,0x5a] { return Err("Guest CPU/RAM result mismatch".into()); }
        Ok(())
    }
}
#[cfg(windows)]
pub use windows::{probe,cpu_smoke};
#[cfg(not(windows))]
pub fn probe()->Result<(),String> { Err("Direct WHP backend requires Windows 10 x64".into()) }

#[cfg(not(windows))]
pub fn cpu_smoke()->Result<(),String> { Err("Direct WHP CPU test requires Windows 10 x64".into()) }

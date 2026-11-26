//! Direct Windows Hypervisor Platform API — no QEMU dependency.
//! Partition lifecycle only. CPU execution/device emulation are not implemented.
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
    }
    fn check(hr:i32, operation:&str)->Result<(),String> {
        if hr < 0 { Err(format!("{operation}: HRESULT 0x{:08x}",hr as u32)) } else { Ok(()) }
    }
    struct Partition(*mut c_void);
    impl Drop for Partition { fn drop(&mut self) { unsafe { WHvDeletePartition(self.0); } } }
    pub fn probe()->Result<(),String> {
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
        Ok(())
    }
}
#[cfg(windows)]
pub use windows::probe;
#[cfg(not(windows))]
pub fn probe()->Result<(),String> { Err("Direct WHP backend requires Windows 10 x64".into()) }

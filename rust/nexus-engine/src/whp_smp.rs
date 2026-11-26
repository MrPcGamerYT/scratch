// Included inside whp::windows. Real WHP AP startup, not fabricated AP markers.
fn registers_at(partition:&Partition,index:u32,names:&[u32],values:&[Register])->Result<(),String> {
    if index>=partition.2||names.len()!=values.len() {return Err("Invalid indexed register access".into());}
    unsafe {check(WHvSetVirtualProcessorRegisters(partition.0,index,names.as_ptr(),names.len() as u32,values.as_ptr()),"WHP indexed registers")}
}
fn run_index(handle:usize,index:u32)->Result<u32,String> {
    let mut context=ExitContext([0;224]);
    unsafe {check(WHvRunVirtualProcessor(handle as *mut c_void,index,context.0.as_mut_ptr().cast(),224),"WHP indexed run")?;}
    Ok(context.reason())
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
    // The AP is NOT assigned a host-created instruction pointer of 0x9000.
    let code=[
        0xc7,0x05,0xf0,0x00,0xe0,0xfe,0xff,0x01,0x00,0x00,
        0xc7,0x05,0x10,0x03,0xe0,0xfe,0x00,0x00,0x00,0x01,
        0xc7,0x05,0x00,0x03,0xe0,0xfe,0x00,0x45,0x00,0x00,
        0xc7,0x05,0x00,0x03,0xe0,0xfe,0x09,0x46,0x00,0x00,
        0x80,0x3d,0x00,0xa0,0x01,0x00,0x5a,0x75,0xf7,0xf4,
    ];
    f.ram[cpu::ENTRY..cpu::ENTRY+code.len()].copy_from_slice(&code);
    // AP startup in real mode: DS=0, address-size override selects 32-bit RAM GPA.
    let ap=[0x31,0xc0,0x8e,0xd8,0x67,0xc6,0x05,0x00,0xa0,0x01,0x00,0x5a,0xf4];
    f.ram[0x9000..0x9000+ap.len()].copy_from_slice(&ap);
    unsafe {std::ptr::copy_nonoverlapping(f.ram.as_ptr(),memory.address.cast::<u8>(),f.ram.len());}
    configure_fixture(&partition,&f)?;
    registers_at(&partition,0,&[0x2003],&[Register::scalar(0xfee00900)])?;
    registers_at(&partition,1,&[0x1c,0x1e,0x1f,0x2001,0x13,0x15,0x12,0x14,0x10,0x11,0x2003,0x80000005],&[
        Register::scalar(0x60000010),Register::scalar(0),Register::scalar(0),Register::scalar(0),
        Register::segment(0x9b),Register::segment(0x93),Register::segment(0x93),Register::segment(0x93),
        Register::scalar(0),Register::scalar(2),Register::scalar(0xfee00800),Register::scalar(1)])?;
    let handle=partition.0 as usize;
    let ((bsp,ap),expired)=bounded(&partition,5000,||std::thread::scope(|scope| {
        let ap=scope.spawn(move ||run_index(handle,1));
        let bsp=run_index(handle,0);
        let ap=ap.join().map_err(|_|"AP run thread panicked")?;
        Ok((bsp?,ap?))
    }))?;
    if expired||bsp!=8||ap!=8 {return Err(format!("SMP INIT/SIPI failed: BSP exit={bsp:x} AP exit={ap:x} timeout={expired}"));}
    // Both vCPUs have stopped and workers joined before accessing shared RAM.
    let marker=unsafe {*memory.address.cast::<u8>().add(0x1a000)};
    if marker!=0x5a {return Err("AP startup marker missing".into());}
    Ok(())
}

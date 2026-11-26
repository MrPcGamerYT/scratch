//! Explicit test startup states. These seed vCPU state, not a BIOS or guest-driven
//! real-to-protected/long-mode transition. No kernel loader is implemented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuMode { Real16, Protected32, Long64 }
impl CpuMode {
    pub fn from_exit(context:&[u8])->Result<Self,String> {
        if context.len()!=224 {return Err("Invalid execution context size".into());}
        let flags=u16::from_le_bytes(context[8..10].try_into().unwrap());
        let attributes=u16::from_le_bytes(context[30..32].try_into().unwrap());
        if flags&4==0 {Ok(Self::Real16)}
        else if flags&16!=0 && attributes&0x2000!=0 {Ok(Self::Long64)}
        else if attributes&0x4000!=0 {Ok(Self::Protected32)}
        else {Err("16-bit protected/compatibility I/O is not implemented".into())}
    }

    pub fn next_rip(self,rip:u64,length:u8)->Result<u64,String> {
        if !(1..=15).contains(&length) {return Err("Invalid instruction length".into());}
        let next=rip.checked_add(u64::from(length)).ok_or("Instruction pointer overflow")?;
        let valid=|n:u64| match self {
            Self::Real16=>n<=u64::from(u16::MAX),
            Self::Protected32=>n<=u64::from(u32::MAX),
            Self::Long64=>n<=0x0000_7fff_ffff_ffff || n>=0xffff_8000_0000_0000,
        };
        if !valid(rip)||!valid(next) {return Err("Instruction pointer outside supported mode range".into());}
        Ok(next)
    }
}
pub const RAM_BYTES:usize=2*1024*1024;
pub const ENTRY:usize=0x8000;
pub const GDT:usize=0x5000;
pub const IDT:usize=0x6000;
pub const DATA:usize=0x18000;
pub const INTERRUPT_COUNT:usize=0x19000;
pub const STACK:u64=0x1ff000;
pub const ALIAS:u64=0x400000;
pub const VALUE64:u64=0x8877_6655_4433_2211;
pub const VALUE32:u64=0x1234_5678;
pub struct Fixture {
    pub mode:CpuMode, pub ram:Vec<u8>, pub cr0:u64, pub cr3:u64, pub cr4:u64,
    pub efer:u64, pub code_attributes:u16, pub idt_limit:u16,
}
fn write(ram:&mut [u8],address:usize,data:&[u8])->Result<(),String> {
    let end=address.checked_add(data.len()).ok_or("RAM offset overflow")?;
    ram.get_mut(address..end).ok_or("Fixture write exceeds guest RAM")?.copy_from_slice(data);Ok(())
}
fn qword(ram:&mut [u8],address:usize,value:u64)->Result<(),String> {write(ram,address,&value.to_le_bytes())}
pub fn fixture(mode:CpuMode)->Result<Fixture,String> {
    if mode==CpuMode::Real16 {return Err("Use the separate real-mode fixture".into());}
    let long=mode==CpuMode::Long64;
    let mut f=Fixture {mode,ram:vec![0;RAM_BYTES],cr0:if long {0x80000011} else {0x11},
        cr3:if long {0x1000} else {0},cr4:if long {0x20} else {0},
        efer:if long {0x500} else {0},code_attributes:if long {0xa09b} else {0xc09b},
        idt_limit:if long {0x80f} else {0}};
    // Null, flat code and flat data descriptors; accessed bits already set.
    qword(&mut f.ram,GDT+8,if long {0x00af9b000000ffff} else {0x00cf9b000000ffff})?;
    qword(&mut f.ram,GDT+16,0x00cf93000000ffff)?;
    let mut code=vec![0xba,0xe9,0,0,0,0xb0,if long {b'L'} else {b'P'},0xee];
    if long {
        // PML4 -> PDPT -> PD. First 2 MiB identity mapped using a large page.
        // Separate 4 KiB alias at VA 0x400000 -> PA 0x18000 proves real paging.
        for (address,value) in [(0x1000,0x2003),(0x2000,0x3003),(0x3000,0x83),
            (0x3010,0x4003),(0x4000,DATA as u64|3)] {qword(&mut f.ram,address,value)?;}
        code.extend_from_slice(&[0x48,0xb8]);code.extend_from_slice(&VALUE64.to_le_bytes());
        code.extend_from_slice(&[0x48,0xa3]);code.extend_from_slice(&ALIAS.to_le_bytes());
        code.extend_from_slice(&[0x48,0x31,0xc0,0x48,0xa1]);code.extend_from_slice(&ALIAS.to_le_bytes());
        code.extend_from_slice(&[0xcd,0x80,0xf4]); // Real guest INT 80h / IRETQ round trip.
        let handler=0x8100usize;
        let mut gate=[0u8;16];gate[..2].copy_from_slice(&(handler as u16).to_le_bytes());
        gate[2..4].copy_from_slice(&8u16.to_le_bytes());gate[5]=0x8e;
        gate[6..8].copy_from_slice(&((handler>>16) as u16).to_le_bytes());
        write(&mut f.ram,IDT+0x80*16,&gate)?;
        // INC qword [0x19000]; IRETQ. Does not alter the expected accumulator.
        write(&mut f.ram,handler,&[0x48,0xff,0x04,0x25,0x00,0x90,0x01,0x00,0x48,0xcf])?;
    } else {
        code.push(0xb8);code.extend_from_slice(&(VALUE32 as u32).to_le_bytes());
        code.push(0xa3);code.extend_from_slice(&(DATA as u32).to_le_bytes());
        code.extend_from_slice(&[0x31,0xc0,0xa1]);code.extend_from_slice(&(DATA as u32).to_le_bytes());
        code.push(0xf4);
    }
    write(&mut f.ram,ENTRY,&code)?;
    Ok(f)
}
/// Long-mode #PF recovery: unmapped read followed by a supervisor write to a
/// read-only PTE with CR0.WP enabled. Guest handler records CR2/error and IRETQs.
pub fn protection_fixture()->Result<Fixture,String> {
    let mut f=fixture(CpuMode::Long64)?;f.cr0|=0x10000;
    qword(&mut f.ram,0x4000,DATA as u64|1)?;
    qword(&mut f.ram,DATA,VALUE64)?;
    let handler=0x8500usize;
    let mut gate=[0;16];gate[..2].copy_from_slice(&(handler as u16).to_le_bytes());
    gate[2..4].copy_from_slice(&8u16.to_le_bytes());gate[5]=0x8e;
    write(&mut f.ram,IDT+14*16,&gate)?;
    let mut code=vec![0x48,0xa1];code.extend_from_slice(&0x200000u64.to_le_bytes()); // absent PDE
    code.extend_from_slice(&[0x48,0xb8]);code.extend_from_slice(&0x55u64.to_le_bytes());
    code.extend_from_slice(&[0x48,0xa3]);code.extend_from_slice(&ALIAS.to_le_bytes()); // read-only PTE
    code.extend_from_slice(&[0x48,0xa1]);code.extend_from_slice(&ALIAS.to_le_bytes());
    code.push(0xf4);write(&mut f.ram,ENTRY,&code)?;
    let mut isr=vec![0x50,0x0f,0x20,0xd0,0x48,0xa3]; // PUSH RAX; MOV RAX,CR2; store
    isr.extend_from_slice(&(DATA as u64+8).to_le_bytes());
    isr.extend_from_slice(&[0x48,0x8b,0x44,0x24,8,0x48,0xa3]); // error code below saved RAX
    isr.extend_from_slice(&(DATA as u64+16).to_le_bytes());
    isr.extend_from_slice(&[0x58,0x48,0x83,0xc4,8, // POP RAX; discard error code
        0x48,0x83,0x04,0x24,10, // skip faulting 10-byte MOFFS instruction
        0x48,0xff,0x04,0x25,0,0x90,1,0,0x48,0xcf]); // count++; IRETQ
    write(&mut f.ram,handler,&isr)?;Ok(f)
}

/// Real-mode entry image: the guest itself loads the GDT, sets CR0.PE,
/// enables PAE and EFER.LME, installs CR3 and enables paging, then far-jumps
/// into long mode. Host must seed only the real-mode entry state.
pub fn transition_image()->Result<Vec<u8>,String> {
    let mut ram=vec![0;RAM_BYTES];
    for (address,value) in [(GDT+8,0x00cf9b000000ffff),
        (GDT+16,0x00cf93000000ffff),(GDT+24,0x00af9b000000ffff),
        (0x1000,0x2003),(0x2000,0x3003),(0x3000,0x83),
        (0x3010,0x4003),(0x4000,DATA as u64|3)] {qword(&mut ram,address,value)?;}
    write(&mut ram,0x5200,&[31,0,0,0x50,0,0])?; // GDTR: limit31, base0x5000
    write(&mut ram,ENTRY,&[
        0xfa,0x31,0xc0,0x8e,0xd8,0x8e,0xc0,0x8e,0xd0, // CLI; zero data segments
        0xbc,0x00,0x7c,0xba,0xe9,0,0xb0,b'R',0xee,
        0x0f,0x01,0x16,0x00,0x52, // LGDT [0x5200]
        0x0f,0x20,0xc0,0x66,0x83,0xc8,1,0x0f,0x22,0xc0,
        0x66,0xea,0,0x82,0,0,8,0, // JMP ptr16:32 -> protected entry
    ])?;
    write(&mut ram,0x8200,&[
        0x66,0xb8,16,0,0x8e,0xd8,0x8e,0xc0,0x8e,0xd0,
        0xbc,0x00,0xf0,0x1f,0, // ESP=STACK
        0xba,0xe9,0,0,0,0xb0,b'P',0xee,
        0x0f,0x20,0xe0,0x83,0xc8,0x20,0x0f,0x22,0xe0, // CR4.PAE
        0xb8,0,0x10,0,0,0x0f,0x22,0xd8, // CR3=0x1000
        0xb9,0x80,0,0,0xc0,0x0f,0x32, // RDMSR EFER
        0x0d,0,1,0,0,0x0f,0x30, // EFER.LME
        0x0f,0x20,0xc0,0x0d,0,0,0,0x80,0x0f,0x22,0xc0, // CR0.PG
        0xea,0,0x84,0,0,24,0, // JMP long-mode code selector
    ])?;
    let mut long=vec![0xba,0xe9,0,0,0,0xb0,b'L',0xee,0x48,0xb8];
    long.extend_from_slice(&VALUE64.to_le_bytes());
    long.extend_from_slice(&[0x48,0xa3]);long.extend_from_slice(&ALIAS.to_le_bytes());
    long.extend_from_slice(&[0x48,0x31,0xc0,0x48,0xa1]);long.extend_from_slice(&ALIAS.to_le_bytes());
    long.push(0xf4);write(&mut ram,0x8400,&long)?;
    Ok(ram)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn read(ram:&[u8],at:usize)->u64 {u64::from_le_bytes(ram[at..at+8].try_into().unwrap())}
    #[test] fn mode_ranges_and_canonical_addresses() {
        assert_eq!(CpuMode::Protected32.next_rip(0x10000,2).unwrap(),0x10002);
        assert!(CpuMode::Real16.next_rip(0xffff,1).is_err());
        assert!(CpuMode::Protected32.next_rip(0xffff_ffff,1).is_err());
        assert_eq!(CpuMode::Long64.next_rip(0xffff_8000_0000_0000,3).unwrap(),0xffff_8000_0000_0003);
        for rip in [0x0000_7fff_ffff_ffff,0x0000_8000_0000_0000,u64::MAX] {assert!(CpuMode::Long64.next_rip(rip,1).is_err());}
        assert!(CpuMode::Long64.next_rip(0,0).is_err());assert!(CpuMode::Long64.next_rip(0,16).is_err());
    }
    #[test] fn infer_mode_after_guest_transitions() {
        let mut c=[0;224];assert_eq!(CpuMode::from_exit(&c).unwrap(),CpuMode::Real16);
        c[8]=4;c[31]=0x40;assert_eq!(CpuMode::from_exit(&c).unwrap(),CpuMode::Protected32);
        c[8]=20;c[31]=0x20;assert_eq!(CpuMode::from_exit(&c).unwrap(),CpuMode::Long64);
        c[31]=0x40;assert_eq!(CpuMode::from_exit(&c).unwrap(),CpuMode::Protected32);
        c[31]=0;assert!(CpuMode::from_exit(&c).is_err());assert!(CpuMode::from_exit(&[]).is_err());
    }
    #[test] fn protected_start_state() {
        let f=fixture(CpuMode::Protected32).unwrap();assert_eq!(f.ram.len(),RAM_BYTES);
        assert_eq!((f.cr0,f.cr3,f.cr4,f.efer),(0x11,0,0,0));
        assert_eq!(read(&f.ram,GDT+8),0x00cf9b000000ffff);
        assert_eq!(&f.ram[ENTRY..ENTRY+5],&[0xba,0xe9,0,0,0]);
        assert_eq!(read(&f.ram,DATA),0);assert!(fixture(CpuMode::Real16).is_err());
    }
    #[test] fn long_start_tables_and_alias() {
        let f=fixture(CpuMode::Long64).unwrap();assert_eq!((f.cr0,f.cr3,f.cr4,f.efer),(0x80000011,0x1000,0x20,0x500));
        for (at,value) in [(0x1000,0x2003),(0x2000,0x3003),(0x3000,0x83),(0x3010,0x4003),(0x4000,0x18003)] {assert_eq!(read(&f.ram,at),value);}
        assert_eq!(read(&f.ram,0x3008),0); // 2..4 MiB deliberately NOT mapped.
        assert_eq!(&f.ram[IDT+0x800..IDT+0x808],&[0,0x81,8,0,0,0x8e,0,0]);
        assert_eq!(read(&f.ram,INTERRUPT_COUNT),0);
    }
    #[test] fn transition_tables_and_real_entry_are_independent_of_seeded_long_state() {
        let ram=transition_image().unwrap();assert_eq!(ram.len(),RAM_BYTES);
        assert_eq!(ram[ENTRY],0xfa);assert_eq!(ram[0x8200],0x66);
        assert_eq!(&ram[0x5200..0x5206],&[31,0,0,0x50,0,0]);
        assert_eq!(read(&ram,GDT+8),0x00cf9b000000ffff);
        assert_eq!(read(&ram,GDT+24),0x00af9b000000ffff);
        assert_eq!(read(&ram,0x4000),DATA as u64|3);assert_eq!(read(&ram,DATA),0);
    }
    #[test] fn protection_fixture_enables_wp_and_keeps_fault_targets_unmapped_or_readonly() {
        let f=protection_fixture().unwrap();assert_ne!(f.cr0&0x10000,0);
        assert_eq!(read(&f.ram,0x3008),0);assert_eq!(read(&f.ram,0x4000),DATA as u64|1);
        assert_eq!(read(&f.ram,DATA),VALUE64);assert_eq!(read(&f.ram,INTERRUPT_COUNT),0);
        assert_eq!(&f.ram[IDT+14*16..IDT+14*16+6],&[0,0x85,8,0,0,0x8e]);
    }
    #[test] fn fixture_writes_fail_closed() {
        let mut ram=[0;16];assert!(write(&mut ram,16,&[1]).is_err());
        assert!(write(&mut ram,usize::MAX,&[1]).is_err());assert_eq!(ram,[0;16]);
    }
}

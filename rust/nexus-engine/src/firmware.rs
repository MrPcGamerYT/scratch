//! Intel MultiProcessor Specification 1.4 discovery data, not a BIOS implementation.
//! Used by the explicit APIC machine; never advertise it for the legacy PIC profile.
pub const MP_BASE:usize=0xf0000;
pub const CONFIG_BASE:usize=MP_BASE+0x100;
pub const IOAPIC_ID:u8=8;
pub const LAPIC_ADDRESS:u32=0xfee00000;
pub const IOAPIC_ADDRESS:u32=0xfec00000;
fn sum(bytes:&[u8])->u8 {bytes.iter().fold(0u8,|sum,b|sum.wrapping_add(*b))}
fn checksum(bytes:&mut [u8],offset:usize) {bytes[offset]=0;bytes[offset]=0u8.wrapping_sub(sum(bytes));}
/// CPUs have APIC IDs 0..count-1, matching the virtual CPUID topology.
/// ISA IRQ0 is overridden to IOAPIC input2; IRQ2 is the absent PIC cascade.
pub fn mp_tables(count:u32)->Result<Vec<u8>,String> {
    if !(1..=8).contains(&count) {return Err("MP topology requires one to eight CPUs".into());}
    let mut config=vec![0u8;44];config[..4].copy_from_slice(b"PCMP");config[6]=4;
    config[8..16].copy_from_slice(b"NEXUS   ");config[16..28].copy_from_slice(b"NEXUS-WHP   ");
    config[36..40].copy_from_slice(&LAPIC_ADDRESS.to_le_bytes());
    for id in 0..count {
        let mut cpu=[0u8;20];cpu[1]=id as u8;cpu[2]=0x14;cpu[3]=if id==0 {3} else {1};
        // No fabricated CPU signature/features: CPUID is authoritative.
        config.extend_from_slice(&cpu);
    }
    config.extend_from_slice(&[1,0,b'I',b'S',b'A',b' ',b' ',b' ']);
    config.extend_from_slice(&[2,IOAPIC_ID,0x11,1]);config.extend_from_slice(&IOAPIC_ADDRESS.to_le_bytes());
    for irq in 0..16u8 {
        if irq==2 {continue;}
        // Type3, INT, conforming ISA polarity/trigger (active-high edge), bus0.
        config.extend_from_slice(&[3,0,0,0,0,irq,IOAPIC_ID,if irq==0 {2} else {irq}]);
    }
    let length=config.len() as u16;config[4..6].copy_from_slice(&length.to_le_bytes());
    config[34..36].copy_from_slice(&((count+17) as u16).to_le_bytes());
    checksum(&mut config,7);
    let mut result=vec![0u8;0x100];result[..4].copy_from_slice(b"_MP_");
    result[4..8].copy_from_slice(&(CONFIG_BASE as u32).to_le_bytes());result[8]=1;result[9]=4;
    checksum(&mut result[..16],10);result.extend_from_slice(&config);Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn mp_layout_checksum_and_routes_match_machine_for_all_cpu_counts() {
        for count in 1..=8 {
            let b=mp_tables(count).unwrap();assert_eq!(MP_BASE%16,0);assert_eq!(sum(&b[..16]),0);
            assert_eq!(&b[..4],b"_MP_");assert_eq!(u32::from_le_bytes(b[4..8].try_into().unwrap()) as usize,CONFIG_BASE);
            assert_eq!(b[8],1);assert_eq!(b[9],4);assert!(b[11..16].iter().all(|v|*v==0));
            let c=&b[0x100..];assert_eq!(&c[..4],b"PCMP");assert_eq!(sum(c),0);
            assert_eq!(usize::from(u16::from_le_bytes(c[4..6].try_into().unwrap())),c.len());
            let entries=u16::from_le_bytes(c[34..36].try_into().unwrap());assert_eq!(u32::from(entries),count+17);
            let mut at=44;let mut ids=Vec::new();let mut routes=Vec::new();let mut bootstrap=0;
            for _ in 0..entries {
                match c[at] {
                    0=>{ids.push(c[at+1]);assert_eq!(c[at+2],0x14);assert_ne!(c[at+3]&1,0);bootstrap+=u32::from(c[at+3]&2!=0);at+=20;},
                    1=>{assert_eq!(&c[at+2..at+8],b"ISA   ");at+=8;},
                    2=>{assert_eq!(c[at+1],IOAPIC_ID);assert_eq!(c[at+3],1);assert_eq!(u32::from_le_bytes(c[at+4..at+8].try_into().unwrap()),IOAPIC_ADDRESS);at+=8;},
                    3=>{assert_eq!(c[at+6],IOAPIC_ID);routes.push((c[at+5],c[at+7]));at+=8;},
                    _=>panic!("Unexpected MP entry"),
                }
            }
            assert_eq!(at,c.len());assert_eq!(ids,(0..count as u8).collect::<Vec<_>>());assert_eq!(bootstrap,1);
            assert_eq!(routes.len(),15);assert!(routes.contains(&(0,2)));assert!(!routes.iter().any(|r|r.0==2));
            for irq in 1..16 {if irq!=2 {assert!(routes.contains(&(irq,irq)));}}
            assert!(MP_BASE+b.len()<=0x100000); // lives inside existing E820 reserved hole
        }
        assert!(mp_tables(0).is_err());assert!(mp_tables(9).is_err());
    }
}

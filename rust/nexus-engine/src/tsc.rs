//! CPUID.15H ratio from the hypervisor's actual processor clock frequency.
//! Do not substitute host nominal/max CPU MHz or host wall-clock measurements.
#[repr(C)]
#[derive(Debug,PartialEq)]
pub struct CpuidResult {pub function:u32,pub reserved:[u32;3],pub eax:u32,pub ebx:u32,pub ecx:u32,pub edx:u32}
pub fn frequency_leaf(hz:u64)->Result<CpuidResult,String> {
    if !(1_000_000..=20_000_000_000).contains(&hz) {return Err("WHP processor clock frequency outside supported range".into());}
    // CPUID TSC Hz = crystal Hz * EBX / EAX. Reduce the rational exactly.
    let crystal=100_000_000u64;
    let (mut a,mut b)=(hz,crystal);while b!=0 {let r=a%b;a=b;b=r;}
    let numerator=u32::try_from(hz/a).map_err(|_|"WHP clock ratio exceeds CPUID register size")?;
    Ok(CpuidResult{function:0x15,reserved:[0;3],eax:(crystal/a) as u32,ebx:numerator,ecx:crystal as u32,edx:0})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn exact_clock_ratios_not_nominal_cpu_mhz() {
        assert_eq!(std::mem::size_of::<CpuidResult>(),32);
        for hz in [1_000_000,999_999_937,2_400_000_000,2_600_000_000,3_579_545_000,20_000_000_000] {
            let leaf=frequency_leaf(hz).unwrap();assert_eq!(leaf.function,0x15);
            assert_eq!(u64::from(leaf.ecx)*u64::from(leaf.ebx)/u64::from(leaf.eax),hz);
            assert_eq!((leaf.reserved,leaf.edx),([0;3],0));
        }
    }
    #[test] fn invalid_or_unrepresentable_clocks_are_not_silently_rounded() {
        for hz in [0,999_999,20_000_000_001,u64::MAX,9_999_999_967] {assert!(frequency_leaf(hz).is_err());}
    }
}

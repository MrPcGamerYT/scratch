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
/// Extend an older basic CPUID range only with explicit zero-feature leaves.
/// Otherwise undefined leaves may alias the native maximum and invent features.
pub fn frequency_leaves(hz:u64,native:[u32;4])->Result<Vec<CpuidResult>,String> {
    let mut leaves=vec![frequency_leaf(hz)?];
    if native[0]<0x15 {
        leaves.push(CpuidResult{function:0,reserved:[0;3],eax:0x15,ebx:native[1],ecx:native[2],edx:native[3]});
        for function in native[0]+1..0x15 {
            leaves.push(CpuidResult{function,reserved:[0;3],eax:0,ebx:0,ecx:0,edx:0});
        }
    }
    Ok(leaves)
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
    #[test] fn old_cpuid_ranges_are_extended_without_inventing_features() {
        let leaves=frequency_leaves(2_600_000_000,[0xd,1,2,3]).unwrap();
        assert_eq!(leaves.len(),9);assert_eq!(leaves[1].function,0);
        assert_eq!((leaves[1].eax,leaves[1].ebx,leaves[1].ecx,leaves[1].edx),(0x15,1,2,3));
        for (n,leaf) in leaves[2..].iter().enumerate() {
            assert_eq!(leaf.function,0xe+n as u32);assert_eq!((leaf.eax,leaf.ebx,leaf.ecx,leaf.edx),(0,0,0,0));
        }
        assert_eq!(frequency_leaves(2_600_000_000,[0x20,1,2,3]).unwrap().len(),1);
    }
    #[test] fn invalid_or_unrepresentable_clocks_are_not_silently_rounded() {
        for hz in [0,999_999,20_000_000_001,u64::MAX,9_999_999_967] {assert!(frequency_leaf(hz).is_err());}
    }
}

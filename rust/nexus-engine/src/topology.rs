//! One socket, one hardware thread per virtual core. Never leak host core/APIC IDs.
//! Preserve WHP-filtered instruction features, not raw host CPUID feature words.
pub const EXIT_LEAVES:[u32;8]=[1,4,5,0xb,0x1f,0x80000001,0x80000008,0x8000001e];
pub fn result(leaf:u32,subleaf:u32,index:u32,count:u32,apic:bool,mut d:[u32;4])->Result<[u32;4],String> {
    if !(1..=8).contains(&count)||index>=count {return Err("Invalid virtual CPU topology".into());}
    let width=32-(count-1).leading_zeros();
    match leaf {
        1=>{
            d[1]=(d[1]&0xffff)|(count<<16)|(index<<24);
            d[2]&=!((1<<3)|(1<<5)|(1<<21)|(1<<24)); // no MONITOR/VMX/x2APIC/TSC-deadline contract
            d[3]&=!((1<<9)|(1<<28));
            if apic {d[3]|=1<<9;}if count>1 {d[3]|=1<<28;}
        }
        4=>{
            if d[0]&31==0 {return Ok([0;4]);}
            let sharing=(((d[0]>>14)&0xfff)+1).min(count);
            d[0]=(d[0]&0x3fff)|((sharing-1)<<14)|((count-1)<<26);
        }
        5=>d=[0;4],
        0xb|0x1f=>d=match subleaf {0=>[0,1,0x100,index],1=>[width,count,0x201,index],_=>[0,0,subleaf&255,index]},
        0x80000001=>d[2]&=!(1<<2), // no nested SVM
        0x80000008=>d[2]=(d[2]&!0xf0ff)|(count-1)|(width<<12),
        0x8000001e=>d=[index,index,0,0],
        _=>return Err("Unexpected topology CPUID exit".into()),
    }
    Ok(d)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn every_vcpu_reports_one_consistent_virtual_package() {
        for count in 1..=8 {for index in 0..count {
            let one=result(1,0,index,count,true,[u32::MAX;4]).unwrap();
            assert_eq!((one[1]>>16)&255,count);assert_eq!(one[1]>>24,index);
            assert_eq!(one[2]&((1<<3)|(1<<5)|(1<<21)|(1<<24)),0);
            assert_eq!(one[3]&(1<<28)!=0,count>1);
            let package=result(0xb,1,index,count,true,[u32::MAX;4]).unwrap();
            assert_eq!((package[1],package[2],package[3]),(count,0x201,index));
            assert!(1<<package[0]>=count);
            assert_eq!(result(0xb,0,index,count,true,[0;4]).unwrap(),[0,1,0x100,index]);
            assert_eq!(result(0xb,2,index,count,true,[0;4]).unwrap(),[0,0,2,index]);
            let amd=result(0x80000008,0,index,count,true,[u32::MAX;4]).unwrap();
            assert_eq!(amd[2]&255,count-1);assert_eq!((amd[2]>>12)&15,package[0]);
        }}
    }
    #[test] fn pic_profile_and_invalid_configuration_do_not_claim_apic_or_nested_virtualization() {
        assert_eq!(result(1,0,0,1,false,[u32::MAX;4]).unwrap()[3]&((1<<9)|(1<<28)),0);
        assert_eq!(result(0x80000001,0,0,1,false,[u32::MAX;4]).unwrap()[2]&(1<<2),0);
        for (index,count) in [(0,0),(0,9),(2,2)] {assert!(result(1,0,index,count,true,[0;4]).is_err());}
    }
    #[test] fn cache_counts_are_clamped_without_inventing_caches() {
        assert_eq!(result(4,0,0,2,true,[0;4]).unwrap(),[0;4]);
        let d=result(4,0,0,2,true,[u32::MAX,2,3,4]).unwrap();
        assert_eq!((d[0]>>26,(d[0]>>14)&0xfff),(1,1));assert_eq!(&d[1..],&[2,3,4]);
    }
}

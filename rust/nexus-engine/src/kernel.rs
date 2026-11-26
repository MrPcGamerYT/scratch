//! Strict subset of Linux x86 boot protocol 2.12..2.15, 32-bit entry.
//! This is a loader, not firmware or a complete PC platform. Inputs must be trusted.
use std::fs::File;
use std::io::Read;
use std::path::Path;
pub const BOOT_PARAMS:usize=0x10000;
pub const CMDLINE:usize=0x20000;
pub const GDT:usize=0x5000;
pub const STACK:u64=0x90000;
pub const DEFAULT_RAM:usize=256*1024*1024;
// Contiguous low RAM must stay below the VirtIO MMIO page at GPA 0xd0000000.
// >3 GiB requires a discontiguous GPA map and matching E820/initrd placement.
pub const MAX_RAM:usize=3072*1024*1024;
pub fn parse_ram_mib(value:&str)->Result<usize,String> {
    if value.is_empty()||!value.bytes().all(|b|b.is_ascii_digit()) {return Err("RAM MiB must be an unsigned decimal integer".into());}
    let mib=value.parse::<usize>().map_err(|_|"RAM MiB overflows host size")?;
    if !(128..=3072).contains(&mib) {return Err("Kernel RAM must be 128..3072 MiB".into());}
    Ok(mib*1024*1024)
}
pub const MAX_INPUT:usize=64*1024*1024;
fn u16_at(b:&[u8],i:usize)->u16 {u16::from_le_bytes(b[i..i+2].try_into().unwrap())}
fn u32_at(b:&[u8],i:usize)->u32 {u32::from_le_bytes(b[i..i+4].try_into().unwrap())}
fn u64_at(b:&[u8],i:usize)->u64 {u64::from_le_bytes(b[i..i+8].try_into().unwrap())}
fn put32(b:&mut [u8],i:usize,n:u32) {b[i..i+4].copy_from_slice(&n.to_le_bytes());}
fn put64(b:&mut [u8],i:usize,n:u64) {b[i..i+8].copy_from_slice(&n.to_le_bytes());}
pub fn read_input(path:&Path)->Result<Vec<u8>,String> {
    let file=File::open(path).map_err(|e|e.to_string())?;
    if !file.metadata().map_err(|e|e.to_string())?.is_file() {return Err("Kernel/initramfs must be a regular file".into());}
    let mut data=Vec::new();file.take((MAX_INPUT+1) as u64).read_to_end(&mut data).map_err(|e|e.to_string())?;
    if data.is_empty()||data.len()>MAX_INPUT {return Err("Kernel/initramfs size must be 1..64 MiB".into());}Ok(data)
}
#[derive(Debug)]
pub struct Region {pub address:usize,pub bytes:Vec<u8>}
#[derive(Debug)]
pub struct KernelPlan {
    pub regions:Vec<Region>, pub ram_bytes:usize,pub entry:u64,pub protocol:u16,
    pub runtime_end:usize,pub initrd_address:Option<usize>,pub framebuffer_output:Option<std::path::PathBuf>,pub android_init_probe:bool,pub android_observe:bool,pub player:Option<std::sync::Arc<crate::player::Session>>,
}
impl KernelPlan {
    pub fn build(image:&[u8],initrd:Option<&[u8]>,command:&str,ram:usize)->Result<Self,String> {
        if !(128*1024*1024..=MAX_RAM).contains(&ram)||ram%4096!=0 {return Err("Kernel RAM must be page-aligned 128..3072 MiB".into());}
        if image.len()<0x268||image.len()>MAX_INPUT {return Err("Truncated/oversized Linux boot image".into());}
        if u16_at(image,0x1fe)!=0xaa55 || &image[0x202..0x206]!=b"HdrS" {return Err("Not an x86 Linux bzImage (AA55/HdrS missing)".into());}
        let protocol=u16_at(image,0x206);
        if !(0x20c..=0x20f).contains(&protocol) {return Err("Supported Linux boot protocols: 2.12..2.15 only".into());}
        let header_end=0x202+usize::from(image[0x201]);
        let setup=(usize::from(if image[0x1f1]==0 {4} else {image[0x1f1]})+1)*512;
        if !(0x264..=0x290).contains(&header_end)||setup<header_end||setup>=image.len() {return Err("Invalid/truncated setup header or protected payload".into());}
        if image[0x211]&1==0||u16_at(image,0x236)&1==0||u32_at(image,0x23c)!=0 {return Err("Only loaded-high x86-64 bzImage on standard PC subarchitecture supported".into());}
        let payload=&image[setup..];let declared=u32_at(image,0x1f4) as usize*16;
        if declared==0||declared>payload.len().saturating_add(15)||payload.len()>declared.saturating_add(16) {return Err("Protected payload length disagrees with syssize".into());}
        let reloc=image[0x234];
        if reloc>1 {return Err("Invalid relocatable_kernel flag".into());}
        let alignment=u32_at(image,0x230) as usize;
        if !alignment.is_power_of_two()||alignment>32*1024*1024 {return Err("Unsupported kernel alignment".into());}
        let (address,runtime_start)=if reloc==1 {
            let address=alignment.max(0x100000);(address,address)
        } else {
            let address=u32_at(image,0x214) as usize;
            let preferred=usize::try_from(u64_at(image,0x258)).map_err(|_|"Kernel runtime address overflow")?;
            if address<0x100000||address%4096!=0||preferred<0x100000||preferred%4096!=0 {
                return Err("Invalid nonrelocatable load/runtime address".into());
            }(address,preferred)
        };
        let runtime=u32_at(image,0x260) as usize;
        let load_end=address.checked_add(payload.len()).ok_or("Kernel payload address overflow")?;
        let runtime_end=runtime_start.checked_add(runtime).ok_or("Kernel runtime overflow")?.max(load_end);
        if runtime<payload.len()||runtime_end>ram {return Err("Kernel runtime/decompression region does not fit RAM".into());}
        let cap=(u32_at(image,0x238) as usize).min(4096);
        if command.as_bytes().contains(&0)||command.len()+1>cap {return Err("Command line contains NUL or exceeds kernel/prototype limit".into());}
        let mut params=vec![0;4096];params[0x1f1..header_end].copy_from_slice(&image[0x1f1..header_end]);
        params[0x210]=0xff;params[0x211]&=!0x60; // Undefined loader; clear QUIET/KEEP_SEGMENTS.
        put32(&mut params,0x214,address as u32);put32(&mut params,0x228,CMDLINE as u32);
        put32(&mut params,0x218,0);put32(&mut params,0x21c,0);put64(&mut params,0x250,0); // no setup_data chain
        params[0x1e8]=3; // boot_params.e820_entries, packed entries of 20 bytes.
        for (i,(start,size,kind)) in [(0u64,0x9fc00u64,1u32),(0x9fc00,0x60400,2),(0x100000,(ram-0x100000) as u64,1)].into_iter().enumerate() {
            let at=0x2d0+i*20;put64(&mut params,at,start);put64(&mut params,at+8,size);put32(&mut params,at+16,kind);
        }
        let mut cmd=command.as_bytes().to_vec();cmd.push(0);
        let mut gdt=vec![0;32];put64(&mut gdt,16,0x00cf9b000000ffff);put64(&mut gdt,24,0x00cf93000000ffff);
        let mut regions=vec![Region{address:BOOT_PARAMS,bytes:params},Region{address:CMDLINE,bytes:cmd},Region{address:GDT,bytes:gdt},Region{address,bytes:payload.to_vec()}];
        let mut initrd_address=None;
        if let Some(data)=initrd {
            if data.is_empty()||data.len()>MAX_INPUT {return Err("Initramfs size must be 1..64 MiB".into());}
            let top=(ram as u64).min(u64::from(u32_at(image,0x22c))+1) as usize;
            let at=top.checked_sub(data.len()).ok_or("Initramfs exceeds address limit")?&!4095;
            if at<runtime_end {return Err("Initramfs overlaps kernel runtime/decompression region".into());}
            put32(&mut regions[0].bytes,0x218,at as u32);put32(&mut regions[0].bytes,0x21c,data.len() as u32);
            regions.push(Region{address:at,bytes:data.to_vec()});initrd_address=Some(at);
        }
        let p=Self{regions,ram_bytes:ram,entry:address as u64,protocol,runtime_end,initrd_address,framebuffer_output:None,android_init_probe:false,android_observe:false,player:None};p.validate_regions()?;Ok(p)
    }
    pub fn with_framebuffer(mut self,path:&Path)->Result<Self,String> {
        let params=self.regions.iter_mut().find(|r|r.address==BOOT_PARAMS).ok_or("Missing boot_params")?;
        crate::framebuffer::configure(&mut params.bytes)?;
        self.framebuffer_output=Some(path.to_path_buf());Ok(self)
    }
    pub fn validate_regions(&self)->Result<(),String> {
        for (i,r) in self.regions.iter().enumerate() {
            let end=r.address.checked_add(r.bytes.len()).ok_or("Kernel region overflow")?;
            if r.bytes.is_empty()||end>self.ram_bytes {return Err("Kernel region outside RAM".into());}
            for other in &self.regions[..i] {
                let other_end=other.address.checked_add(other.bytes.len()).ok_or("Kernel region overflow")?;
                if r.address<other_end&&other.address<end {return Err("Overlapping kernel load regions".into());}
            }
        }Ok(())
    }
}
/// Synthetic handshake image for loader execution tests, NOT a Linux kernel.
pub fn contract_image()->Vec<u8> {
    let mut b=vec![0;2560+4096];b[0x1f1]=4;b[0x201]=0x6a;
    b[0x1fe..0x200].copy_from_slice(&0xaa55u16.to_le_bytes());b[0x202..0x206].copy_from_slice(b"HdrS");
    b[0x206..0x208].copy_from_slice(&0x20fu16.to_le_bytes());b[0x211]=1;b[0x234]=1;
    put32(&mut b,0x1f4,256);put32(&mut b,0x214,0x100000);put32(&mut b,0x22c,0x37ffffff);
    put32(&mut b,0x230,0x200000);b[0x236]=1;put32(&mut b,0x238,2048);put32(&mut b,0x260,0x100000);
    let code=[0x8b,0x86,0x28,0x02,0,0,0x8b,0x18,0x8b,0x8e,0x14,0x02,0,0,
        0x8b,0xbe,0x18,0x02,0,0,0x8b,0x17,0x0f,0xb6,0xae,0xe8,0x01,0,0,0xf4];
    b[2560..2560+code.len()].copy_from_slice(&code);b
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn boot_params_and_layout() {
        let p=KernelPlan::build(&contract_image(),Some(b"INIT"),"nexus",DEFAULT_RAM).unwrap();
        assert_eq!(p.entry,0x200000);assert_eq!(p.runtime_end,0x300000);
        let b=&p.regions[0].bytes;assert_eq!(u32_at(b,0x228),CMDLINE as u32);assert_eq!(b[0x1e8],3);
        assert_eq!(u64_at(b,0x2f8),0x100000);assert_eq!(u64_at(b,0x300),(DEFAULT_RAM-0x100000) as u64);
        assert_eq!(u32_at(b,0x218),p.initrd_address.unwrap() as u32);assert_eq!(u32_at(b,0x21c),4);
        assert_eq!(u64_at(b,0x250),0);assert_eq!(b[0x1ef],0);
        assert_eq!(p.regions[1].bytes,b"nexus\0");assert_eq!(p.regions[2].bytes.len(),32);
    }
    #[test] fn malformed_headers_refused() {
        let original=contract_image();
        for (at,value) in [(0x1fe,0),(0x202,0),(0x206,1),(0x207,3),(0x201,0),(0x211,0),(0x236,0),(0x234,2),(0x23c,1)] {
            let mut b=original.clone();b[at]=value;assert!(KernelPlan::build(&b,None,"",DEFAULT_RAM).is_err(),"offset {at:x}");
        }
        for len in [0,0x100,0x267,2560,3000] {assert!(KernelPlan::build(&original[..len],None,"",DEFAULT_RAM).is_err());}
    }
    #[test] fn bounds_and_initrd_collision() {
        let b=contract_image();assert!(KernelPlan::build(&b,None,"nul\0line",DEFAULT_RAM).is_err());
        assert!(KernelPlan::build(&b,None,&"x".repeat(2048),DEFAULT_RAM).is_err());
        for ram in [0,64*1024*1024,DEFAULT_RAM+1,MAX_RAM+4096] {assert!(KernelPlan::build(&b,None,"",ram).is_err());}
        let mut bad=b.clone();put32(&mut bad,0x260,u32::MAX);assert!(KernelPlan::build(&bad,None,"",DEFAULT_RAM).is_err());
        bad=b.clone();put32(&mut bad,0x230,3);assert!(KernelPlan::build(&bad,None,"",DEFAULT_RAM).is_err());
        bad=b.clone();put32(&mut bad,0x22c,0x200000);assert!(KernelPlan::build(&bad,Some(b"INIT"),"",DEFAULT_RAM).is_err());
        assert!(KernelPlan::build(&b,Some(b""),"",DEFAULT_RAM).is_err());
    }
    #[test] fn larger_ram_e820_initrd_and_mmio_separation() {
        for mib in [128,256,512,1024,2048,3072] {
            let ram=parse_ram_mib(&mib.to_string()).unwrap();
            let p=KernelPlan::build(&contract_image(),Some(b"INIT"),"nexus",ram).unwrap();
            assert_eq!(p.ram_bytes,ram);assert_eq!(u64_at(&p.regions[0].bytes,0x300),(ram-0x100000) as u64);
            let at=p.initrd_address.unwrap();assert!(at+4<=ram&&at+4<=0x38000000&&at>=p.runtime_end);
            assert!(ram<0xd0000000);
        }
        for bad in ["","127","3073","-128","+256"," 256","256 ","1.5","0x100","184467440737095516160"] {assert!(parse_ram_mib(bad).is_err());}
    }
    #[test] fn nonrelocatable_runtime_is_separate_from_compressed_load() {
        let mut image=contract_image();image[0x234]=0;put64(&mut image,0x258,0x1000000);
        let plan=KernelPlan::build(&image,None,"",DEFAULT_RAM).unwrap();
        assert_eq!(plan.entry,0x100000);assert_eq!(plan.runtime_end,0x1100000);
        put64(&mut image,0x258,u64::MAX);assert!(KernelPlan::build(&image,None,"",DEFAULT_RAM).is_err());
    }
    #[test] fn overlap_validation() {
        let mut p=KernelPlan::build(&contract_image(),None,"",DEFAULT_RAM).unwrap();
        p.regions.push(Region{address:BOOT_PARAMS,bytes:vec![1]});assert!(p.validate_regions().is_err());
        p.regions.pop();p.regions.push(Region{address:usize::MAX,bytes:vec![1]});assert!(p.validate_regions().is_err());
    }
}

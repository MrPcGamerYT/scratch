//! Loader for a bounded 16-bit test sector. No BIOS/UEFI or disk-device services.
use crate::image::Image;
use std::path::Path;

pub fn read_test_sector(path:&Path)->Result<[u8;512],String> {
    let mut image=Image::open(path).map_err(|e|e.to_string())?;
    image.verify().map_err(|e|e.to_string())?;
    let mut sector=[0;512];
    image.read_sectors(0,&mut sector).map_err(|e|e.to_string())?;
    validate_signature(&sector)?;
    Ok(sector)
}
fn validate_signature(sector:&[u8;512])->Result<(),String> {
    if sector[510..]!=[0x55,0xaa] {return Err("Test sector requires 55AA signature; no BIOS/UEFI services are implemented".into());}
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn requires_boot_signature() {
        let mut sector=[0;512];assert!(validate_signature(&sector).is_err());
        sector[510]=0x55;sector[511]=0xaa;assert!(validate_signature(&sector).is_ok());
        sector[510]=0xaa;sector[511]=0x55;assert!(validate_signature(&sector).is_err());
    }
}

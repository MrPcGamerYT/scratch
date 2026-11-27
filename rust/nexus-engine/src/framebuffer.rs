//! Fixed, unaccelerated guest scanout surface. This is not a virtual GPU or renderer.
//! Mapped separately from ordinary RAM and VirtIO registers; captures require a stopped vCPU.
use std::{fs::OpenOptions,io::Write,path::Path};
pub const GPA:u64=0xc0000000;
pub const WIDTH:usize=800;
pub const HEIGHT:usize=600;
pub const STRIDE:usize=WIDTH*4;
pub const PIXEL_BYTES:usize=STRIDE*HEIGHT;
pub const MAPPED_BYTES:usize=PIXEL_BYTES.div_ceil(65536)*65536;
/// Linux screen_info ABI: 32-bit XRGB, little-endian B,G,R,X pixels.
pub fn configure(params:&mut [u8])->Result<(),String> {
    if params.len()!=4096 {return Err("Framebuffer requires a complete Linux boot_params page".into());}
    params[..64].fill(0);
    params[0x0f]=0x23; // VIDEO_TYPE_VLFB; sysfb may expose this via simplefb.
    for (at,value) in [(0x12,WIDTH as u16),(0x14,HEIGHT as u16),(0x16,32),(0x24,STRIDE as u16)] {params[at..at+2].copy_from_slice(&value.to_le_bytes());}
    params[0x18..0x1c].copy_from_slice(&(GPA as u32).to_le_bytes());
    params[0x1c..0x20].copy_from_slice(&((MAPPED_BYTES/65536) as u32).to_le_bytes());
    params[0x26..0x2e].copy_from_slice(&[8,16,8,8,8,0,8,24]);
    params[0x34..0x36].copy_from_slice(&0x80u16.to_le_bytes()); // linear framebuffer
    params[0x1e8]=4;
    let at=0x2d0+3*20;
    params[at..at+8].copy_from_slice(&GPA.to_le_bytes());
    params[at+8..at+16].copy_from_slice(&(MAPPED_BYTES as u64).to_le_bytes());
    params[at+16..at+20].copy_from_slice(&2u32.to_le_bytes()); // E820 reserved, not allocator RAM
    Ok(())
}
pub fn rgb(pixels:&[u8])->Result<Vec<u8>,String> {
    if pixels.len()!=MAPPED_BYTES {return Err("Invalid framebuffer mapping length".into());}
    let mut bytes=Vec::with_capacity(WIDTH*HEIGHT*3);
    for pixel in pixels[..PIXEL_BYTES].chunks_exact(4) {bytes.extend_from_slice(&[pixel[2],pixel[1],pixel[0]]);}
    Ok(bytes)
}
pub fn write_capture(file:&mut dyn Write,pixels:&[u8])->Result<(),String> {
    let rgb=rgb(pixels)?;
    write!(file,"P6\n{WIDTH} {HEIGHT}\n255\n").map_err(|e|e.to_string())?;
    file.write_all(&rgb).map_err(|e|e.to_string())?;
    file.flush().map_err(|e|e.to_string())
}
pub fn capture(path:&Path,pixels:&[u8])->Result<(),String> {
    if pixels.len()!=MAPPED_BYTES {return Err("Invalid framebuffer mapping length".into());}
    let mut file=OpenOptions::new().write(true).create_new(true).open(path).map_err(|e|e.to_string())?;
    write_capture(&mut file,pixels)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn screen_info_and_reserved_surface() {
        let mut p=[0;4096];configure(&mut p).unwrap();
        assert_eq!(p[0x0f],0x23);assert_eq!(&p[0x12..0x18],&[0x20,3,0x58,2,32,0]);
        assert_eq!(u32::from_le_bytes(p[0x18..0x1c].try_into().unwrap()),GPA as u32);
        assert_eq!(p[0x1e8],4);assert!(GPA>=crate::kernel::MAX_RAM as u64);
        assert!(GPA+MAPPED_BYTES as u64<=0xd0000000);assert_eq!(MAPPED_BYTES%4096,0);
        assert!(configure(&mut [0;64]).is_err());
    }
    #[test] fn exact_rgb_channels_ignore_x_and_mapping_padding() {
        let mut p=vec![0;MAPPED_BYTES];p[..8].copy_from_slice(&[1,2,3,255,6,5,4,90]);p[PIXEL_BYTES..].fill(99);
        let bytes=rgb(&p).unwrap();assert_eq!(&bytes[..6],&[3,2,1,4,5,6]);assert_eq!(bytes.len(),WIDTH*HEIGHT*3);
        assert!(rgb(&p[..PIXEL_BYTES]).is_err());assert!(rgb(&[]).is_err());
    }
    #[test] fn capture_refuses_overwrite() {
        let path=std::env::temp_dir().join(format!("nexus-scanout-{}.ppm",std::process::id()));
        let pixels=vec![0;MAPPED_BYTES];capture(&path,&pixels).unwrap();let saved=std::fs::read(&path).unwrap();
        assert!(capture(&path,&pixels).is_err());assert_eq!(std::fs::read(&path).unwrap(),saved);std::fs::remove_file(path).unwrap();
    }
}

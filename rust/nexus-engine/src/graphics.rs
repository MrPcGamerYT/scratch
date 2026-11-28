//! Direct3D 11 host-renderer validation. Not a guest GPU or Android acceleration.
//! Hardware and WARP are explicit, separate choices; never silently substitute WARP.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Backend {Hardware,Warp}
pub struct Rendered {pub pixels:Vec<u8>,pub feature_level:u32,pub adapter:String,pub vendor_id:u32,pub device_id:u32,pub software_adapter:bool,pub adapter_flags:u32}
pub fn is_software_adapter(vendor:u32,device:u32,flags:u32,name:&str)->bool {
    flags&2!=0||(vendor==0x1414&&device==0x8c)||name.to_ascii_lowercase().contains("basic render driver")
}
pub const WIDTH:usize=64;
pub const HEIGHT:usize=64;
pub fn reference()->Vec<u8> {
    (0..HEIGHT).flat_map(|y|(0..WIDTH).flat_map(move |x|[(x%4*85) as u8,(y%4*85) as u8,((x+y)%4*85) as u8,255])).collect()
}
#[cfg(not(windows))]
pub fn render(_:Backend)->Result<Rendered,String> {Err("Direct3D host renderer requires Windows".into())}
#[cfg(windows)]
pub use windows::render;

/// Present a VirtIO-GPU scanout surface through the host D3D11 texture pipeline:
/// upload the surface pixels to a CPU-writable staging texture, copy through a
/// GPU-side render-target texture, then read back and return the pixels. This is
/// the host transport path for guest GPU scanout -- not a guest GPU or Android
/// acceleration. `Backend` selects the D3D11 device (WARP for deterministic
/// transport validation; Hardware requires a non-software adapter).
#[cfg(not(windows))]
pub fn present(_:Backend,_:&crate::virtio_gpu::Surface)->Result<Rendered,String> {Err("Direct3D host presenter requires Windows".into())}
#[cfg(windows)]
pub use windows::present;
#[cfg(windows)]
mod windows {
    use super::*;
    use std::ffi::c_void;
    type Ptr=*mut c_void;
    #[repr(C)] struct Guid {a:u32,b:u16,c:u16,d:[u8;8]}
    #[repr(C)] struct AdapterDesc {description:[u16;128],vendor:u32,device:u32,subsystem:u32,revision:u32,video:usize,system:usize,shared:usize,luid:[u32;2],flags:u32}
    #[repr(C)] struct TextureDesc {width:u32,height:u32,mips:u32,array:u32,format:u32,samples:u32,quality:u32,usage:u32,bind:u32,cpu:u32,misc:u32}
    #[repr(C)] struct Mapped {data:Ptr,row_pitch:u32,depth_pitch:u32}
    #[repr(C)] struct Viewport {x:f32,y:f32,width:f32,height:f32,min:f32,max:f32}
    #[link(name="d3d11")] extern "system" {
        fn D3D11CreateDevice(adapter:Ptr,driver:u32,software:Ptr,flags:u32,levels:*const u32,count:u32,sdk:u32,device:*mut Ptr,level:*mut u32,context:*mut Ptr)->i32;
    }
    #[link(name="d3dcompiler")] extern "system" {
        fn D3DCompile(source:*const u8,bytes:usize,name:*const u8,defines:Ptr,include:Ptr,entry:*const u8,target:*const u8,flags:u32,effect_flags:u32,code:*mut Ptr,errors:*mut Ptr)->i32;
    }
    struct Com(Ptr);
    impl Com {
        unsafe fn method(&self,index:usize)->*const c_void {*(*self.0.cast::<*const *const c_void>()).add(index)}
        unsafe fn bytes(&self)->&[u8] {
            let data:unsafe extern "system" fn(Ptr)->Ptr=std::mem::transmute(self.method(3));
            let size:unsafe extern "system" fn(Ptr)->usize=std::mem::transmute(self.method(4));
            std::slice::from_raw_parts(data(self.0).cast(),size(self.0))
        }
    }
    impl Drop for Com {
        fn drop(&mut self) {if !self.0.is_null() {unsafe {let release:unsafe extern "system" fn(Ptr)->u32=std::mem::transmute(self.method(2));release(self.0);}}}
    }
    fn check(hr:i32,stage:&str)->Result<(),String> {if hr<0 {Err(format!("D3D11 {stage} failed HRESULT=0x{:08x}",hr as u32))} else {Ok(())}}
    unsafe fn shader(source:&[u8],target:&[u8])->Result<Com,String> {
        let mut code=Com(std::ptr::null_mut());let mut errors=Com(std::ptr::null_mut());
        let hr=D3DCompile(source.as_ptr(),source.len(),std::ptr::null(),std::ptr::null_mut(),std::ptr::null_mut(),c"main".as_ptr().cast(),target.as_ptr(),0x800,0,&mut code.0,&mut errors.0);
        if hr<0 {return Err(format!("D3DCompile HRESULT=0x{:08x}: {}",hr as u32,if errors.0.is_null() {String::new()} else {String::from_utf8_lossy(errors.bytes()).into_owned()}));}
        Ok(code)
    }
    pub fn render(backend:Backend)->Result<Rendered,String> {
        let null=std::ptr::null_mut();
        unsafe {
            let mut device=Com(null);let mut context=Com(null);let mut level=0;
            let levels=[0xb000,0xa100,0xa000];
            let hr=D3D11CreateDevice(null,if backend==Backend::Hardware {1} else {5},null,0x20,levels.as_ptr(),levels.len() as u32,7,&mut device.0,&mut level,&mut context.0);
            if hr<0 {return Err(format!("GRAPHICS_DEVICE_UNAVAILABLE backend={backend:?} HRESULT=0x{:08x}",hr as u32));}
            let query:unsafe extern "system" fn(Ptr,*const Guid,*mut Ptr)->i32=std::mem::transmute(device.method(0));
            let mut dxgi=Com(null);
            check(query(device.0,&Guid{a:0x54ec77fa,b:0x1377,c:0x44e6,d:[0x8c,0x32,0x88,0xfd,0x5f,0x44,0xc8,0x4c]},&mut dxgi.0),"DXGI device query")?;
            let get_adapter:unsafe extern "system" fn(Ptr,*mut Ptr)->i32=std::mem::transmute(dxgi.method(7));
            let mut adapter=Com(null);check(get_adapter(dxgi.0,&mut adapter.0),"adapter query")?;
            let query_adapter:unsafe extern "system" fn(Ptr,*const Guid,*mut Ptr)->i32=std::mem::transmute(adapter.method(0));
            let mut adapter1=Com(null);
            check(query_adapter(adapter.0,&Guid{a:0x29038f61,b:0x3839,c:0x4626,d:[0x91,0xfd,0x08,0x68,0x79,0x01,0x1a,0x05]},&mut adapter1.0),"adapter1 query")?;
            let describe:unsafe extern "system" fn(Ptr,*mut AdapterDesc)->i32=std::mem::transmute(adapter1.method(10));
            let mut adapter_desc:AdapterDesc=std::mem::zeroed();check(describe(adapter1.0,&mut adapter_desc),"adapter description")?;
            let adapter_name=String::from_utf16_lossy(&adapter_desc.description[..adapter_desc.description.iter().position(|x|*x==0).unwrap_or(128)]);
            // Some virtualized Windows runners report Flags=0 for Basic Render Driver.
            // Never certify that known CPU renderer as a physical hardware path.
            let software_adapter=is_software_adapter(adapter_desc.vendor,adapter_desc.device,adapter_desc.flags,&adapter_name);
            if backend==Backend::Hardware&&software_adapter {return Err(format!("GRAPHICS_DEVICE_UNAVAILABLE backend=Hardware HRESULT=0x887a0004 software DXGI adapter: {adapter_name}"));}
            let vertex=shader(b"float4 main(uint id:SV_VertexID):SV_Position { float2 p[3]={float2(-1,-1),float2(-1,3),float2(3,-1)}; return float4(p[id],0,1); }",c"vs_4_0".to_bytes_with_nul())?;
            let pixel=shader(b"float4 main(float4 pos:SV_Position):SV_Target { uint2 p=uint2(pos.xy); return float4(float(p.x%4)/3,float(p.y%4)/3,float((p.x+p.y)%4)/3,1); }",c"ps_4_0".to_bytes_with_nul())?;
            let mut vs=Com(null);let mut ps=Com(null);
            let create_vs:unsafe extern "system" fn(Ptr,*const u8,usize,Ptr,*mut Ptr)->i32=std::mem::transmute(device.method(12));
            let create_ps:unsafe extern "system" fn(Ptr,*const u8,usize,Ptr,*mut Ptr)->i32=std::mem::transmute(device.method(15));
            check(create_vs(device.0,vertex.bytes().as_ptr(),vertex.bytes().len(),null,&mut vs.0),"vertex shader")?;
            check(create_ps(device.0,pixel.bytes().as_ptr(),pixel.bytes().len(),null,&mut ps.0),"pixel shader")?;
            let mut target=Com(null);let mut staging=Com(null);let mut view=Com(null);
            let texture:unsafe extern "system" fn(Ptr,*const TextureDesc,Ptr,*mut Ptr)->i32=std::mem::transmute(device.method(5));
            let mut desc=TextureDesc{width:WIDTH as u32,height:HEIGHT as u32,mips:1,array:1,format:28,samples:1,quality:0,usage:0,bind:0x20,cpu:0,misc:0};
            check(texture(device.0,&desc,null,&mut target.0),"render target allocation")?;
            desc.usage=3;desc.bind=0;desc.cpu=0x20000;
            check(texture(device.0,&desc,null,&mut staging.0),"readback allocation")?;
            let rtv:unsafe extern "system" fn(Ptr,Ptr,Ptr,*mut Ptr)->i32=std::mem::transmute(device.method(9));
            check(rtv(device.0,target.0,null,&mut view.0),"render target view")?;
            let set_targets:unsafe extern "system" fn(Ptr,u32,*const Ptr,Ptr)=std::mem::transmute(context.method(33));
            set_targets(context.0,1,&view.0,null);
            let set_viewport:unsafe extern "system" fn(Ptr,u32,*const Viewport)=std::mem::transmute(context.method(44));
            set_viewport(context.0,1,&Viewport{x:0.,y:0.,width:WIDTH as f32,height:HEIGHT as f32,min:0.,max:1.});
            let set_vs:unsafe extern "system" fn(Ptr,Ptr,*const Ptr,u32)=std::mem::transmute(context.method(11));
            let set_ps:unsafe extern "system" fn(Ptr,Ptr,*const Ptr,u32)=std::mem::transmute(context.method(9));
            set_vs(context.0,vs.0,std::ptr::null(),0);set_ps(context.0,ps.0,std::ptr::null(),0);
            let topology:unsafe extern "system" fn(Ptr,u32)=std::mem::transmute(context.method(24));topology(context.0,4);
            // Clear to a color that cannot satisfy the asymmetric shader oracle.
            let clear:unsafe extern "system" fn(Ptr,Ptr,*const f32)=std::mem::transmute(context.method(50));clear(context.0,view.0,[1.,0.,1.,0.].as_ptr());
            let draw:unsafe extern "system" fn(Ptr,u32,u32)=std::mem::transmute(context.method(13));draw(context.0,3,0);
            let copy:unsafe extern "system" fn(Ptr,Ptr,Ptr)=std::mem::transmute(context.method(47));copy(context.0,staging.0,target.0);
            let map:unsafe extern "system" fn(Ptr,Ptr,u32,u32,u32,*mut Mapped)->i32=std::mem::transmute(context.method(14));
            let unmap:unsafe extern "system" fn(Ptr,Ptr,u32)=std::mem::transmute(context.method(15));
            let mut mapped=Mapped{data:null,row_pitch:0,depth_pitch:0};check(map(context.0,staging.0,0,1,0,&mut mapped),"synchronized readback")?;
            if mapped.data.is_null()||mapped.row_pitch<(WIDTH*4) as u32 {unmap(context.0,staging.0,0);return Err("D3D11 invalid mapped row pitch".into());}
            let mut pixels=Vec::with_capacity(WIDTH*HEIGHT*4);
            for row in 0..HEIGHT {pixels.extend_from_slice(std::slice::from_raw_parts(mapped.data.cast::<u8>().add(row*mapped.row_pitch as usize),WIDTH*4));}
            unmap(context.0,staging.0,0);
            let removed:unsafe extern "system" fn(Ptr)->i32=std::mem::transmute(device.method(39));check(removed(device.0),"device status")?;
            if pixels!=reference() {return Err("D3D11 shader/readback pixel mismatch".into());}
            Ok(Rendered{pixels,feature_level:level,adapter:adapter_name,vendor_id:adapter_desc.vendor,device_id:adapter_desc.device,software_adapter,adapter_flags:adapter_desc.flags})
        }
    }

    /// Upload a VirtIO-GPU scanout surface through the D3D11 texture pipeline
    /// and read it back. Exercises the host transport path: CPU staging texture
    /// -> GPU render-target texture -> CPU readback, byte-preserving for a
    /// same-format (R8G8B8A8_UNORM) copy.
    pub fn present(backend:Backend,surface:&crate::virtio_gpu::Surface)->Result<Rendered,String> {
            let null=std::ptr::null_mut();
            unsafe {
                let mut device=Com(null);let mut context=Com(null);let mut level=0;
                let levels=[0xb000,0xa100,0xa000];
                let hr=D3D11CreateDevice(null,if backend==Backend::Hardware {1} else {5},null,0x20,levels.as_ptr(),levels.len() as u32,7,&mut device.0,&mut level,&mut context.0);
                if hr<0 {return Err(format!("GRAPHICS_DEVICE_UNAVAILABLE backend={backend:?} HRESULT=0x{:08x}",hr as u32));}
                let query:unsafe extern "system" fn(Ptr,*const Guid,*mut Ptr)->i32=std::mem::transmute(device.method(0));
                let mut dxgi=Com(null);
                check(query(device.0,&Guid{a:0x54ec77fa,b:0x1377,c:0x44e6,d:[0x8c,0x32,0x88,0xfd,0x5f,0x44,0xc8,0x4c]},&mut dxgi.0),"DXGI device query")?;
                let get_adapter:unsafe extern "system" fn(Ptr,*mut Ptr)->i32=std::mem::transmute(dxgi.method(7));
                let mut adapter=Com(null);check(get_adapter(dxgi.0,&mut adapter.0),"adapter query")?;
                let query_adapter:unsafe extern "system" fn(Ptr,*const Guid,*mut Ptr)->i32=std::mem::transmute(adapter.method(0));
                let mut adapter1=Com(null);
                check(query_adapter(adapter.0,&Guid{a:0x29038f61,b:0x3839,c:0x4626,d:[0x91,0xfd,0x08,0x68,0x79,0x01,0x1a,0x05]},&mut adapter1.0),"adapter1 query")?;
                let describe:unsafe extern "system" fn(Ptr,*mut AdapterDesc)->i32=std::mem::transmute(adapter1.method(10));
                let mut adapter_desc:AdapterDesc=std::mem::zeroed();check(describe(adapter1.0,&mut adapter_desc),"adapter description")?;
                let adapter_name=String::from_utf16_lossy(&adapter_desc.description[..adapter_desc.description.iter().position(|x|*x==0).unwrap_or(128)]);
                let software_adapter=is_software_adapter(adapter_desc.vendor,adapter_desc.device,adapter_desc.flags,&adapter_name);
                if backend==Backend::Hardware&&software_adapter {return Err(format!("GRAPHICS_DEVICE_UNAVAILABLE backend=Hardware HRESULT=0x887a0004 software DXGI adapter: {adapter_name}"));}
                let width=surface.width;let height=surface.height;
                if width==0||height==0 {return Err("Present surface has invalid geometry".into());}
                let texture:unsafe extern "system" fn(Ptr,*const TextureDesc,Ptr,*mut Ptr)->i32=std::mem::transmute(device.method(5));
                let mut staging=Com(null);
                let mut desc=TextureDesc{width,height,mips:1,array:1,format:28,samples:1,quality:0,usage:3,bind:0,cpu:0x20000,misc:0};
                check(texture(device.0,&desc,null,&mut staging.0),"staging texture")?;
                let mut target=Com(null);
                desc.usage=0;desc.bind=0x20;desc.cpu=0;
                check(texture(device.0,&desc,null,&mut target.0),"render target texture")?;
                let mut readback=Com(null);
                desc.usage=3;desc.bind=0;desc.cpu=0x20000;
                check(texture(device.0,&desc,null,&mut readback.0),"readback texture")?;
                let copy:unsafe extern "system" fn(Ptr,Ptr,Ptr)=std::mem::transmute(context.method(47));
                let map:unsafe extern "system" fn(Ptr,Ptr,u32,u32,u32,*mut Mapped)->i32=std::mem::transmute(context.method(14));
                let unmap:unsafe extern "system" fn(Ptr,Ptr,u32)=std::mem::transmute(context.method(15));
                let row_bytes=(width*4) as usize;
                let mut mapped=Mapped{data:null,row_pitch:0,depth_pitch:0};
                check(map(context.0,staging.0,0,1,0,&mut mapped),"staging map")?;
                if mapped.data.is_null()||mapped.row_pitch<width*4 {unmap(context.0,staging.0,0);return Err("D3D11 invalid staging row pitch".into());}
                for row in 0..height as usize {
                    let src=row*surface.pitch as usize;let dst=row*mapped.row_pitch as usize;
                    std::ptr::copy_nonoverlapping(surface.data.as_ptr().add(src),mapped.data.cast::<u8>().add(dst),row_bytes);
                }
                unmap(context.0,staging.0,0);
                // staging -> render target -> readback (byte-preserving same-format copy).
                copy(context.0,target.0,staging.0);
                copy(context.0,readback.0,target.0);
                let mut mapped=Mapped{data:null,row_pitch:0,depth_pitch:0};
                check(map(context.0,readback.0,0,1,0,&mut mapped),"readback map")?;
                if mapped.data.is_null()||mapped.row_pitch<width*4 {unmap(context.0,readback.0,0);return Err("D3D11 invalid readback row pitch".into());}
                let mut pixels=Vec::with_capacity((width*height*4) as usize);
                for row in 0..height as usize {pixels.extend_from_slice(std::slice::from_raw_parts(mapped.data.cast::<u8>().add(row*mapped.row_pitch as usize),row_bytes));}
                unmap(context.0,readback.0,0);
                let removed:unsafe extern "system" fn(Ptr)->i32=std::mem::transmute(device.method(39));check(removed(device.0),"device status")?;
                Ok(Rendered{pixels,feature_level:level,adapter:adapter_name,vendor_id:adapter_desc.vendor,device_id:adapter_desc.device,software_adapter,adapter_flags:adapter_desc.flags})
            }
        }
    #[cfg(test)] mod tests {
        use super::*;
        #[test] fn ffi_layout() {assert_eq!(std::mem::size_of::<AdapterDesc>(),312);assert_eq!(std::mem::offset_of!(AdapterDesc,flags),304);assert_eq!(std::mem::size_of::<TextureDesc>(),44);assert_eq!(std::mem::size_of::<Mapped>(),16);assert_eq!(std::mem::size_of::<Viewport>(),24);}
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn reject_known_cpu_renderer_even_when_dxgi_software_flag_is_missing() {
        assert!(is_software_adapter(0x1414,0x8c,0,"adapter"));
        assert!(is_software_adapter(0,0,0,"Microsoft Basic Render Driver"));
        assert!(is_software_adapter(0x8086,0x1912,2,"anything"));
        assert!(!is_software_adapter(0x8086,0x1912,0,"Intel HD Graphics 530"));
        assert!(!is_software_adapter(0x10de,0x1234,0,"example adapter"));
    }
    #[test] fn asymmetric_reference() {let p=reference();assert_eq!(p.len(),64*64*4);assert_eq!(&p[..8],&[0,0,0,255,85,0,85,255]);assert_eq!(&p[256..260],&[0,85,85,255]);assert_ne!(&p[..256],&p[256..512]);}
}

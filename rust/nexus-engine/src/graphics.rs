//! Direct3D 11 host-renderer validation. Not a guest GPU or Android acceleration.
//! Hardware and WARP are explicit, separate choices; never silently substitute WARP.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Backend {Hardware,Warp}
pub const WIDTH:usize=64;
pub const HEIGHT:usize=64;
pub fn reference()->Vec<u8> {
    (0..HEIGHT).flat_map(|y|(0..WIDTH).flat_map(move |x|[(x%4*85) as u8,(y%4*85) as u8,((x+y)%4*85) as u8,255])).collect()
}
#[cfg(not(windows))]
pub fn render(_:Backend)->Result<(Vec<u8>,u32),String> {Err("Direct3D host renderer requires Windows".into())}
#[cfg(windows)]
pub use windows::render;
#[cfg(windows)]
mod windows {
    use super::*;
    use std::ffi::c_void;
    type Ptr=*mut c_void;
    #[repr(C)] struct TextureDesc {width:u32,height:u32,mips:u32,array:u32,format:u32,samples:u32,quality:u32,usage:u32,bind:u32,cpu:u32,misc:u32}
    #[repr(C)] struct Mapped {data:Ptr,row_pitch:u32,depth_pitch:u32}
    #[repr(C)] struct Viewport {x:f32,y:f32,width:f32,height:f32,min:f32,max:f32}
    #[link(name="d3d11")] extern "system" {
        fn D3D11CreateDevice(adapter:Ptr,driver:u32,software:Ptr,flags:u32,levels:*const u32,count:u32,sdk:u32,device:*mut Ptr,level:*mut u32,context:*mut Ptr)->i32;
    }
    #[link(name="d3dcompiler_47")] extern "system" {
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
    pub fn render(backend:Backend)->Result<(Vec<u8>,u32),String> {
        let null=std::ptr::null_mut();
        unsafe {
            let mut device=Com(null);let mut context=Com(null);let mut level=0;
            let levels=[0xb000,0xa100,0xa000];
            let hr=D3D11CreateDevice(null,if backend==Backend::Hardware {1} else {5},null,0x20,levels.as_ptr(),levels.len() as u32,7,&mut device.0,&mut level,&mut context.0);
            if hr<0 {return Err(format!("GRAPHICS_DEVICE_UNAVAILABLE backend={backend:?} HRESULT=0x{:08x}",hr as u32));}
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
            Ok((pixels,level))
        }
    }
    #[cfg(test)] mod tests {
        use super::*;
        #[test] fn ffi_layout() {assert_eq!(std::mem::size_of::<TextureDesc>(),44);assert_eq!(std::mem::size_of::<Mapped>(),16);assert_eq!(std::mem::size_of::<Viewport>(),24);}
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn asymmetric_reference() {let p=reference();assert_eq!(p.len(),64*64*4);assert_eq!(&p[..8],&[0,0,0,255,85,0,85,255]);assert_eq!(&p[256..260],&[0,85,85,255]);assert_ne!(&p[..256],&p[256..512]);}
}

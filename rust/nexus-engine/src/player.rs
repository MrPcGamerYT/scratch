//! Native software presentation. UI reads owned snapshots, never live guest RAM.
use std::sync::{atomic::{AtomicBool,AtomicU64,AtomicUsize,Ordering},Arc,Mutex};
#[derive(Debug)]
pub struct Session {pub stop:AtomicBool,hwnd:AtomicUsize,frame:Mutex<Vec<u8>>,input:Mutex<std::collections::VecDeque<u8>>,pub presents:AtomicU64,pub presented_crc32:AtomicU64}
impl Default for Session {
    fn default()->Self {Self{stop:AtomicBool::new(false),hwnd:AtomicUsize::new(0),input:Mutex::new(std::collections::VecDeque::new()),frame:Mutex::new(vec![0;crate::framebuffer::PIXEL_BYTES]),presents:AtomicU64::new(0),presented_crc32:AtomicU64::new(0)}}
}
impl Session {
    pub fn publish(&self,pixels:&[u8])->Result<(),String> {
        if pixels.len()!=crate::framebuffer::MAPPED_BYTES {return Err("Invalid live scanout length".into());}
        self.frame.lock().map_err(|_|"Player snapshot lock poisoned")?.copy_from_slice(&pixels[..crate::framebuffer::PIXEL_BYTES]);Ok(())
    }
    pub fn type_byte(&self,byte:u8)->Result<(),String> {
        if !(byte.is_ascii_graphic()||matches!(byte,b' '|b'\r'|b'\n'|8|127|9)) {return Err("Console input supports ASCII text only".into());}
        let mut input=self.input.lock().map_err(|_|"Player input lock poisoned")?;
        if input.len()>=256 {return Err("Player console input queue full".into());}input.push_back(byte);Ok(())
    }
    pub fn next_byte(&self)->Option<u8> {self.input.lock().ok()?.pop_front()}
    pub fn stopped(&self)->bool {self.stop.load(Ordering::Acquire)}
}
/// Aspect-preserving destination. Coordinates outside this rectangle are letterbox.
pub fn viewport(width:i32,height:i32)->(i32,i32,i32,i32) {
    if width<=0||height<=0 {return (0,0,0,0);}
    let w=i64::from(width).min(i64::from(height)*4/3) as i32;
    let h=(i64::from(w)*3/4) as i32;((width-w)/2,(height-h)/2,w,h)
}
pub struct Window {pub session:Arc<Session>,thread:Option<std::thread::JoinHandle<Result<(),String>>>}
impl Window {
    pub fn request_close(&self)->Result<(),String> {native::close(&self.session)}
    pub fn open()->Result<Self,String> {
        let session=Arc::new(Session::default());let shared=session.clone();
        let (tx,rx)=std::sync::mpsc::sync_channel(1);
        let thread=std::thread::spawn(move ||native::run(shared,tx));
        match rx.recv() {
            Ok(Ok(()))=>Ok(Self{session,thread:Some(thread)}),
            _=>{let _=thread.join();Err("Native player window could not initialize".into())}
        }
    }
}
impl Drop for Window {
    fn drop(&mut self) {self.session.stop.store(true,Ordering::Release);if let Some(t)=self.thread.take() {let _=t.join();}}
}
#[cfg(not(windows))]
mod native {
    use super::*;
    pub fn close(s:&Session)->Result<(),String> {let _=s.hwnd.load(Ordering::Relaxed);Err("Native player requires Windows".into())}
    pub fn run(_:Arc<Session>,ready:std::sync::mpsc::SyncSender<Result<(),String>>)->Result<(),String> {let _=ready.send(Err("Native player requires Windows".into()));Err("Native player requires Windows".into())}
}
#[cfg(windows)]
mod native {
    use super::*;
    use std::ffi::c_void;
    type Handle=*mut c_void;
    #[repr(C)] struct Point{x:i32,y:i32}
    #[repr(C)] struct Msg{hwnd:Handle,message:u32,wparam:usize,lparam:isize,time:u32,pt:Point,private:u32}
    #[repr(C)] struct Class{style:u32,proc:unsafe extern "system" fn(Handle,u32,usize,isize)->isize,extra:i32,window_extra:i32,instance:Handle,icon:Handle,cursor:Handle,background:Handle,menu:*const u16,name:*const u16}
    #[repr(C)] struct Rect{left:i32,top:i32,right:i32,bottom:i32}
    #[repr(C)] struct Bitmap{size:u32,width:i32,height:i32,planes:u16,bits:u16,compression:u32,image:u32,x:i32,y:i32,used:u32,important:u32}
    #[link(name="user32")] extern "system" {
        fn RegisterClassW(c:*const Class)->u16;
        fn UnregisterClassW(name:*const u16,instance:Handle)->i32;
        fn CreateWindowExW(ex:u32,class:*const u16,title:*const u16,style:u32,x:i32,y:i32,w:i32,h:i32,parent:Handle,menu:Handle,instance:Handle,param:Handle)->Handle;
        fn DefWindowProcW(h:Handle,m:u32,w:usize,l:isize)->isize;
        fn DestroyWindow(h:Handle)->i32;
        fn PostQuitMessage(code:i32);
        fn PostMessageW(h:Handle,m:u32,w:usize,l:isize)->i32;
        fn GetMessageW(msg:*mut Msg,h:Handle,min:u32,max:u32)->i32;
        fn TranslateMessage(msg:*const Msg)->i32;
        fn DispatchMessageW(msg:*const Msg)->isize;
        fn SetTimer(h:Handle,id:usize,ms:u32,callback:Handle)->usize;
        fn KillTimer(h:Handle,id:usize)->i32;
        fn GetClientRect(h:Handle,rect:*mut Rect)->i32;
        fn GetDC(h:Handle)->Handle;
        fn ReleaseDC(h:Handle,dc:Handle)->i32;
        fn LoadCursorW(instance:Handle,name:*const u16)->Handle;
        fn FillRect(dc:Handle,r:*const Rect,brush:Handle)->i32;
    }
    #[link(name="gdi32")] extern "system" {
        fn GetStockObject(index:i32)->Handle;
        fn StretchDIBits(dc:Handle,x:i32,y:i32,w:i32,h:i32,sx:i32,sy:i32,sw:i32,sh:i32,bits:*const c_void,info:*const Bitmap,usage:u32,rop:u32)->i32;
    }
    #[link(name="kernel32")] extern "system" {fn GetModuleHandleW(name:*const u16)->Handle;}
    unsafe extern "system" fn proc(h:Handle,m:u32,w:usize,l:isize)->isize {
        if m==2 {PostQuitMessage(0);return 0;}DefWindowProcW(h,m,w,l)
    }
    pub fn close(s:&Session)->Result<(),String> {
        let h=s.hwnd.load(Ordering::Acquire) as Handle;
        if h.is_null()||unsafe {PostMessageW(h,0x10,0,0)}==0 {return Err("Player close request failed".into());}Ok(())
    }
    pub fn run(shared:Arc<Session>,ready:std::sync::mpsc::SyncSender<Result<(),String>>)->Result<(),String> {
        let null=std::ptr::null_mut();let class:Vec<u16>="NexusNativeScanout\0".encode_utf16().collect();
        let title:Vec<u16>="Nexus — native software player (Android display not verified)\0".encode_utf16().collect();
        unsafe {
            let instance=GetModuleHandleW(std::ptr::null());
            let c=Class{style:3,proc,extra:0,window_extra:0,instance,icon:null,cursor:LoadCursorW(null,32512usize as *const u16),background:GetStockObject(4),menu:std::ptr::null(),name:class.as_ptr()};
            if RegisterClassW(&c)==0 {return Err("RegisterClassW failed".into());}
            let h=CreateWindowExW(0,class.as_ptr(),title.as_ptr(),0x10cf0000,i32::MIN,i32::MIN,840,660,null,null,instance,null);
            if h.is_null() {UnregisterClassW(class.as_ptr(),instance);return Err("CreateWindowExW failed".into());}
            if SetTimer(h,1,33,null)==0 {DestroyWindow(h);UnregisterClassW(class.as_ptr(),instance);return Err("Player timer failed".into());}
            shared.hwnd.store(h as usize,Ordering::Release);
            let _=ready.send(Ok(()));
            let mut msg:Msg=std::mem::zeroed();let mut error=None;
            loop {
                let status=GetMessageW(&mut msg,null,0,0);
                if status<=0 {if status<0 {error=Some("Player message loop failed".to_string());}break;}
                if shared.stopped() {DestroyWindow(h);break;}
                if msg.message==0x102&&msg.wparam<=127 {let _=shared.type_byte(msg.wparam as u8);}
                if msg.message==0x113 {
                    let mut rect=Rect{left:0,top:0,right:0,bottom:0};GetClientRect(h,&mut rect);
                    let (x,y,w,height)=viewport(rect.right,rect.bottom);
                    let frame=match shared.frame.lock() {Ok(f)=>f.clone(),Err(_)=>{error=Some("Player snapshot lock poisoned".into());DestroyWindow(h);break;}};
                    let dc=GetDC(h);
                    if !dc.is_null() {
                        FillRect(dc,&rect,GetStockObject(4));
                        let info=Bitmap{size:40,width:800,height:-600,planes:1,bits:32,compression:0,image:0,x:0,y:0,used:0,important:0};
                        if w>0&&height>0&&StretchDIBits(dc,x,y,w,height,0,0,800,600,frame.as_ptr().cast(),&info,0,0x00cc0020)>0 {shared.presented_crc32.store(u64::from(!crate::image::crc32(&frame,!0)),Ordering::Release);shared.presents.fetch_add(1,Ordering::Release);}
                        ReleaseDC(h,dc);
                    }
                }
                TranslateMessage(&msg);DispatchMessageW(&msg);
            }
            shared.hwnd.store(0,Ordering::Release);shared.stop.store(true,Ordering::Release);KillTimer(h,1);UnregisterClassW(class.as_ptr(),instance);
            error.map_or(Ok(()),Err)
        }
    }
}
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn aspect_and_minimize() {assert_eq!(viewport(1000,600),(100,0,800,600));assert_eq!(viewport(800,800),(0,100,800,600));assert_eq!(viewport(0,5),(0,0,0,0));assert_eq!(viewport(i32::MAX,i32::MAX).2,i32::MAX);}
    #[test] fn console_input_is_bounded_ordered_and_text_only() {
        let s=Session::default();assert!(s.type_byte(0xff).is_err());
        for _ in 0..256 {s.type_byte(b'x').unwrap();}assert!(s.type_byte(b'y').is_err());
        for _ in 0..256 {assert_eq!(s.next_byte(),Some(b'x'));}assert_eq!(s.next_byte(),None);
        for b in b"logcat\r" {s.type_byte(*b).unwrap();}for b in b"logcat\r" {assert_eq!(s.next_byte(),Some(*b));}
    }
    #[test] fn snapshots_are_owned_and_bounded() {let s=Session::default();let mut p=vec![17;crate::framebuffer::MAPPED_BYTES];s.publish(&p).unwrap();p[0]=99;assert_eq!(s.frame.lock().unwrap()[0],17);assert!(s.publish(&p[..10]).is_err());assert!(!s.stopped());s.stop.store(true,Ordering::Release);assert!(s.stopped());}
}

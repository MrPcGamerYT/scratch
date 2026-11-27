//! Native software presentation. UI reads owned snapshots, never live guest RAM.
//! Player UI: thin icon-only titlebar (Back/Home/Recents/Fullscreen/Keymap/
//! Volume/RAM cleaner/Install APK/Settings) with per-app keymaps. Button clicks
//! and keymap translations push bridge commands that the engine drains and
//! feeds to the guest agent over COM2.
use std::collections::VecDeque;
use std::sync::{atomic::{AtomicBool,AtomicI32,AtomicU64,AtomicUsize,Ordering},Arc,Mutex};

/// A titlebar button. `label` is the short text drawn on the button; `command`
/// is the bridge command line pushed to the guest agent when clicked.
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub struct Button {pub label:&'static str,pub command:&'static str}

/// The titlebar button layout. Position is computed at draw time from the
/// window width; each button is `BUTTON_WIDTH` pixels wide with `BUTTON_GAP`
/// pixels between them, left-aligned starting at `TITLEBAR_PAD`.
pub const TITLEBAR_HEIGHT:i32=28;
pub const BUTTON_WIDTH:i32=32;
pub const BUTTON_GAP:i32=4;
pub const TITLEBAR_PAD:i32=6;

/// Default titlebar buttons in left-to-right order.
/// Each maps to a bridge command the guest agent already supports.
pub const BUTTONS:&[Button]=[
    &Button{label:"<",command:"NEXUS 1 BACK"},
    &Button{label:"H",command:"NEXUS 1 HOME"},
    &Button{label:"R",command:"NEXUS 1 RECENTS"},
    &Button{label:"F",command:"__FULLSCREEN__"},
    &Button{label:"K",command:"__KEYMAP__"},
    &Button{label:"V",command:"NEXUS 1 VOLUME_UP"},
    &Button{label:"M",command:"NEXUS 1 VOLUME_DOWN"},
    &Button{label:"G",command:"NEXUS 1 PROVISION_HOME"},
    &Button{label:"A",command:"__INSTALL_APK__"},
    &Button{label:"S",command:"__SETTINGS__"},
];

/// Returns the x-offset of button `index` within a titlebar of `width` pixels.
/// Returns `None` if the button doesn't fit.
pub fn button_x(width:i32,index:usize)->Option<i32> {
    if width<=0||index>=BUTTONS.len() {return None;}
    let x=TITLEBAR_PAD+(index as i32)*(BUTTON_WIDTH+BUTTON_GAP);
    if x+BUTTON_WIDTH>width {None} else {Some(x)}
}

/// Returns the index of the button hit by a click at `x` within the titlebar,
/// or `None` if the click is outside all buttons.
pub fn hit_button(width:i32,x:i32)->Option<usize> {
    if x<TITLEBAR_PAD {return None;}
    for i in 0..BUTTONS.len() {
        let bx=button_x(width,i)?;
        if x>=bx&&x<bx+BUTTON_WIDTH {return Some(i);}
        if x<bx {return None;}
    }
    None
}

/// A per-app keymap entry: maps a host virtual key code to a bridge command.
/// The host key code is a Win32 VK_* constant; the command is the bridge line.
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct KeymapEntry{pub host_key:u32,pub command:String}

/// Per-app keymaps. Each entry maps an app package name (or "default") to a
/// list of key→command mappings. The player's keydown handler looks up the
/// active app and translates the host key to a bridge command.
#[derive(Clone,Debug,Default)]
pub struct Keymaps{pub maps:std::collections::HashMap<String,Vec<KeymapEntry>>}
impl Keymaps {
    pub fn new()->Self {Self::default()}
    pub fn add(&mut self,app:&str,entry:KeymapEntry) {
        self.maps.entry(app.to_string()).or_default().push(entry);
    }
    /// Look up the command for `host_key` in the keymap for `app`
    /// (falling back to "default"). Returns `None` if no mapping exists.
    pub fn translate(&self,app:&str,host_key:u32)->Option<String> {
        let map=self.maps.get(app).or_else(||self.maps.get("default"))?;
        map.iter().find(|e|e.host_key==host_key).map(|e|e.command.clone())
    }
}

#[derive(Debug)]
pub struct Session {
    pub stop:AtomicBool,
    hwnd:AtomicUsize,
    frame:Mutex<Vec<u8>>,
    input:Mutex<VecDeque<u8>>,
    pub presents:AtomicU64,
    pub presented_crc32:AtomicU64,
    /// Outbound bridge commands queued by the player UI. The engine drains
    /// these in its run loop and feeds them to the bridge's pending queue.
    commands:Mutex<VecDeque<String>>,
    /// Active app package name, set by the engine from bridge QUERY_APP.
    current_app:Mutex<Option<String>>,
    /// Per-app keymaps for keyboard translation.
    pub keymaps:Mutex<Keymaps>,
    /// Fullscreen state (toggles window chrome).
    pub fullscreen:AtomicBool,
    /// Volume level: -1 = muted, 0..100 = percent.
    pub volume:AtomicI32,
    /// Current keymap selector index (cycled by the Keymap button).
    pub keymap_selector:AtomicUsize,
}
impl Default for Session {
    fn default()->Self {
        Self{
            stop:AtomicBool::new(false),
            hwnd:AtomicUsize::new(0),
            input:Mutex::new(VecDeque::new()),
            frame:Mutex::new(vec![0;crate::framebuffer::PIXEL_BYTES]),
            presents:AtomicU64::new(0),
            presented_crc32:AtomicU64::new(0),
            commands:Mutex::new(VecDeque::new()),
            current_app:Mutex::new(None),
            keymaps:Mutex::new(Keymaps::new()),
            fullscreen:AtomicBool::new(false),
            volume:AtomicI32::new(50),
            keymap_selector:AtomicUsize::new(0),
        }
    }
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

    /// Push a bridge command line (e.g. "NEXUS 1 BACK") to the outbound queue.
    /// The engine drains these via `next_command()` and feeds them to the bridge.
    pub fn push_command(&self,command:&str)->Result<(),String> {
        let mut q=self.commands.lock().map_err(|_|"Player command queue poisoned")?;
        if q.len()>=64 {return Err("Player command queue full".into());}
        q.push_back(command.to_string());
        Ok(())
    }
    /// Pop the next outbound bridge command, or `None` if empty.
    pub fn next_command(&self)->Option<String> {self.commands.lock().ok()?.pop_front()}

    /// Set the active app package name (called by the engine from bridge QUERY_APP).
    pub fn set_app(&self,app:&str)->Result<(),String> {
        let mut slot=self.current_app.lock().map_err(|_|"Player app slot poisoned")?;
        *slot=Some(app.to_string());
        Ok(())
    }
    /// Get the active app package name, if any.
    pub fn app(&self)->Option<String> {self.current_app.lock().ok()?.clone()}

    /// Toggle fullscreen state. Returns the new state.
    pub fn toggle_fullscreen(&self)->bool {
        let new=!self.fullscreen.swap(!self.fullscreen.load(Ordering::Acquire),Ordering::AcqRel);
        new
    }
    /// Cycle the keymap selector. Returns the new index.
    pub fn cycle_keymap(&self)->usize {
        let idx=self.keymap_selector.fetch_add(1,Ordering::AcqRel);
        idx
    }
    /// Adjust volume by `delta` (clamped to 0..100). Returns the new level.
    pub fn adjust_volume(&self,delta:i32)->i32 {
        let mut v=self.volume.load(Ordering::Acquire);
        v=v.saturating_add(delta).clamp(0,100);
        self.volume.store(v,Ordering::Release);
        v
    }
    /// Translate a host key via the active app's keymap. Returns the bridge
    /// command or `None` if no mapping exists.
    pub fn translate_key(&self,host_key:u32)->Option<String> {
        let app=self.app().unwrap_or_else(||"default".to_string());
        let keymaps=self.keymaps.lock().ok()?;
        keymaps.translate(&app,host_key)
    }
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
        fn TextOutW(dc:Handle,x:i32,y:i32,text:*const u16,count:i32)->i32;
        fn SetTextColor(dc:Handle,color:u32)->u32;
        fn SetBkColor(dc:Handle,color:u32)->u32;
        fn SetBkMode(dc:Handle,mode:i32)->i32;
        fn Rectangle(dc:Handle,left:i32,top:i32,right:i32,bottom:i32)->i32;
    }
    #[link(name="kernel32")] extern "system" {fn GetModuleHandleW(name:*const u16)->Handle;}
    #[link(name="user32")] extern "system" {
        fn GetWindowLongW(h:Handle,index:i32)->i32;
        fn SetWindowLongW(h:Handle,index:i32,value:i32)->i32;
        fn SetWindowPos(h:Handle,insert_after:Handle,x:i32,y:i32,cx:i32,cy:i32,flags:u32)->i32;
    }
    const WS_CAPTION:u32=0x00C00000;
    const WS_THICKFRAME:u32=0x00800000;
    const GWL_STYLE:i32=-16;
    const SWP_NOZORDER:u32=0x0004;
    const SWP_NOMOVE:u32=0x0002;
    const SWP_NOSIZE:u32=0x0001;
    unsafe extern "system" fn proc(h:Handle,m:u32,w:usize,l:isize)->isize {
        if m==2 {PostQuitMessage(0);return 0;}
        // WM_LBUTTONDOWN: hit-test titlebar buttons.
        if m==0x201 {
            let x=(l & 0xffff as isize) as i32;
            let y=(l >> 16) as i32;
            if y<TITLEBAR_HEIGHT {
                let mut rect=Rect{left:0,top:0,right:0,bottom:0};
                if GetClientRect(h,&mut rect)!=0&&rect.right>0 {
                    if let Some(idx)=hit_button(rect.right,x) {
                        let cmd=BUTTONS[idx].command;
                        // Special UI commands handled locally.
                        if cmd=="__FULLSCREEN__" {
                            let _=shared_toggle_fullscreen(h);
                        } else if cmd=="__KEYMAP__" {
                            let _=shared_cycle_keymap();
                        } else if cmd=="__INSTALL_APK__" {
                            let _=shared_push("NEXUS 1 APK_INSTALL");
                        } else if cmd=="__SETTINGS__" {
                            let _=shared_push("NEXUS 1 QUERY_APP");
                        } else {
                            let _=shared_push(cmd);
                        }
                    }
                }
            }
        }
        // WM_KEYDOWN: translate via active app's keymap.
        if m==0x100 {
            let vk=w as u32;
            if let Some(cmd)=shared_translate_key(vk) {let _=shared_push(&cmd);}
        }
        DefWindowProcW(h,m,w,l)
    }
    // Thread-safe helpers that the proc uses. The Session is shared via a
    // static Mutex set once at window creation; the message loop runs on one
    // thread but DispatchMessageW may re-enter, so a Mutex avoids UB.
    static SESSION:std::sync::Mutex<Option<Arc<Session>>>=std::sync::Mutex::new(None);
    fn shared_push(cmd:&str) {if let Ok(guard)=SESSION.lock() {if let Some(s)=&*guard {let _=s.push_command(cmd);}}}
    fn shared_translate_key(vk:u32)->Option<String> {SESSION.lock().ok()?.as_ref().and_then(|s|s.translate_key(vk))}
    fn shared_toggle_fullscreen(h:Handle)->bool {
        if let Ok(guard)=SESSION.lock() {if let Some(s)=&*guard {let fs=s.toggle_fullscreen();unsafe {if fs {SetWindowLongW(h,GWL_STYLE,GetWindowLongW(h,GWL_STYLE)&!(WS_CAPTION as i32|WS_THICKFRAME as i32));SetWindowPos(h,std::ptr::null_mut(),0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOZORDER);}else{SetWindowLongW(h,GWL_STYLE,GetWindowLongW(h,GWL_STYLE)|(WS_CAPTION as i32|WS_THICKFRAME as i32));SetWindowPos(h,std::ptr::null_mut(),0,0,0,0,SWP_NOMOVE|SWP_NOSIZE|SWP_NOZORDER);}}return fs;}}false
    }
    fn shared_cycle_keymap()->usize {SESSION.lock().ok().and_then(|g|g.as_ref().map(|s|s.cycle_keymap())).unwrap_or(0)}
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
            if let Ok(mut g)=SESSION.lock() {*g=Some(shared.clone());}
            let _=ready.send(Ok(()));
            let mut msg:Msg=std::mem::zeroed();let mut error=None;
            loop {
                let status=GetMessageW(&mut msg,null,0,0);
                if status<=0 {if status<0 {error=Some("Player message loop failed".to_string());}break;}
                if shared.stopped() {DestroyWindow(h);break;}
                if msg.message==0x102&&msg.wparam<=127 {let _=shared.type_byte(msg.wparam as u8);}
                if msg.message==0x113 {
                    let mut rect=Rect{left:0,top:0,right:0,bottom:0};GetClientRect(h,&mut rect);
                    let dc=GetDC(h);
                    if !dc.is_null() {
                        // Draw titlebar.
                        let tb=Rect{left:0,top:0,right:rect.right,bottom:TITLEBAR_HEIGHT};
                        FillRect(dc,&tb,GetStockObject(4));
                        SetBkMode(dc,1);
                        SetTextColor(dc,0xffffff);
                        for (i,btn) in BUTTONS.iter().enumerate() {
                            if let Some(bx)=button_x(rect.right,i) {
                                let br=Rect{left:bx,top:0,right:bx+BUTTON_WIDTH,bottom:TITLEBAR_HEIGHT};
                                Rectangle(dc,br.left,br.top,br.right,br.bottom);
                                let lbl:Vec<u16>=btn.label.encode_utf16().collect();
                                TextOutW(dc,bx+4,2,lbl.as_ptr(),lbl.len() as i32);
                            }
                        }
                        // Draw scanout viewport below the titlebar.
                        let (x,y,w,height)=viewport(rect.right,rect.bottom-TITLEBAR_HEIGHT);
                        let fy=y+TITLEBAR_HEIGHT;
                        let frame=match shared.frame.lock() {Ok(f)=>f.clone(),Err(_)=>{error=Some("Player snapshot lock poisoned".into());DestroyWindow(h);break;}};
                        FillRect(dc,&Rect{left:0,top:TITLEBAR_HEIGHT,right:rect.right,bottom:rect.bottom},GetStockObject(4));
                        let info=Bitmap{size:40,width:800,height:-600,planes:1,bits:32,compression:0,image:0,x:0,y:0,used:0,important:0};
                        if w>0&&height>0&&StretchDIBits(dc,x,fy,w,height,0,0,800,600,frame.as_ptr().cast(),&info,0,0x00cc0020)>0 {shared.presented_crc32.store(u64::from(!crate::image::crc32(&frame,!0)),Ordering::Release);shared.presents.fetch_add(1,Ordering::Release);}
                        ReleaseDC(h,dc);
                    }
                }
                TranslateMessage(&msg);DispatchMessageW(&msg);
            }
            if let Ok(mut g)=SESSION.lock() {*g=None;}
            shared.hwnd.store(0,Ordering::Release);shared.stop.store(true,Ordering::Release);KillTimer(h,1);UnregisterClassW(class.as_ptr(),instance);
            error.map_or(Ok(()),Err)
        }
    }
    #[cfg(test)]
    mod abi_tests {
        use super::*;
        use std::mem::{size_of,offset_of};
        #[test] fn win32_x64_layouts_match_sdk_contract() {
            assert_eq!(size_of::<Class>(),72);assert_eq!(offset_of!(Class,proc),8);
            assert_eq!(offset_of!(Class,instance),24);assert_eq!(offset_of!(Class,name),64);
            assert_eq!(size_of::<Msg>(),48);assert_eq!(offset_of!(Msg,wparam),16);
            assert_eq!(offset_of!(Msg,lparam),24);assert_eq!(offset_of!(Msg,pt),36);
            assert_eq!(size_of::<Bitmap>(),40);assert_eq!(size_of::<Rect>(),16);
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
    #[test] fn command_queue_is_bounded_ordered_and_drains() {
        let s=Session::default();
        s.push_command("NEXUS 1 BACK").unwrap();
        s.push_command("NEXUS 1 HOME").unwrap();
        assert_eq!(s.next_command(),Some("NEXUS 1 BACK".to_string()));
        assert_eq!(s.next_command(),Some("NEXUS 1 HOME".to_string()));
        assert_eq!(s.next_command(),None);
        for _ in 0..64 {s.push_command("NEXUS 1 BACK").unwrap();}
        assert!(s.push_command("NEXUS 1 HOME").is_err());
    }
    #[test] fn button_layout_hits_and_gaps() {
        // 10 buttons * (32+4) + 6 = 366. Window of 840 fits all.
        assert_eq!(BUTTONS.len(),10);
        assert_eq!(button_x(840,0),Some(6));
        assert_eq!(button_x(840,1),Some(42));
        assert_eq!(button_x(840,9),Some(6+9*36));
        assert_eq!(hit_button(840,6),Some(0));
        assert_eq!(hit_button(840,37),Some(0));
        assert_eq!(hit_button(840,42),Some(1));
        assert_eq!(hit_button(840,365),Some(9));
        assert_eq!(hit_button(840,0),None);
        assert_eq!(hit_button(840,366),None);
        // Too narrow: button doesn't fit.
        assert_eq!(button_x(10,0),None);
        assert_eq!(hit_button(10,5),None);
    }
    #[test] fn keymaps_translate_per_app_with_default_fallback() {
        let mut k=Keymaps::new();
        k.add("default",KeymapEntry{host_key:0x41,command:"NEXUS 1 BACK".into()});
        k.add("com.foo.app",KeymapEntry{host_key:0x42,command:"NEXUS 1 HOME".into()});
        assert_eq!(k.translate("com.foo.app",0x41),Some("NEXUS 1 BACK".to_string()));
        assert_eq!(k.translate("com.foo.app",0x42),Some("NEXUS 1 HOME".to_string()));
        assert_eq!(k.translate("com.other.app",0x41),Some("NEXUS 1 BACK".to_string()));
        assert_eq!(k.translate("com.other.app",0x42),None);
    }
    #[test] fn session_translate_key_uses_active_app() {
        let s=Session::default();
        {let mut k=s.keymaps.lock().unwrap();k.add("com.foo.app",KeymapEntry{host_key:0x41,command:"NEXUS 1 BACK".into()});k.add("default",KeymapEntry{host_key:0x5a,command:"NEXUS 1 HOME".into()});}
        assert_eq!(s.translate_key(0x5a),Some("NEXUS 1 HOME".to_string()));
        s.set_app("com.foo.app").unwrap();
        assert_eq!(s.translate_key(0x41),Some("NEXUS 1 BACK".to_string()));
        assert_eq!(s.translate_key(0x5a),None);
    }
    #[test] fn fullscreen_keymap_volume_toggle() {
        let s=Session::default();
        assert!(!s.toggle_fullscreen());
        assert!(s.toggle_fullscreen());
        assert!(!s.toggle_fullscreen());
        assert_eq!(s.cycle_keymap(),0);
        assert_eq!(s.cycle_keymap(),1);
        assert_eq!(s.volume.load(Ordering::Acquire),50);
        assert_eq!(s.adjust_volume(10),60);
        assert_eq!(s.adjust_volume(-70),0);
        assert_eq!(s.adjust_volume(200),100);
    }
    #[test] fn default_volume_and_app_state() {
        let s=Session::default();
        assert_eq!(s.volume.load(Ordering::Acquire),50);
        assert_eq!(s.app(),None);
        assert!(!s.fullscreen.load(Ordering::Acquire));
        assert_eq!(s.keymap_selector.load(Ordering::Acquire),0);
    }
}

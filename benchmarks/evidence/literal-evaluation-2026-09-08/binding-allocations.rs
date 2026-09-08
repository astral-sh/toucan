use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
static CALLS: AtomicUsize = AtomicUsize::new(0);
static BYTES: AtomicUsize = AtomicUsize::new(0);
struct Counter;
unsafe impl GlobalAlloc for Counter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        CALLS.fetch_add(1, Ordering::Relaxed);
        BYTES.fetch_add(size, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) { unsafe { System.dealloc(ptr, layout) } }
}
#[global_allocator]
static ALLOCATOR: Counter = Counter;

fn main(){
 let args=std::env::args().collect::<Vec<_>>();CALLS.store(0,Ordering::Relaxed);BYTES.store(0,Ordering::Relaxed);let mut config=toucan::Config::new(toucan::Target::X86_64UnknownLinuxGnu);let setup_calls=CALLS.load(Ordering::Relaxed);let setup_bytes=BYTES.load(Ordering::Relaxed);let mut options=toucan::BindingOptions{no_layout_tests:false,..Default::default()};
 let mut i=2;while i<args.len(){match args[i].as_str(){"--target"=>{},"-I"=>config.preprocessor.include_dirs.push((&args[i+1]).into()),"--allowlist"=>options.allowlist.push(args[i+1].clone()),"--sysroot"=>{config.preprocessor.include_dirs.push(std::path::Path::new(&args[i+1]).join("usr/include/x86_64-linux-gnu"));config.preprocessor.include_dirs.push(std::path::Path::new(&args[i+1]).join("usr/include"));},_=>panic!("unexpected argument")};i+=2;}
 let path=std::path::Path::new(&args[1]);let compilation=toucan::parse_file(path,&config).unwrap();drop(compilation.bindings(&options).unwrap());drop(compilation);
 CALLS.store(0,Ordering::Relaxed);BYTES.store(0,Ordering::Relaxed);let compilation=toucan::parse_file(path,&config).unwrap();let parse_calls=CALLS.load(Ordering::Relaxed);let parse_bytes=BYTES.load(Ordering::Relaxed);
 CALLS.store(0,Ordering::Relaxed);BYTES.store(0,Ordering::Relaxed);let (output,_)=compilation.bindings(&options).unwrap();let calls=CALLS.load(Ordering::Relaxed);let bytes=BYTES.load(Ordering::Relaxed);eprintln!("setup {setup_calls} {setup_bytes} parse {parse_calls} {parse_bytes} bindings {calls} {bytes} unit {} profile {}",std::mem::size_of::<toucan::semantic::TranslationUnit>(),std::mem::size_of::<toucan::CompilerProfile>());print!("{output}");
}

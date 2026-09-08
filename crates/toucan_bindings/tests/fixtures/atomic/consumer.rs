#![allow(non_camel_case_types,non_snake_case,dead_code)]
mod bindings { include!("bindings.rs"); }
use bindings::*;
use std::sync::atomic::Ordering;
#[no_mangle]
pub unsafe extern "C" fn rust_storage(p:*mut Fields,q:*mut AtomicPair)->i32 {
    (*p).a.fetch_add(2,Ordering::SeqCst);
    c_pair_write(q);
    0
}
unsafe extern "C" fn callback(x:i32)->i32 {x+2}
unsafe extern "C" fn callback_i8(x:i8)->i8 {x+2}
unsafe extern "C" fn callback_i16(x:i16)->i16 {x+2}
unsafe extern "C" fn callback_i64(x:i64)->i64 {x+2}
unsafe extern "C" fn callback_b(x:bool)->bool {!x}
unsafe extern "C" fn callback_f(x:f32)->f32 {x+2.0}
unsafe extern "C" fn callback_d(x:f64)->f64 {x+2.0}
unsafe extern "C" fn callback_p(x:*mut i32)->*mut i32 {x.add(2)}
fn main(){unsafe{
    let mut data=[0i32;4];
    let mut fields=Fields{a:AtomicInt::new(1),b:AtomicBool::new(false),p:AtomicPointer::new(std::ptr::null_mut())};
    c_fields(&mut fields,data.as_mut_ptr());
    assert_eq!(fields.a.load(Ordering::SeqCst),4);
    assert!(fields.b.load(Ordering::SeqCst));
    assert_eq!(fields.p.load(Ordering::SeqCst),data.as_mut_ptr());
    let mut pair=AtomicPair::uninit();
    c_pair_init(&mut pair);
    // Const atomic pointees have opaque read-only C views with no Rust methods.
    assert_eq!(c_pair_sum((&pair as *const AtomicPair).cast()),3.75);
    assert_eq!(c_callback_storage(&mut fields,&mut pair),0);
    assert_eq!(fields.a.load(Ordering::SeqCst),6);
    assert_eq!(c_pair_sum((&pair as *const AtomicPair).cast()),12.0);
    let mut union=U{a:std::mem::ManuallyDrop::new(AtomicInt::new(7))};
    c_union_increment(&mut union);assert_eq!(union.a.load(Ordering::SeqCst),9);
    let global=&*std::ptr::addr_of!(GLOBAL);
    global.store(0,Ordering::SeqCst);
    let worker=std::thread::spawn(||c_increment(10000));
    for _ in 0..10000 {global.fetch_add(1,Ordering::SeqCst);}
    worker.join().unwrap();assert_eq!(global.load(Ordering::SeqCst),20000);
    assert_eq!(c_read_constant(),17);assert_eq!(c_read_device(),19);
    assert_eq!(c_i8(-100) as i64,-101);assert_eq!(c_i16(-100) as i64,-101);
    assert_eq!(c_i32(-100),-101);assert_eq!(c_i64(-100),-101);
    assert!(c_b(false));assert!(!c_b(true));
    assert_eq!(c_f(-2.5),-1.0);assert_eq!(c_d(-2.5),-1.0);
    assert_eq!(c_p(data.as_mut_ptr()),data.as_mut_ptr().add(1));
    assert_eq!(c_enum(999),999);
    let atomic_enum=AtomicEnum::new(999);assert_eq!(atomic_enum.load(Ordering::SeqCst),999);
    assert_eq!(c_function(Some(callback)).unwrap()(7),9);
    assert_eq!(c_callback(Some(callback),-100),-98);
    assert_eq!(c_callback_i8(Some(callback_i8),-100),-98);
    assert_eq!(c_callback_i16(Some(callback_i16),-100),-98);
    assert_eq!(c_callback_i64(Some(callback_i64),-100),-98);
    assert_eq!(c_callback_b(Some(callback_b),false),true);
    assert_eq!(c_callback_f(Some(callback_f),-2.5),-0.5);
    assert_eq!(c_callback_d(Some(callback_d),-2.5),-0.5);
    assert_eq!(c_callback_p(Some(callback_p),data.as_mut_ptr()),data.as_mut_ptr().add(2));
    assert_eq!(c_many(-1,-2,-3,-4,-5,-6,-7,-8,-9,-10,-11,-12),-78);
}}

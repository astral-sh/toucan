#![allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals
)]
mod b {
    include!("bindings.rs");
}
use b::*;
use std::ffi::{c_long, c_void};
unsafe extern "C" fn callback(mut v: MuslPair, delta: c_long, context: *mut c_void) -> MuslPair {
    let count = &mut *(context as *mut c_long);
    *count += 1;
    v.x += delta as f32;
    v.y += (*count) as f32;
    v
}
macro_rules! sa {
    ($v:ident, $t:ty) => {
        $v.extend_from_slice(&[std::mem::size_of::<$t>(), std::mem::align_of::<$t>()]);
    };
}
macro_rules! offset {
    ($t:ty,$f:ident) => {{
        let value = std::mem::MaybeUninit::<$t>::uninit();
        let base = value.as_ptr();
        unsafe { std::ptr::addr_of!((*base).$f) as usize - base as usize }
    }};
}
fn main() {
    let arm = cfg!(target_arch = "aarch64");
    let mut dimensions = vec![];
    sa!(dimensions, std::ffi::c_char);
    sa!(dimensions, i16);
    sa!(dimensions, i32);
    sa!(dimensions, c_long);
    sa!(dimensions, i64);
    sa!(dimensions, *mut c_void);
    sa!(dimensions, u32);
    sa!(dimensions, u32);
    sa!(dimensions, f32);
    sa!(dimensions, f64);
    dimensions.extend_from_slice(&[16, 16, if arm { 32 } else { 24 }, 8]);
    sa!(dimensions, MuslPair);
    sa!(dimensions, MuslTriplet);
    sa!(dimensions, MuslMixed);
    sa!(dimensions, MuslPacked);
    sa!(dimensions, MuslBits);
    sa!(dimensions, MuslAtomic);
    sa!(dimensions, MuslUnion);
    dimensions.extend_from_slice(&[
        offset!(MuslPair, y),
        offset!(MuslTriplet, z),
        offset!(MuslMixed, value),
        offset!(MuslMixed, number),
        offset!(MuslMixed, pointer),
        offset!(MuslPacked, value),
        offset!(MuslBits, tail),
        usize::from(arm),
        1,
        usize::from(arm),
        1,
    ]);
    let mut c_dimensions = vec![usize::MAX; dimensions.len()];
    unsafe {
        musl_dimensions(c_dimensions.as_mut_ptr());
    }
    assert_eq!(dimensions, c_dimensions);
    assert_eq!(MUSL_CHAR_UNSIGNED, i32::from(arm));
    assert_eq!(MUSL_MASK, 0x8123456789abcdef);
    for i in 0..1000 {
        unsafe {
            let p = musl_pair(MuslPair {
                x: i as f32,
                y: 10.0,
            });
            assert_eq!((p.x, p.y), (i as f32 + 2.0, 7.0));
            let t = musl_triplet(MuslTriplet { x: 1, y: 2, z: 3 });
            assert_eq!((t.x, t.y, t.z), (2, 4, 6));
            let mut context = 0 as c_long;
            let p = musl_callback(
                Some(callback),
                MuslPair { x: 3.0, y: 4.0 },
                &mut context as *mut _ as *mut c_void,
            );
            assert_eq!((p.x, p.y, context), (19.0, 7.0, 2));
            let m = musl_mixed(MuslMixed {
                tag: 1,
                value: i,
                number: 7.0,
                pointer: &mut context as *mut _ as *mut c_void,
            });
            assert_eq!(
                (m.tag, m.value, m.number, m.pointer),
                (7, i + 17, 14.0, &mut context as *mut _ as *mut c_void)
            );
            let p = musl_packed(MuslPacked { tag: 2, value: i });
            let value = p.value;
            assert_eq!((p.tag, value), (9, i + 19));
            assert_eq!(
                musl_union(MuslUnion { integer: i as u64 }).integer,
                (i as u64) ^ MUSL_MASK
            );
            assert_eq!(
                musl_stack(1, 2, 3, 4, 5, 6, 7, 8, 1., 2., 3., 4., 5., 6., 7., 8., 9.),
                489
            );
            assert_eq!(
                musl_variadic(1, 2i32, 3.0f64, 4i64, &context as *const c_long),
                12
            );
            let mut bits: MuslBits = std::mem::zeroed();
            musl_bits_write(&mut bits);
            assert_eq!(
                (bits.value(), bits.flag(), bits.rest(), bits.tail),
                (-9, 6, 381, 12)
            );
            bits.set_value(-7);
            bits.set_flag(3);
            bits.set_rest(217);
            bits.tail = 4;
            assert_eq!(musl_bits_read(&bits), 4217293);
            let mut atom: MuslAtomic = std::mem::zeroed();
            musl_atomic_store(&mut atom, i as u64 + 100);
            assert_eq!(musl_atomic_load(&atom), i as u64 + 100);
        }
    }
    println!("musl ABI: {} dimensions; 1000 aggregate, callback, stack, variadic, bitfield, atomic rounds",dimensions.len());
}

#![allow(dead_code, non_camel_case_types, non_snake_case, non_upper_case_globals)]
mod bindings {
    include!("bindings.rs");
}
use bindings::*;
use std::ffi::{c_long, c_void};

unsafe extern "C" fn callback(mut pair: I686Pair, delta: c_long, context: *mut c_void) -> I686Pair {
    let calls = &mut *(context as *mut c_long);
    *calls += 1;
    pair.x += delta as f32;
    pair.y += *calls as f32;
    pair
}

fn main() {
    assert_eq!(std::mem::size_of::<usize>(), 4);
    assert_eq!(std::mem::size_of::<I686Pair>(), 8);
    assert_eq!(std::mem::size_of::<I686Record>(), 28);
    assert_eq!(std::mem::align_of::<I686Record>(), 4);
    assert_eq!(std::mem::offset_of!(I686Record, count), 20);
    assert_eq!(std::mem::size_of::<I686Packed>(), 5);
    assert_eq!(std::mem::align_of::<I686Packed>(), 1);
    assert_eq!(std::mem::size_of::<I686Bits>(), 4);
    assert_eq!(std::mem::offset_of!(I686Bits, tail), 3);
    assert_eq!(I686_MAGIC, 0x7351a20);
    assert_eq!(I686_MASK, 0x1122334455667788);

    for index in 0..1000i32 {
        unsafe {
            let pair = i686_pair(I686Pair { x: index as f32, y: 10.0 });
            assert_eq!((pair.x, pair.y), (index as f32 + 2.0, 7.0));

            let mut calls = 0 as c_long;
            let pair = i686_callback(
                Some(callback),
                I686Pair { x: 3.0, y: 4.0 },
                &mut calls as *mut _ as *mut c_void,
            );
            assert_eq!((pair.x, pair.y, calls), (19.0, 7.0, 2));

            let context = &mut calls as *mut _ as *mut c_void;
            let record = i686_record(
                I686Record {
                    tag: 1,
                    value: index as c_long,
                    number: 7.0,
                    context,
                    count: 0x1234,
                },
                17,
            );
            assert_eq!(record.tag, 7);
            assert_eq!(record.value, index as c_long + 17);
            assert_eq!(record.number, 14.0);
            assert_eq!(record.context, context);
            assert_eq!(record.count, 0x1234 ^ I686_MASK as i64);

            let packed = i686_packed(I686Packed { tag: 2, value: index as c_long });
            let value = packed.value; // Copy before access to avoid an unaligned reference.
            assert_eq!((packed.tag, value), (9, index as c_long + 19));

            assert_eq!(
                i686_stack(1, 2, 3, 4, 5, 6, 7, 8, 1., 2., 3., 4., 5., 6., 7., 8.),
                408,
            );

            let mut bits: I686Bits = std::mem::zeroed();
            i686_bits_write(&mut bits);
            assert_eq!((bits.value(), bits.flag(), bits.rest(), bits.tail), (-9, 6, 381, 12));
            bits.set_value(-7);
            bits.set_flag(3);
            bits.set_rest(217);
            bits.tail = 4;
            assert_eq!(i686_bits_read(&bits), 4217293);
        }
    }
    println!("i686 C/Rust FFI: 1000 aggregate, callback, stack, and bitfield rounds passed");
}

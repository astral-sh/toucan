#![allow(
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals
)]
mod bindings {
    include!("bindings.rs");
}
use bindings::*;
use std::ffi::{c_long, c_void};

unsafe extern "C" fn callback(mut pair: Armv7Pair, delta: f32, context: *mut c_void) -> Armv7Pair {
    let calls = &mut *(context as *mut c_long);
    *calls += 1;
    pair.x += delta;
    pair.y += *calls as f32;
    pair
}

fn main() {
    assert_eq!(std::mem::size_of::<usize>(), 4);
    assert_eq!(std::mem::size_of::<Armv7Pair>(), 8);
    assert_eq!(std::mem::size_of::<Armv7Record>(), 32);
    assert_eq!(std::mem::align_of::<Armv7Record>(), 8);
    assert_eq!(std::mem::offset_of!(Armv7Record, count), 24);
    assert_eq!(std::mem::size_of::<Armv7Packed>(), 5);
    assert_eq!(std::mem::align_of::<Armv7Packed>(), 1);
    assert_eq!(std::mem::size_of::<Armv7Bits>(), 4);
    assert_eq!(std::mem::offset_of!(Armv7Bits, tail), 3);
    assert_eq!(ARMV7_MAGIC, 0x7351a20);
    assert_eq!(ARMV7_MASK, 0x1122334455667788);

    for index in 0..256i32 {
        unsafe {
            let pair = armv7_pair(Armv7Pair {
                x: index as f32,
                y: 10.0,
            });
            assert_eq!((pair.x, pair.y), (index as f32 + 2.0, 7.0));

            let mut calls = 0 as c_long;
            let pair = armv7_callback(
                Some(callback),
                Armv7Pair { x: 3.0, y: 4.0 },
                &mut calls as *mut _ as *mut c_void,
            );
            assert_eq!((pair.x, pair.y, calls), (19.0, 7.0, 2));

            let context = &mut calls as *mut _ as *mut c_void;
            let record = armv7_record(
                Armv7Record {
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
            assert_eq!(record.count, 0x1234 ^ ARMV7_MASK as i64);

            let mut packed = Armv7Packed {
                tag: 2,
                value: index as c_long,
            };
            armv7_packed_write(&mut packed, 19);
            let value = std::ptr::addr_of!(packed.value).read_unaligned();
            assert_eq!((packed.tag, value), (9, index as c_long + 19));

            assert_eq!(
                armv7_stack(1, 2, 3, 4, 5, 6, 7, 8, 1., 2., 3., 4., 5., 6., 7., 8.),
                408,
            );

            let mut bits: Armv7Bits = std::mem::zeroed();
            armv7_bits_write(&mut bits);
            assert_eq!(
                (bits.value(), bits.flag(), bits.rest(), bits.tail),
                (-9, 6, 381, 12)
            );
            bits.set_value(-7);
            bits.set_flag(3);
            bits.set_rest(217);
            bits.tail = 4;
            assert_eq!(armv7_bits_read(&bits), 4217293);
        }
    }
    println!(
        "ARMv7 hard-float C/Rust FFI: 256 aggregate, callback, stack, and bitfield rounds passed"
    );
}

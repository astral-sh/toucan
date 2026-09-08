#![allow(non_camel_case_types, non_snake_case, dead_code)]
mod bindings { include!("bindings.rs"); }
use bindings::*;
#[cfg(rustified_enums)]
macro_rules! value { ($ty:ident,$variant:ident,$n:expr) => {$ty::$variant}; }
#[cfg(not(rustified_enums))]
macro_rules! value { ($ty:ident,$variant:ident,$n:expr) => {$n as $ty}; }
unsafe extern "C" fn callback(_:Byte)->Byte{value!(Byte,BYTE_LAST,199)}
unsafe extern "C" fn byte_value(x:Byte)->i32{x as i32}
unsafe extern "C" fn signed_value(x:SignedByte)->i32{x as i32}
unsafe extern "C" fn word_value(x:Word)->i32{x as i32}
unsafe extern "C" fn signed_word_value(x:SignedWord)->i32{x as i32}
fn main(){unsafe{
    assert_eq!(byte_echo(value!(Byte,BYTE_MAX,255)) as u32,255);
    assert_eq!(signed_next(value!(SignedByte,SIGNED_MIN,-128)) as i32,-127);
    assert_eq!(word_previous(value!(Word,WORD_MAX,65535)) as u32,65534);
    assert_eq!(signed_word_next(value!(SignedWord,SWORD_MIN,-32768)) as i32,-32767);
    assert_eq!(invoke(Some(callback),value!(Byte,BYTE_MAX,255)) as u32,199);
    assert_eq!(sum_many(value!(Byte,BYTE_ZERO,0),value!(Byte,BYTE_MAX,255),value!(Byte,BYTE_ONE,1),value!(Byte,BYTE_TWO,2),value!(Byte,BYTE_THREE,3),value!(Byte,BYTE_MID,127),value!(Byte,BYTE_NEXT,128),value!(Byte,BYTE_LAST,199),value!(Byte,BYTE_THREE,3),value!(Byte,BYTE_TWO,2),value!(Byte,BYTE_ONE,1),value!(Byte,BYTE_ZERO,0)),721);
    let packet=packet_update(Packet{tag:3,byte:value!(Byte,BYTE_MID,127),signed_byte:value!(SignedByte,SIGNED_MIN,-128),word:value!(Word,WORD_MAX,65535),values:[value!(Byte,BYTE_ONE,1),value!(Byte,BYTE_TWO,2),value!(Byte,BYTE_THREE,3)]});
    assert_eq!((packet.tag as i32,packet.byte as u32,packet.signed_byte as i32,packet.word as u32),(3,128,-127,65534));
    assert_eq!([packet.values[0]as u32,packet.values[1]as u32,packet.values[2]as u32],[1,2,199]);
    assert_eq!(union_previous(Value{word:value!(Word,WORD_MAX,65535)}).word as u32,65534);
    assert_eq!(promoted(3,value!(Byte,BYTE_MAX,255) as i32,value!(SignedByte,SIGNED_MIN,-128) as i32,value!(Word,WORD_MAX,65535) as i32,value!(SignedWord,SWORD_MIN,-32768) as i32),32897);
    assert_eq!(check_named_callbacks(Some(byte_value),Some(signed_value),Some(word_value),Some(signed_word_value)),0);
    #[cfg(not(rustified_enums))]
    {assert_eq!(check_callbacks(Some(byte_value),Some(signed_value),Some(word_value),Some(signed_word_value)),0);}
}}

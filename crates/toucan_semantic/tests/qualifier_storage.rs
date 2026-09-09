use std::collections::HashSet;
use std::mem::size_of;

use toucan_semantic::{IntegerKind, Qualifiers, Type, TypeKind};

#[test]
fn microsoft_qualifiers_fit_existing_types_and_keep_json_and_identity() {
    assert_eq!(size_of::<Qualifiers>(), 4);
    assert_eq!(size_of::<Type>(), 40);

    let mut identities = HashSet::new();
    for (unaligned, ptr32, suffix) in [
        (false, false, ""),
        (true, false, ",\"is_unaligned\":true"),
        (false, true, ",\"is_msvc_ptr32\":true"),
        (true, true, ",\"is_unaligned\":true,\"is_msvc_ptr32\":true"),
    ] {
        let mut qualifiers = Qualifiers::default();
        qualifiers.set_unaligned(unaligned);
        qualifiers.set_msvc_ptr32(ptr32);
        assert_eq!(qualifiers.is_unaligned(), unaligned);
        assert_eq!(qualifiers.is_msvc_ptr32(), ptr32);
        assert_eq!(
            serde_json::to_string(&qualifiers).unwrap(),
            format!("{{\"is_const\":false,\"is_volatile\":false,\"is_restrict\":false{suffix}}}")
        );
        let mut ty = Type::new(TypeKind::Integer(IntegerKind::Int));
        ty.qualifiers = qualifiers;
        assert!(identities.insert(ty));
    }
    assert_eq!(identities.len(), 4);

    let mut qualifiers = Qualifiers::default();
    qualifiers.set_unaligned(true);
    qualifiers.set_msvc_ptr32(true);
    qualifiers.set_unaligned(false);
    assert!(!qualifiers.is_unaligned());
    assert!(qualifiers.is_msvc_ptr32());
    qualifiers.set_unaligned(true);
    qualifiers.set_msvc_ptr32(false);
    assert!(qualifiers.is_unaligned());
    assert!(!qualifiers.is_msvc_ptr32());
}

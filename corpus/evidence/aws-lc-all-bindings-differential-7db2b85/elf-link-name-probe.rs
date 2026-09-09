unsafe extern "C" {
    #[link_name = "\u{1}toucan_link_name_probe"]
    fn probe() -> i32;
}

#[no_mangle]
pub unsafe extern "C" fn invoke() -> i32 {
    probe()
}

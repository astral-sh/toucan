// Set AWS_BINDINGS_PATH to the extracted Toucan or bindgen-cli bindings.
include!(env!("AWS_BINDINGS_PATH"));

fn main() {
    // Printing the address forces a native relocation for this C global.
    println!("{:p}", ::core::ptr::addr_of!(ASN1_BOOLEAN_it));
}

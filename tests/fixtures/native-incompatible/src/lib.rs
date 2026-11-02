#[repr(C)]
pub struct Header {
    version: u32,
    size: u32,
}
static HEADER: Header = Header {
    version: 999,
    size: 8,
};
#[no_mangle]
pub extern "C" fn micopi_plugin_v1() -> *const Header {
    &HEADER
}

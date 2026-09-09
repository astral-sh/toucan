#[cfg(all(feature = "allocator-jemalloc", feature = "allocator-mimalloc"))]
compile_error!("select at most one benchmark allocator");

#[cfg(all(
    feature = "allocator-jemalloc",
    not(all(unix, not(target_os = "openbsd")))
))]
compile_error!("the jemalloc benchmark requires Unix other than OpenBSD");

#[cfg(all(
    feature = "allocator-jemalloc",
    not(feature = "allocator-mimalloc"),
    unix,
    not(target_os = "openbsd")
))]
#[global_allocator]
static ALLOCATOR: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(all(feature = "allocator-mimalloc", not(feature = "allocator-jemalloc")))]
#[global_allocator]
static ALLOCATOR: mimalloc::MiMalloc = mimalloc::MiMalloc;

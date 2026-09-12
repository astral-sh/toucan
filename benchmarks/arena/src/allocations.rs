//! System-allocator instrumentation for a separate, untimed benchmark build.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

static ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static REALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static ALLOCATED_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_BYTES: AtomicUsize = AtomicUsize::new(0);

struct CountingAllocator;

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

fn allocated(size: usize) {
    ALLOCATED_BYTES.fetch_add(size, Relaxed);
    let live = LIVE_BYTES.fetch_add(size, Relaxed) + size;
    PEAK_BYTES.fetch_max(live, Relaxed);
}

// Delegate allocation and deallocation to System with the original pointers and
// layouts. Counters use atomics and never allocate or change allocator behavior.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            ALLOCATIONS.fetch_add(1, Relaxed);
            allocated(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            ALLOCATIONS.fetch_add(1, Relaxed);
            allocated(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Relaxed);
        unsafe { System.dealloc(pointer, layout) }
    }

    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, size: usize) -> *mut u8 {
        let pointer = unsafe { System.realloc(pointer, old, size) };
        if !pointer.is_null() {
            REALLOCATIONS.fetch_add(1, Relaxed);
            LIVE_BYTES.fetch_sub(old.size(), Relaxed);
            allocated(size);
        }
        pointer
    }
}

pub fn measure(operation: impl FnOnce()) -> serde_json::Value {
    let allocations = ALLOCATIONS.load(Relaxed);
    let reallocations = REALLOCATIONS.load(Relaxed);
    let allocated_bytes = ALLOCATED_BYTES.load(Relaxed);
    let live_bytes = LIVE_BYTES.load(Relaxed);
    PEAK_BYTES.store(live_bytes, Relaxed);
    operation();
    // Read all counters before constructing the JSON result, which allocates.
    let allocations = ALLOCATIONS.load(Relaxed) - allocations;
    let reallocations = REALLOCATIONS.load(Relaxed) - reallocations;
    let allocated_bytes = ALLOCATED_BYTES.load(Relaxed) - allocated_bytes;
    let peak_live_bytes = PEAK_BYTES.load(Relaxed) - live_bytes;
    let retained_bytes = LIVE_BYTES.load(Relaxed) as i64 - live_bytes as i64;
    serde_json::json!({
        "allocations": allocations,
        "reallocations": reallocations,
        "allocated_bytes": allocated_bytes,
        "peak_live_bytes": peak_live_bytes,
        "retained_bytes": retained_bytes,
    })
}

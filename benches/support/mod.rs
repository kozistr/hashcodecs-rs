// Keep benchmark allocation behavior stable without choosing an allocator for crate users.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(target_os = "windows")]
pub fn pin_to_one_cpu() {
    use core::ffi::c_void;

    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut c_void;
        fn GetProcessAffinityMask(
            process: *mut c_void,
            process_mask: *mut usize,
            system_mask: *mut usize,
        ) -> i32;
        fn SetProcessAffinityMask(process: *mut c_void, mask: usize) -> i32;
    }

    let process = unsafe { GetCurrentProcess() };
    let mut process_mask = 0;
    let mut system_mask = 0;
    let result =
        unsafe { GetProcessAffinityMask(process, &raw mut process_mask, &raw mut system_mask) };
    assert_ne!(result, 0, "failed to read the benchmark process affinity");
    let first_available_cpu = process_mask & process_mask.wrapping_neg();
    let result = unsafe { SetProcessAffinityMask(process, first_available_cpu) };
    assert_ne!(result, 0, "failed to pin the benchmark process");
}

#[cfg(target_os = "linux")]
pub fn pin_to_one_cpu() {
    const CPU_SET_BYTES: usize = 128;

    #[repr(C)]
    struct CpuSet {
        words: [usize; CPU_SET_BYTES / size_of::<usize>()],
    }

    unsafe extern "C" {
        fn sched_getaffinity(process: i32, size: usize, set: *mut CpuSet) -> i32;
        fn sched_setaffinity(process: i32, size: usize, set: *const CpuSet) -> i32;
    }

    let mut available = CpuSet {
        words: [0; CPU_SET_BYTES / size_of::<usize>()],
    };
    let result = unsafe { sched_getaffinity(0, size_of::<CpuSet>(), &raw mut available) };
    assert_eq!(result, 0, "failed to read the benchmark process affinity");
    let mut set = CpuSet {
        words: [0; CPU_SET_BYTES / size_of::<usize>()],
    };
    let (index, word) = available
        .words
        .iter()
        .copied()
        .enumerate()
        .find(|(_, word)| *word != 0)
        .expect("the benchmark process has no available CPU");
    set.words[index] = word & word.wrapping_neg();
    let result = unsafe { sched_setaffinity(0, size_of::<CpuSet>(), &raw const set) };
    assert_eq!(result, 0, "failed to pin the benchmark process");
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn pin_to_one_cpu() {}

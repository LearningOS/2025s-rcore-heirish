//! Process management syscalls
use crate::{
    mm::{frame_alloc, translated_byte_buffer, PTEFlags, PageTable, VPNRange, VirtAddr},
    task::{
        change_program_brk, current_user_token, exit_current_and_run_next,
        get_current_task_syscall_num, suspend_current_and_run_next,
    },
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let timeval_len = core::mem::size_of::<TimeVal>();
    let mut buffers = translated_byte_buffer(current_user_token(), ts as *const u8, timeval_len);
    let us = get_time_us();
    let time_val = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    unsafe {
        let time_ptr = buffers[0].as_mut_ptr() as *mut TimeVal;
        core::ptr::write_volatile(time_ptr, time_val);
    }
    0
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    match trace_request {
        0 => {
            println!("[Kernel] sys_trace request:0");
            let page_table = PageTable::from_token(current_user_token());
            if let Some(page_table_entry) = page_table.translate(VirtAddr::from(id).into()) {
                if page_table_entry.is_valid() && page_table_entry.readable() {
                    let byte_array = page_table_entry.ppn().get_bytes_array();
                    println!("[Kernel] sys_trace request:0, will read");
                    return byte_array[0] as isize;
                }
            }
            -1
        }
        1 => {
            println!("[Kernel] sys_trace request:1");
            let page_table = PageTable::from_token(current_user_token());
            if let Some(page_table_entry) = page_table.translate(VirtAddr::from(id).into()) {
                if page_table_entry.is_valid() && page_table_entry.writable() {
                    let byte_array = page_table_entry.ppn().get_bytes_array();
                    println!("[Kernel] sys_trace request:1, will write");
                    byte_array[0] = data as u8;
                }
            }
            -1
        }
        2 => {
            println!("[Kernel] sys_trace request:2");
            get_current_task_syscall_num(id) as isize
        }
        _ => -1,
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    let start_va = VirtAddr::from(start);
    let end_va = VirtAddr::from(start + len);
    if !start_va.aligned() {
        return -1;
    }

    if prot & !0x7 != 0 {
        return -1;
    }

    if prot & 0x7 == 0 {
        return -1;
    }

    let mut pte_flags = PTEFlags::U;
    if prot & 0x1 != 0 {
        pte_flags |= PTEFlags::R;
    }
    if prot & 0x2 != 0 {
        pte_flags |= PTEFlags::W;
    }
    if prot & 0x4 != 0 {
        pte_flags |= PTEFlags::X;
    }

    //precondition: start_va is aligned
    let mut page_table = PageTable::from_token(current_user_token());
    let vpn_range = VPNRange::new(start_va.floor(), end_va.ceil());
    for vpn in vpn_range {
        if let Some(pte) = page_table.translate(vpn) {
            println!(
                "[Kernel] sys_mmap found pte {:?} for vpn {:?}",
                pte.flags(),
                vpn
            );
            if pte.is_valid() {
                println!("[Kernel] sys_mmap already mapped vpn {:?}", vpn);
                return -1;
            }
        }
        if let Some(frame) = frame_alloc() {
            page_table.map(vpn, frame.ppn, pte_flags);
            println!(
                "[Kernel] sys_mmap mapped vpn {:?}, pte flags {:?}., len:{}",
                vpn, pte_flags, len
            );
        } else {
            println!("[Kernel] sys_mmap allocate physical frame failed.");
            return -1;
        }
    }
    println!("[Kernel] sys_mmap mapping done");
    0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    let start_va = VirtAddr::from(start);
    let end_va = VirtAddr::from(start + len);
    if !start_va.aligned() {
        return -1;
    }
    let mut page_table = PageTable::from_token(current_user_token());
    let vpn_range = VPNRange::new(start_va.floor(), end_va.ceil());
    for vpn in vpn_range {
        let pte_ret = page_table.translate(vpn);
        if pte_ret.is_none() {
            println!(
                "[Kernel] sys_munmap vpn {:?} not mapped yet, no pte found!",
                vpn
            );
            return -1;
        }
        let pte = pte_ret.unwrap();
        if !pte.is_valid() {
            println!(
                "[Kernel] sys_munmap vpn {:?} not mapped yet, pte_flags:{:?}",
                vpn,
                pte.flags()
            );
            return -1;
        }
        page_table.unmap(vpn);
    }
    0
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

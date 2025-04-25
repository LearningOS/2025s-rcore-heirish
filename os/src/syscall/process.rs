//! Process management syscalls
use crate::{
    mm::{frame_alloc, translated_byte_buffer, PTEFlags, PageTable, VPNRange, VirtAddr},
    task::{
        change_program_brk, current_user_token, exit_current_and_run_next,
        get_current_task_syscall_num, map_one_frame_current_task, suspend_current_and_run_next,
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
            let page_table = PageTable::from_token(current_user_token());
            let virt_addr = VirtAddr::from(id);
            if let Some(page_table_entry) = page_table.translate(virt_addr.floor()) {
                if page_table_entry.is_valid()
                    && page_table_entry.readable()
                    && page_table_entry.umode_accessable()
                {
                    let addr_offset = virt_addr.page_offset();
                    let byte_array = &page_table_entry.ppn().get_bytes_array()[addr_offset..];
                    return byte_array[0] as isize;
                }
            }
            -1
        }
        1 => {
            let page_table = PageTable::from_token(current_user_token());
            let virt_addr = VirtAddr::from(id);
            if let Some(page_table_entry) = page_table.translate(virt_addr.floor()) {
                if page_table_entry.is_valid()
                    && page_table_entry.writable()
                    && page_table_entry.umode_accessable()
                {
                    let addr_offset = virt_addr.page_offset();
                    let byte_array = &mut page_table_entry.ppn().get_bytes_array()[addr_offset..];
                    byte_array[0] = data as u8;
                    return 0;
                }
            }
            -1
        }
        2 => get_current_task_syscall_num(id) as isize,
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
    //DANGER: do not use this temporary page_table to do any operations that might add items to PageTable::frames,
    //because it's actually create a new empty vector in PageTable::from_token, not current running tasks's actual pte frames.
    let page_table = PageTable::from_token(current_user_token());
    let vpn_range = VPNRange::new(start_va.floor(), end_va.ceil());
    for vpn in vpn_range {
        if let Some(pte) = page_table.translate(vpn) {
            if pte.is_valid() {
                return -1;
            }
        }
        if let Some(frame) = frame_alloc() {
            // here DO NOT use page_table.map() to map the frame, because here page_table is a temporary variable.
            // after this sys call it weill be reclaimed., so page_table.frames will also be reclaimed.
            // then all the page_table.frames will be push into StackFrameAllocator.recycled.
            // when a new memory frame allocate request to StackFrameAllocator, all bit in the frame will be cleared
            // so the new pte added by page_table.frames will lost in next sys_mmap call
            //if use page_table.map here, it will be
            //1st sys_mmap: page_table.map -> page_table.frame.push -> end of call -> page_table reclaim -> FrameTrack::Drop->frame_dealloc
            //2nd sys_mmap with same vpn : page_table.transfer will found pte, all info including ppn and flags  will be lost because that leaf node frame already reclaimed at the end of previous call.
            map_one_frame_current_task(vpn, frame.ppn, pte_flags);
        } else {
            return -1;
        }
    }
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
            return -1;
        }
        let pte = pte_ret.unwrap();
        if !pte.is_valid() {
            return -1;
        }
        page_table.unmap(vpn); //here it's OK to use temporary page_table, because in unmmap no frames removed, only change its content.
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

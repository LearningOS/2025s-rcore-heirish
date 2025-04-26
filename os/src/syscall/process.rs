//! Process management syscalls
use crate::{
    mm::{VPNRange, VirtAddr},
    timer::get_time_us,
};
use alloc::sync::Arc;

use crate::{
    loader::get_app_data_by_name,
    mm::{frame_alloc, translated_refmut, translated_str, PTEFlags, PageTable},
    task::{
        add_task, current_task, current_task_map_one, current_user_token,
        exit_current_and_run_next, suspend_current_and_run_next,
    },
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.exec(data);
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!(
        "kernel::pid[{}] sys_waitpid [{}]",
        current_task().unwrap().pid.0,
        pid
    );
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_get_time NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let us = get_time_us();
    *translated_refmut(current_user_token(), ts) = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    0
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
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
            current_task_map_one(vpn, frame.ppn, pte_flags);
        } else {
            return -1;
        }
    }
    0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
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
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(data) = get_app_data_by_name(path.as_str()) {
        let task = current_task().unwrap();
        task.spawn(data) as isize
    } else {
        -1
    }
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().pid.0
    );
    if prio >= 2 {
        current_task().unwrap().set_priority(prio);
        prio
    } else {
        -1
    }
}

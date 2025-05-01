//! Implementation of  [`ProcessControlBlock`]

use super::id::RecycleAllocator;
use super::manager::insert_into_pid2process;
use super::TaskControlBlock;
use super::{add_task, SignalFlags};
use super::{pid_alloc, PidHandle};
use crate::fs::{File, Stdin, Stdout};
use crate::mm::{translated_refmut, MemorySet, KERNEL_SPACE};
use crate::sync::{Condvar, Mutex, Semaphore, UPSafeCell};
use crate::trap::{trap_handler, TrapContext};
use alloc::string::String;
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefMut;

/// Resource types
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SyncResourceType {
    ///mutex type
    Mutex = 0,
    ///semaphore type
    Semaphore = 1,
}
pub const RESOURCE_TYPES: usize = 2;

/// Process Control Block
pub struct ProcessControlBlock {
    /// immutable
    pub pid: PidHandle,
    /// mutable
    inner: UPSafeCell<ProcessControlBlockInner>,
}

/// Inner of Process Control Block
pub struct ProcessControlBlockInner {
    /// is zombie?
    pub is_zombie: bool,
    /// memory set(address space)
    pub memory_set: MemorySet,
    /// parent process
    pub parent: Option<Weak<ProcessControlBlock>>,
    /// children process
    pub children: Vec<Arc<ProcessControlBlock>>,
    /// exit code
    pub exit_code: i32,
    /// file descriptor table
    pub fd_table: Vec<Option<Arc<dyn File + Send + Sync>>>,
    /// signal flags
    pub signals: SignalFlags,
    /// tasks(also known as threads)
    pub tasks: Vec<Option<Arc<TaskControlBlock>>>,
    /// task resource allocator
    pub task_res_allocator: RecycleAllocator,
    /// mutex list
    pub mutex_list: Vec<Option<Arc<dyn Mutex>>>,
    /// semaphore list
    pub semaphore_list: Vec<Option<Arc<Semaphore>>>,
    /// condvar list
    pub condvar_list: Vec<Option<Arc<Condvar>>>,

    /// Note: how to handle the resource for exited thread, how to deallocate the resource?
    /// 1. as per the allocated/needed resource for exited thread, No need to take care of it.
    /// because a thread executing code wil make sure it's allocated and needed resource is 0 before exit, or else it will not able to exit.
    /// after thread exited, resource_allocated[:][tid][:] = 0, resource_needed[:][tid][:]= 0
    /// 2. as per available resource, for mutex create will inc 1, lock will dec 1, unlock will inc 1
    /// for semaphore, create will inc n, down will dec 1, up will inc 1
    /// so once resource created, it will be available till end of the process.
    /// deadlock detect enabled
    pub deadlock_detect_enabled: bool,
    /// mutex/semaphore resource available, [Vec::new(); RESOURCE_TYPES]
    pub resource_available: Vec<Vec<usize>>,
    /// mutex/semaphore resource allocated
    pub resource_allocated: Vec<Vec<Vec<usize>>>,
    /// mutex/semaphore resource allocated
    pub resource_needed: Vec<Vec<Vec<usize>>>,
}

impl ProcessControlBlockInner {
    #[allow(unused)]
    /// get the address of app's page table
    pub fn get_user_token(&self) -> usize {
        self.memory_set.token()
    }
    /// allocate a new file descriptor
    pub fn alloc_fd(&mut self) -> usize {
        if let Some(fd) = (0..self.fd_table.len()).find(|fd| self.fd_table[*fd].is_none()) {
            fd
        } else {
            self.fd_table.push(None);
            self.fd_table.len() - 1
        }
    }
    /// allocate a new task id
    pub fn alloc_tid(&mut self) -> usize {
        self.task_res_allocator.alloc()
    }
    /// deallocate a task id
    pub fn dealloc_tid(&mut self, tid: usize) {
        self.task_res_allocator.dealloc(tid)
    }
    /// the count of tasks(threads) in this process
    pub fn thread_count(&self) -> usize {
        self.tasks.len()
    }
    /// get a task with tid in this process
    pub fn get_task(&self, tid: usize) -> Arc<TaskControlBlock> {
        self.tasks[tid].as_ref().unwrap().clone()
    }
    /// add available resource
    pub fn inc_available_resource(
        &mut self,
        res_type: SyncResourceType,
        res_id: usize,
        res_count: usize,
    ) {
        let res_table = &mut self.resource_available[res_type as usize];
        if res_table.len() < res_id + 1 {
            res_table.resize(res_id + 1, 0);
        }
        res_table[res_id] += res_count;
    }
    //decrease avaialble resource
    fn dec_available_resource(
        &mut self,
        res_type: SyncResourceType,
        res_id: usize,
        res_count: usize,
    ) {
        let res_table = &mut self.resource_available[res_type as usize];
        if res_table.len() < res_id + 1 {
            res_table.resize(res_id + 1, 0);
        }
        res_table[res_id] -= res_count;
    }

    ///init task's resource allocated and needed to 0
    pub fn init_task_resouce_data(&mut self, tid: usize) {
        for i in 0..RESOURCE_TYPES as usize {
            let type_resource_allocated = &mut self.resource_allocated[i];
            let type_resource_needed = &mut self.resource_needed[i];

            assert_eq!(type_resource_allocated.len(), type_resource_needed.len());
            if type_resource_allocated.len() < tid + 1 {
                type_resource_allocated.resize(tid + 1, Vec::new());
                type_resource_needed.resize(tid + 1, Vec::new());
            }
        }
    }

    /// increase allocated resource
    pub fn inc_allocated_resource(
        &mut self,
        tid: usize,
        res_type: SyncResourceType,
        res_id: usize,
        res_count: usize,
    ) {
        //println!(
        //    "inc allocated resource :tid {}, res_type {}, res_id {}, res_count {}",
        //    tid, res_type as usize, res_id, res_count
        //);
        let thread_resource_allocated = &mut self.resource_allocated[res_type as usize][tid];
        if thread_resource_allocated.len() < res_id + 1 {
            thread_resource_allocated.resize(res_id + 1, 0);
        }
        thread_resource_allocated[res_id] += res_count;
        self.dec_available_resource(res_type, res_id, res_count);
    }

    /// decrease allocated resource
    pub fn dec_allocated_resource(
        &mut self,
        tid: usize,
        res_type: SyncResourceType,
        res_id: usize,
        res_count: usize,
    ) {
        //println!(
        //    "dec allocated resource :tid {}, res_type {}, res_id {}, res_count {}",
        //    tid, res_type as usize, res_id, res_count
        //);
        let task_allocated_resource = &mut self.resource_allocated[res_type as usize][tid];
        //handle unlock/up without lock/down use cases
        if res_id < task_allocated_resource.len() {
            let actual_count = res_count.min(task_allocated_resource[res_id]);
            self.resource_allocated[res_type as usize][tid][res_id] -= actual_count;
            self.inc_available_resource(res_type, res_id, actual_count);
        }
    }

    /// increase needed resource
    pub fn inc_needed_resource(
        &mut self,
        tid: usize,
        res_type: SyncResourceType,
        res_id: usize,
        res_count: usize,
    ) {
        //println!(
        //    "inc needed resource :tid {}, res_type {}, res_id {}, res_count {}",
        //    tid, res_type as usize, res_id, res_count
        //);
        let thread_resources_needed = &mut (self.resource_needed[res_type as usize][tid]);
        if thread_resources_needed.len() < res_id + 1 {
            thread_resources_needed.resize(res_id + 1, 0);
        }
        thread_resources_needed[res_id] += res_count;
    }

    /// decrease needed resource
    pub fn dec_needed_resource(
        &mut self,
        tid: usize,
        res_type: SyncResourceType,
        res_id: usize,
        res_count: usize,
    ) {
        //println!(
        //    "dec needed resource :tid {}, res_type {}, res_id {}, res_count {}",
        //    tid, res_type as usize, res_id, res_count
        //);
        self.resource_needed[res_type as usize][tid][res_id] -= res_count;
    }

    ///check mutex deadlock
    pub fn is_safe_acquire(&mut self, res_type: SyncResourceType) -> bool {
        if self.deadlock_detect_enabled {
            let type_resource_allocated = &self.resource_allocated[res_type as usize];
            let type_resource_needed = &self.resource_needed[res_type as usize];

            assert_eq!(self.tasks.len(), type_resource_allocated.len());
            assert_eq!(type_resource_allocated.len(), type_resource_needed.len());
            //step 1
            let mut work = self.resource_available[res_type as usize].clone();
            let mut finish = vec![false; self.tasks.len()];

            //println!("in is_safe_acquire, tid {}, type_resource_allocated: {:?}, type_resource_needed:{:?},work:{:?},finish:{:?}"
            //, tid, type_resource_allocated, type_resource_needed,work, finish);
            loop {
                //step 2
                let mut found = false;
                for task_id in 0..finish.len() {
                    let task_resource_needed = &type_resource_needed[task_id];
                    let task_resource_allocated = &type_resource_allocated[task_id];
                    if finish[task_id] == false
                        && task_resource_needed
                            .iter()
                            .enumerate()
                            .all(|(resource_id, &val)| val <= work[resource_id])
                    {
                        //step 3
                        let _: Vec<_> = task_resource_allocated
                            .iter()
                            .enumerate()
                            .map(|(resource_id, &resource_count)| {
                                work[resource_id] += resource_count;
                            })
                            .collect();
                        finish[task_id] = true;
                        found = true;
                        //println!("in is_safe_acquire, step 3: type_resource_allocated: {:?}, type_resource_needed:{:?},work:{:?},finish:{:?}"
                        //, type_resource_allocated, type_resource_needed,work, finish);
                    }
                }
                //step 4
                if found == false {
                    break;
                }
            }
            //step 4
            let is_safe = finish.iter().all(|&val| val);
            //println!("is safe?{}", is_safe);
            is_safe
        } else {
            true
        }
    }
}

impl ProcessControlBlock {
    /// inner_exclusive_access
    pub fn inner_exclusive_access(&self) -> RefMut<'_, ProcessControlBlockInner> {
        self.inner.exclusive_access()
    }
    /// new process from elf file
    pub fn new(elf_data: &[u8]) -> Arc<Self> {
        trace!("kernel: ProcessControlBlock::new");
        // memory_set with elf program headers/trampoline/trap context/user stack
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        // allocate a pid
        let pid_handle = pid_alloc();
        let process = Arc::new(Self {
            pid: pid_handle,
            inner: unsafe {
                UPSafeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: None,
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: vec![
                        // 0 -> stdin
                        Some(Arc::new(Stdin)),
                        // 1 -> stdout
                        Some(Arc::new(Stdout)),
                        // 2 -> stderr
                        Some(Arc::new(Stdout)),
                    ],
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),

                    deadlock_detect_enabled: false,
                    resource_available: vec![Vec::new(); RESOURCE_TYPES],
                    resource_allocated: vec![Vec::new(); RESOURCE_TYPES], //thread 0, main thread
                    resource_needed: vec![Vec::new(); RESOURCE_TYPES],    //thread 0, main thread
                })
            },
        });
        // create a main thread, we should allocate ustack and trap_cx here
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&process),
            ustack_base,
            true,
        ));
        // prepare trap_cx of main thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        let ustack_top = task_inner.res.as_ref().unwrap().ustack_top();
        let kstack_top = task.kstack.get_top();
        drop(task_inner);
        *trap_cx = TrapContext::app_init_context(
            entry_point,
            ustack_top,
            KERNEL_SPACE.exclusive_access().token(),
            kstack_top,
            trap_handler as usize,
        );
        // add main thread to the process
        let mut process_inner = process.inner_exclusive_access();
        process_inner.tasks.push(Some(Arc::clone(&task)));
        process_inner.init_task_resouce_data(
            task.as_ref()
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .tid,
        );
        drop(process_inner);
        insert_into_pid2process(process.getpid(), Arc::clone(&process));
        // add main thread to scheduler
        add_task(task);
        process
    }

    /// Only support processes with a single thread.
    pub fn exec(self: &Arc<Self>, elf_data: &[u8], args: Vec<String>) {
        trace!("kernel: exec");
        assert_eq!(self.inner_exclusive_access().thread_count(), 1);
        // memory_set with elf program headers/trampoline/trap context/user stack
        trace!("kernel: exec .. MemorySet::from_elf");
        let (memory_set, ustack_base, entry_point) = MemorySet::from_elf(elf_data);
        let new_token = memory_set.token();
        // substitute memory_set
        trace!("kernel: exec .. substitute memory_set");
        self.inner_exclusive_access().memory_set = memory_set;
        // then we alloc user resource for main thread again
        // since memory_set has been changed
        trace!("kernel: exec .. alloc user resource for main thread again");
        let task = self.inner_exclusive_access().get_task(0);
        let mut task_inner = task.inner_exclusive_access();
        task_inner.res.as_mut().unwrap().ustack_base = ustack_base;
        task_inner.res.as_mut().unwrap().alloc_user_res();
        task_inner.trap_cx_ppn = task_inner.res.as_mut().unwrap().trap_cx_ppn();
        // push arguments on user stack
        trace!("kernel: exec .. push arguments on user stack");
        let mut user_sp = task_inner.res.as_mut().unwrap().ustack_top();
        user_sp -= (args.len() + 1) * core::mem::size_of::<usize>();
        let argv_base = user_sp;
        let mut argv: Vec<_> = (0..=args.len())
            .map(|arg| {
                translated_refmut(
                    new_token,
                    (argv_base + arg * core::mem::size_of::<usize>()) as *mut usize,
                )
            })
            .collect();
        *argv[args.len()] = 0;
        for i in 0..args.len() {
            user_sp -= args[i].len() + 1;
            *argv[i] = user_sp;
            let mut p = user_sp;
            for c in args[i].as_bytes() {
                *translated_refmut(new_token, p as *mut u8) = *c;
                p += 1;
            }
            *translated_refmut(new_token, p as *mut u8) = 0;
        }
        // make the user_sp aligned to 8B for k210 platform
        user_sp -= user_sp % core::mem::size_of::<usize>();
        // initialize trap_cx
        trace!("kernel: exec .. initialize trap_cx");
        let mut trap_cx = TrapContext::app_init_context(
            entry_point,
            user_sp,
            KERNEL_SPACE.exclusive_access().token(),
            task.kstack.get_top(),
            trap_handler as usize,
        );
        trap_cx.x[10] = args.len();
        trap_cx.x[11] = argv_base;
        *task_inner.get_trap_cx() = trap_cx;
    }

    /// Only support processes with a single thread.
    pub fn fork(self: &Arc<Self>) -> Arc<Self> {
        trace!("kernel: fork");
        let mut parent = self.inner_exclusive_access();
        assert_eq!(parent.thread_count(), 1);
        // clone parent's memory_set completely including trampoline/ustacks/trap_cxs
        let memory_set = MemorySet::from_existed_user(&parent.memory_set);
        // alloc a pid
        let pid = pid_alloc();
        // copy fd table
        let mut new_fd_table: Vec<Option<Arc<dyn File + Send + Sync>>> = Vec::new();
        for fd in parent.fd_table.iter() {
            if let Some(file) = fd {
                new_fd_table.push(Some(file.clone()));
            } else {
                new_fd_table.push(None);
            }
        }
        // create child process pcb
        let child = Arc::new(Self {
            pid,
            inner: unsafe {
                UPSafeCell::new(ProcessControlBlockInner {
                    is_zombie: false,
                    memory_set,
                    parent: Some(Arc::downgrade(self)),
                    children: Vec::new(),
                    exit_code: 0,
                    fd_table: new_fd_table,
                    signals: SignalFlags::empty(),
                    tasks: Vec::new(),
                    task_res_allocator: RecycleAllocator::new(),
                    mutex_list: Vec::new(),
                    semaphore_list: Vec::new(),
                    condvar_list: Vec::new(),

                    deadlock_detect_enabled: false,
                    resource_available: vec![Vec::new(); RESOURCE_TYPES],
                    resource_allocated: vec![Vec::new(); RESOURCE_TYPES], //thread 0, main thread
                    resource_needed: vec![Vec::new(); RESOURCE_TYPES],    //thread 0, main thread
                })
            },
        });
        // add child
        parent.children.push(Arc::clone(&child));
        // create main thread of child process
        let task = Arc::new(TaskControlBlock::new(
            Arc::clone(&child),
            parent
                .get_task(0)
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .ustack_base(),
            // here we do not allocate trap_cx or ustack again
            // but mention that we allocate a new kstack here
            false,
        ));
        // attach task to child process
        let mut child_inner = child.inner_exclusive_access();
        child_inner.tasks.push(Some(Arc::clone(&task)));
        child_inner.init_task_resouce_data(
            task.as_ref()
                .inner_exclusive_access()
                .res
                .as_ref()
                .unwrap()
                .tid,
        );
        drop(child_inner);
        // modify kstack_top in trap_cx of this thread
        let task_inner = task.inner_exclusive_access();
        let trap_cx = task_inner.get_trap_cx();
        trap_cx.kernel_sp = task.kstack.get_top();
        drop(task_inner);
        insert_into_pid2process(child.getpid(), Arc::clone(&child));
        // add this thread to scheduler
        add_task(task);
        child
    }
    /// get pid
    pub fn getpid(&self) -> usize {
        self.pid.0
    }
}

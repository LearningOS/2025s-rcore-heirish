### 实现总结
- 添加资源类型SyncResourceType:用以标识资源是Mutex还是Semaphore
- 在ProcessControlBlock中添加4个数据结构
    - deadlock_detect_enabled:标识死锁检测是否开启
    - resource_available:二维数组[resource_type][resource_id], 最后一维按需分配，未对齐
    - resource_allocated:三维数组[resource_type][task_id][resource_id], 最后一维按需分配，未对齐
    - resource_needed:三维数组[resource_type][task_id][resource_id], 最后一维按需分配，未对齐
    - 如果算法运行正确。不需要take care of task 退出时数据的状态
    - 即使没有开启死锁检查，在mutex/semaphore create/acquire/release时也要维护这几个数据结构, 以支持同一个进程多次enable/disable deadlock detector.
- 死锁检查相关流程执行过程
    - create thread: init data in resource_allocated and resource_needed
    - create mutex/semaphore: init data in resource_available
    - mutex/semaphore acquire
        - increase needed in resource_needed[resource_type][curr_task_id]
        - check whether it's safe to assign resource to currrent task
        - if it's safe, do mutex lock/donw, then increase resource allocated, decrease available resource and decrease needed resource needed; otherwise do nothing.
    - mutex/semaphore release
        - decrease allocated resource and increase available resource.
- 检测算法:学习银行家算法并按算法实现即可,需要注意的是检查is safe时，针对整个资源的情况进行判断，并非只对当前task当前申请的resource_id,所以在is_safe_acquire中是遍历了所有task的所有resource.针对某个task, 只有当其所有的resource能被满足时，才能设置其finish为true. 对整个进程来说，只有所有task的finish都为true时，才安全。
### 问答题
- 1. 在我们的多线程实现中，当主线程 (即 0 号线程) 退出时，视为整个进程退出， 此时需要结束该进程管理的所有线程并回收其资源。 
    - 需要回收的资源有哪些？ 
      > TaskUserRes， TaskContext
    - 其他线程的 TaskControlBlock 可能在哪些位置被引用，分别是否需要回收，为什么？
      > 在申请的mutex, semaphore,condvar的阻塞队列中,不需要，整个进程能退出的前提是所有线程都没有被阻塞，因此这些队列在进程退出前己清空 
- 2.对比以下两种 Mutex 中的实现，二者有什么区别？这些区别可能会导致什么问题？
```
 impl Mutex for Mutex1 {
     fn lock(&self) {
         loop {
             let mut mutex_inner = self.inner.exclusive_access();
             if mutex_inner.locked {
                 mutex_inner.wait_queue.push_back(current_task().unwrap());
                 drop(mutex_inner);
                 block_current_and_run_next();
             } else {
                mutex_inner.locked = true;
                break;
            }
        }
    }

    fn unlock(&self) {
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        mutex_inner.locked = false;
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            add_task(waking_task);
        }
    }
}

impl Mutex for Mutex2 {
    fn lock(&self) {
        let mut mutex_inner = self.inner.exclusive_access();
        if mutex_inner.locked {
            mutex_inner.wait_queue.push_back(current_task().unwrap());
            drop(mutex_inner);
            block_current_and_run_next();
        } else {
            mutex_inner.locked = true;
        }
    }

    fn unlock(&self) {
        let mut mutex_inner = self.inner.exclusive_access();
        assert!(mutex_inner.locked);
        if let Some(waking_task) = mutex_inner.wait_queue.pop_front() {
            add_task(waking_task);
        } else {
            mutex_inner.locked = false;
        }
    }
}
```
### 荣誉准则
1. 在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：
   > NA
2. 此外，我也参考了以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：
   > rCore-Tutorial-Guide-2025S文档
3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。
4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。